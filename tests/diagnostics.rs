use std::path::Path;
use std::process::{Command, Output};

fn child(directory: &Path, case: &str) -> Output {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "diagnostics_child", "--nocapture"])
        .env("TRY_DIAGNOSTICS_TEST_DIR", directory)
        .env("TRY_DIAGNOSTICS_TEST_CASE", case)
        .env_remove("TRY_LOG");
    if case == "invalid_filter" {
        command.env("TRY_LOG", "try=not_a_level");
    }
    command.output().unwrap()
}

fn records(directory: &Path) -> String {
    std::fs::read_dir(directory.join("logs"))
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "log")
        })
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect()
}

#[test]
fn normal_logging_filters_debug_and_diagnostic_mode_keeps_it() {
    for case in ["normal", "debug"] {
        let directory = tempfile::tempdir().unwrap();
        let output = child(directory.path(), case);
        assert!(output.status.success(), "{output:?}");
        let text = records(directory.path());
        assert!(text.contains("event=run_started version="));
        assert!(text.contains(" git=") && text.contains(" worktree=") && text.contains(" run="));
        assert!(text.contains("event=probe_info"));
        assert_eq!(text.contains("event=probe_debug"), case == "debug");
    }
}

#[test]
fn invalid_filter_falls_back_without_losing_records() {
    let directory = tempfile::tempdir().unwrap();
    assert!(child(directory.path(), "invalid_filter").status.success());
    let text = records(directory.path());
    assert!(text.contains("event=invalid_log_filter"));
    assert!(text.contains("event=probe_info"));
}

#[test]
fn unwritable_log_directory_falls_back_without_failing_the_process() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("logs"), "occupied by a file").unwrap();
    let output = child(directory.path(), "normal");
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("event=logging_fallback"));
    assert!(stderr.contains("event=probe_info"));
}

#[test]
fn panic_record_survives_a_failed_child_and_restart_preserves_previous_records() {
    let directory = tempfile::tempdir().unwrap();
    assert!(!child(directory.path(), "panic").status.success());
    let before = records(directory.path());
    assert!(before.contains("event=panic"));
    assert!(before.contains("diagnostics probe panic"));
    assert!(before.contains("location=") && before.contains("backtrace="));
    assert!(child(directory.path(), "normal").status.success());
    assert!(records(directory.path()).contains(&before));
}

#[test]
#[ignore = "isolated process helper for diagnostics integration tests"]
fn diagnostics_child() {
    let directory = std::env::var_os("TRY_DIAGNOSTICS_TEST_DIR").unwrap();
    let case = std::env::var("TRY_DIAGNOSTICS_TEST_CASE").unwrap();
    let handle = r#try::diagnostics::initialize(Path::new(&directory), case == "debug").unwrap();
    log::info!(target: "try::probe", "event=probe_info");
    log::debug!(target: "try::probe", "event=probe_debug");
    if case == "panic" {
        panic!("diagnostics probe panic");
    }
    handle.shutdown();
}
