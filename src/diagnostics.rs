use flexi_logger::{
    Cleanup, Criterion, DeferredNow, FileSpec, LogSpecification, Logger, LoggerHandle, Naming,
    WriteMode,
};
use std::io::{self, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const OLD_LOG_FILES: usize = 3;
const MAX_MESSAGE_CHARS: usize = 8192;
static RUN_ID: OnceLock<String> = OnceLock::new();

/// Initialize once, after acquiring the profile lock and before loading data or opening UI.
/// Keep the handle until the final exit/error record has been written, then call shutdown().
pub fn initialize(data_directory: &Path, verbose: bool) -> Option<LoggerHandle> {
    let default_spec = if verbose {
        "warn,try=debug"
    } else {
        "warn,try=info"
    };
    let (spec, invalid_filter) = specification(default_spec);
    let directory = data_directory.join("logs");
    let handle = match file_logger(&directory, spec.clone(), MAX_LOG_BYTES, OLD_LOG_FILES).start() {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("Try: 无法写入日志，改用 stderr：{error}");
            match Logger::with(spec)
                .format(format_record)
                .write_mode(WriteMode::Direct)
                .start()
            {
                Ok(handle) => {
                    log::warn!(target: "try::diagnostics", "event=logging_fallback reason={error}");
                    handle
                }
                Err(error) => {
                    eprintln!("Try: 无法初始化日志：{error}");
                    return None;
                }
            }
        }
    };
    install_panic_hook();
    log::info!(target: "try::diagnostics",
        "event=run_started version={} git={} worktree={} os={} arch={} diagnostics={verbose}",
        env!("CARGO_PKG_VERSION"),
        env!("TRY_GIT_REVISION"),
        env!("TRY_GIT_STATE"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    if invalid_filter {
        log::warn!(target: "try::diagnostics", "event=invalid_log_filter fallback={default_spec}");
    }
    Some(handle)
}

fn specification(default_spec: &str) -> (LogSpecification, bool) {
    match std::env::var("TRY_LOG") {
        Ok(value) if !value.trim().is_empty() => match LogSpecification::parse(&value) {
            Ok(spec) => (spec, false),
            Err(_) => (LogSpecification::parse(default_spec).unwrap(), true),
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            (LogSpecification::parse(default_spec).unwrap(), true)
        }
        _ => (LogSpecification::parse(default_spec).unwrap(), false),
    }
}

/// Append a helper event while the profile lock excludes the application's
/// logger. Close this local writer before releasing the lock or starting a UI.
/// Logging is best-effort and must never change installation or recovery.
pub(crate) fn record_update(
    store: &crate::storage::Store,
    level: log::Level,
    message: std::fmt::Arguments<'_>,
) {
    let (spec, _) = specification("warn,try=info");
    let Ok((logger, handle)) = file_logger(
        &store.directory().join("logs"),
        spec,
        MAX_LOG_BYTES,
        OLD_LOG_FILES,
    )
    .build() else {
        return;
    };
    let record = log::Record::builder()
        .target("try::updates")
        .level(level)
        .args(message)
        .build();
    if logger.enabled(record.metadata()) {
        logger.log(&record);
    }
    handle.shutdown();
}

fn file_logger(
    directory: &Path,
    spec: LogSpecification,
    max_bytes: u64,
    old_files: usize,
) -> Logger {
    Logger::with(spec)
        .log_to_file(
            FileSpec::default()
                .directory(directory)
                .basename("try")
                .suppress_timestamp(),
        )
        .append()
        .format(format_record)
        .write_mode(WriteMode::Direct)
        .rotate(
            Criterion::Size(max_bytes),
            Naming::Numbers,
            Cleanup::KeepLogFiles(old_files),
        )
        .cleanup_in_background_thread(false)
}

fn format_record(
    writer: &mut dyn Write,
    now: &mut DeferredNow,
    record: &log::Record<'_>,
) -> io::Result<()> {
    let run = RUN_ID.get_or_init(|| {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("{}-{start:x}", std::process::id())
    });
    // Keep one physical line per event and bound unusually large errors/panic messages.
    let message = record.args().to_string();
    let bounded: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    let bounded = bounded.replace('\r', "\\r").replace('\n', "\\n");
    write!(
        writer,
        "{} {:5} run={run} {} {bounded}",
        now.now_utc_owned().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        record.level(),
        record.target()
    )
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic");
        log::error!(target: "try::diagnostics",
            "event=panic thread={:?} location={:?} message={payload:?} backtrace={}",
            thread.name(),
            info.location(),
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_bounded_and_restart_appends_without_touching_other_files() {
        let directory = tempfile::tempdir().unwrap();
        let other = directory.path().join("unrelated.txt");
        std::fs::write(&other, "preserve").unwrap();
        {
            let (logger, handle) = file_logger(
                directory.path(),
                LogSpecification::parse("info").unwrap(),
                1024,
                3,
            )
            .build()
            .unwrap();
            for index in 0..100 {
                logger.log(
                    &log::Record::builder()
                        .level(log::Level::Info)
                        .target("try::test")
                        .args(format_args!(
                            "event=rotation index={index} {}",
                            "x".repeat(128)
                        ))
                        .build(),
                );
            }
            logger.flush();
            handle.shutdown();
        }
        let before = std::fs::read_to_string(directory.path().join("try_rCURRENT.log")).unwrap();
        {
            let (logger, handle) = file_logger(
                directory.path(),
                LogSpecification::parse("info").unwrap(),
                1024,
                3,
            )
            .build()
            .unwrap();
            logger.log(
                &log::Record::builder()
                    .level(log::Level::Info)
                    .target("try::test")
                    .args(format_args!("event=restart"))
                    .build(),
            );
            logger.flush();
            handle.shutdown();
        }
        let logs: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "log")
            })
            .collect();
        assert_eq!(logs.len(), 4);
        let after = logs
            .iter()
            .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
            .collect::<String>();
        assert!(after.contains(&before));
        assert!(after.contains("event=restart"));
        assert_eq!(std::fs::read_to_string(other).unwrap(), "preserve");
    }
}
