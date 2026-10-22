//! One-shot update helper, transactional replacement and startup acknowledgement.
//! The helper is a temporary copy of Try; no extra program ships with a release.
use crate::knowledge::AppResult;
use crate::storage::{Store, write_atomic};
use crate::tr;
use crate::update_check::Release;
use crate::updater::{
    PackageKind, PreparedUpdate, digest_file, executable_permissions, payload_executable,
};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PROFILE_FILES: [&str; 3] = [
    "state.json",
    "learning_state.json",
    "update_check_state.json",
];
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Job {
    schema: u32,
    tag: String,
    kind: PackageKind,
    target: PathBuf,
    launch_target: PathBuf,
    data: PathBuf,
    parent_pid: u32,
    nonce: String,
    payload_sha256: String,
    helper_sha256: String,
    old_sha256: String,
    saved_files: Vec<String>,
}

fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

fn read_job(path: &Path) -> AppResult<(Job, PathBuf)> {
    if path.file_name().and_then(|p| p.to_str()) != Some("job.json") {
        return Err("invalid_update_job".into());
    }
    let root =
        fs::canonicalize(path.parent().ok_or("invalid_update_job")?).map_err(|e| e.to_string())?;
    let name = root
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("invalid_update_stage")?;
    if !name.starts_with(".try_update_") || name.len() <= 12 {
        return Err("invalid_update_stage".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(32 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 {
        return Err("invalid_update_job".into());
    }
    let mut job: Job = serde_json::from_slice(&bytes).map_err(|_| "invalid_update_job")?;
    job.target = fs::canonicalize(&job.target).map_err(|e| e.to_string())?;
    job.data = fs::canonicalize(&job.data).map_err(|e| e.to_string())?;
    if job.schema != 1
        || Release::from_tag(&job.tag).is_err()
        || job.parent_pid == 0
        || !hex(&job.nonce)
        || !hex(&job.payload_sha256)
        || !hex(&job.helper_sha256)
        || !hex(&job.old_sha256)
        || !job.target.is_absolute()
        || fs::canonicalize(&job.launch_target).map_err(|e| e.to_string())? != job.target
        || !job.data.is_absolute()
        || job
            .target
            .parent()
            .and_then(|p| fs::canonicalize(p).ok())
            .as_deref()
            != root.parent()
        || job.target == root
        || job.data.starts_with(&root)
        || (job.kind == PackageKind::Macos && job.data.starts_with(&job.target))
        || job.saved_files.len() > PROFILE_FILES.len()
        || job
            .saved_files
            .iter()
            .any(|p| !PROFILE_FILES.contains(&p.as_str()))
    {
        return Err("invalid_update_job".into());
    }
    Ok((job, root))
}

fn save_job(job: &Job, root: &Path) -> AppResult<()> {
    write_atomic(
        &root.join("job.json"),
        &serde_json::to_vec(job).map_err(|e| e.to_string())?,
    )
}

fn record_install_failure(data: &Path, tag: &str, stage: &str, error: &str) {
    if let Ok(store) = Store::open(data.to_owned()) {
        crate::diagnostics::record_update(
            &store,
            log::Level::Warn,
            format_args!("event=install_failed target={tag} stage={stage} reason={error}"),
        );
    }
}

pub fn record_handoff_failure(data: &Path, release: &Release, error: &str) {
    record_install_failure(data, release.tag(), "handoff", error);
}

fn clean_appimage_environment(command: &mut Command) {
    if std::env::var_os("APPIMAGE").is_some() {
        for key in ["APPIMAGE", "APPDIR", "OWD", "ARGV0", "LD_LIBRARY_PATH"] {
            command.env_remove(key);
        }
    }
}

fn hidden(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

impl PreparedUpdate {
    /// Called only after the UI, data lock and log handle have been closed.
    pub fn launch(self, data: &Path) -> AppResult<()> {
        let data = fs::canonicalize(data).map_err(|e| e.to_string())?;
        if self.installation.kind == PackageKind::Macos
            && data.starts_with(&self.installation.target)
        {
            return Err(tr!("数据目录位于应用包内，请先移出再更新。").into());
        }
        let root = self.directory.path();
        let helper = root.join(self.installation.kind.helper());
        fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &helper)
            .map_err(|e| e.to_string())?;
        executable_permissions(&helper)?;
        let nonce: String = rand::random::<[u8; 32]>()
            .iter()
            .map(|n| format!("{n:02x}"))
            .collect();
        let job = Job {
            schema: 1,
            tag: self.release.tag().to_owned(),
            kind: self.installation.kind,
            old_sha256: digest_file(&payload_executable(
                self.installation.kind,
                &self.installation.target,
            ))?,
            launch_target: if self.installation.kind == PackageKind::Windows {
                std::env::current_exe().map_err(|e| e.to_string())?
            } else {
                self.installation.target.clone()
            },
            target: self.installation.target,
            data,
            parent_pid: std::process::id(),
            nonce,
            payload_sha256: self.payload_sha256,
            helper_sha256: digest_file(&helper)?,
            saved_files: vec![],
        };
        save_job(&job, root)?;
        let mut command = Command::new(&helper);
        command
            .arg("--apply-update")
            .arg(root.join("job.json"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .current_dir(root);
        hidden(&mut command);
        // The copied Linux binary can still need the old bundle's already-known
        // libraries while its helper starts. Only the new AppImage clears them.
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let mut output = child.stdout.take().ok_or("update_helper_stdout")?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = Vec::new();
            let result = output.by_ref().take(17).read_to_end(&mut line);
            let _ = sender.send(result.is_ok() && line == b"try-update-ready\n");
        });
        if receiver.recv_timeout(Duration::from_secs(10)) != Ok(true) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(tr!("无法启动更新，请重试。").into());
        }
        let result = child
            .stdin
            .take()
            .ok_or("update_helper_stdin")?
            .write_all(job.nonce.as_bytes())
            .map_err(|e| e.to_string());
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return result;
        }
        // The helper now owns cleanup. Keep staging through parent process exit.
        let _ = self.directory.keep();
        Ok(())
    }
}

/// Dispatch before data loading or GUI initialization; this copy never opens a tray.
pub fn helper_mode(arguments: &[OsString]) -> Option<AppResult<()>> {
    if arguments.first().is_none_or(|s| s != "--apply-update") {
        return None;
    }
    Some(if arguments.len() == 2 {
        run_helper(Path::new(&arguments[1]))
    } else {
        Err("invalid_update_arguments".into())
    })
}

fn run_helper(path: &Path) -> AppResult<()> {
    let (mut job, root) = read_job(path)?;
    if fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        != fs::canonicalize(root.join(job.kind.helper())).map_err(|e| e.to_string())?
        || digest_file(&root.join(job.kind.helper()))? != job.helper_sha256
        || digest_file(&payload_executable(
            job.kind,
            &root.join(job.kind.payload()),
        ))? != job.payload_sha256
        || digest_file(&payload_executable(job.kind, &job.target))? != job.old_sha256
    {
        return Err("update_source_changed".into());
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("helper.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock().map_err(|e| e.to_string())?;
    write_atomic(
        &root.join("helper_pid"),
        std::process::id().to_string().as_bytes(),
    )?;
    let parent = Parent::open(job.parent_pid)?;
    std::io::stdout()
        .write_all(b"try-update-ready\n")
        .map_err(|e| e.to_string())?;
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut input = String::new();
    std::io::stdin()
        .take(65)
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    if input != job.nonce {
        return Err("update_handoff_cancelled".into());
    }
    parent.wait()?;
    let result = apply_and_start(&mut job, &root);
    if let Err(error) = &result {
        // Once acknowledged, this helper is responsible for a visible recovery.
        let _ = write_atomic(&root.join("failure.txt"), error.as_bytes());
        if !root.join("previous").exists() && !root.join("rolled_back").exists() {
            let _ = start_application(&job, &root, false);
        } else if root.join("previous").exists() {
            rfd::MessageDialog::new()
                .set_title(tr!("Try · 更新未完成"))
                .set_description(tr!("旧程序和数据备份保留在：{path}", path = root.display()))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
    result
}

fn snapshot(job: &mut Job, root: &Path) -> AppResult<()> {
    let directory = root.join("profile_backup");
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    for name in PROFILE_FILES {
        let source = job.data.join(name);
        if source.exists() {
            let metadata = fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 64 * 1024 * 1024
            {
                return Err("invalid_update_profile".into());
            }
            fs::copy(source, directory.join(name)).map_err(|e| e.to_string())?;
            job.saved_files.push(name.into());
        }
    }
    save_job(job, root)
}

fn replace(job: &Job, root: &Path) -> AppResult<()> {
    let previous = root.join("previous");
    if previous.exists() {
        return Err("update_backup_already_exists".into());
    }
    fs::rename(&job.target, &previous).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(root.join(job.kind.payload()), &job.target) {
        fs::rename(&previous, &job.target)
            .map_err(|restore| format!("{error}; restore={restore}"))?;
        return Err(error.to_string());
    }
    Ok(())
}

fn restore(job: &Job, root: &Path) -> AppResult<()> {
    // Never overwrite a profile in use by another process.
    let profile = Store::open(job.data.clone())?;
    crate::diagnostics::record_update(
        &profile,
        log::Level::Info,
        format_args!("event=rollback_started target={}", job.tag),
    );
    let result = (|| {
        let previous = root.join("previous");
        if previous.exists() {
            if job.target.exists() {
                fs::rename(&job.target, root.join("failed")).map_err(|e| e.to_string())?;
            }
            fs::rename(previous, &job.target).map_err(|e| e.to_string())?;
        }
        for name in PROFILE_FILES {
            let destination = job.data.join(name);
            if job.saved_files.iter().any(|s| s == name) {
                let bytes =
                    fs::read(root.join("profile_backup").join(name)).map_err(|e| e.to_string())?;
                write_atomic(&destination, &bytes)?;
            } else if destination.is_file() {
                fs::remove_file(destination).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();
    match &result {
        Ok(()) => crate::diagnostics::record_update(
            &profile,
            log::Level::Info,
            format_args!("event=rollback_succeeded target={}", job.tag),
        ),
        Err(error) => crate::diagnostics::record_update(
            &profile,
            log::Level::Warn,
            format_args!("event=rollback_failed target={} reason={error}", job.tag),
        ),
    }
    result
}

fn start_application(job: &Job, root: &Path, updated: bool) -> AppResult<Child> {
    // Preserve the original launch spelling (not Windows' canonical \\?\ path),
    // so existing shortcuts and login registration keep the same executable path.
    let executable = payload_executable(job.kind, &job.launch_target);
    let mut command = Command::new(executable);
    command
        .arg("--data-dir")
        .arg(&job.data)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(job.target.parent().ok_or("invalid_update_target")?);
    if job.kind == PackageKind::Linux {
        clean_appimage_environment(&mut command);
    }
    if updated {
        command
            .arg("--complete-update")
            .arg(root.join("job.json"))
            .arg(&job.nonce);
    } else {
        command.arg("--update-failed");
    }
    hidden(&mut command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    if let Err(error) = write_atomic(
        &root.join(if updated { "child_pid" } else { "rollback_pid" }),
        child.id().to_string().as_bytes(),
    ) {
        stop_application(&mut child);
        return Err(error);
    }
    Ok(child)
}

fn stop_application(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Ok(pid) = i32::try_from(child.id())
            && pid > 1
        {
            // SAFETY: start_application created a separate process group for
            // this owned child, including the AppImage's extracted process.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn apply_and_start(job: &mut Job, root: &Path) -> AppResult<()> {
    // Multiple data profiles can share one installation. Keep one stable lock
    // file across replacements so their helpers cannot overwrite each other.
    let installation_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(
            job.target
                .parent()
                .ok_or("invalid_update_target")?
                .join(".try_update.lock"),
        )
        .map_err(|e| e.to_string())?;
    installation_lock
        .try_lock()
        .map_err(|_| "another_update_running")?;
    if digest_file(&payload_executable(job.kind, &job.target))? != job.old_sha256 {
        return Err("update_source_changed".into());
    }
    {
        let profile = Store::open(job.data.clone())?;
        crate::diagnostics::record_update(
            &profile,
            log::Level::Info,
            format_args!(
                "event=install_started target={} helper_version={}",
                job.tag,
                env!("CARGO_PKG_VERSION")
            ),
        );
        snapshot(job, root).inspect_err(|error| {
            crate::diagnostics::record_update(
                &profile,
                log::Level::Warn,
                format_args!(
                    "event=install_failed target={} stage=backup reason={error}",
                    job.tag
                ),
            );
        })?;
        replace(job, root).inspect_err(|error| {
            crate::diagnostics::record_update(
                &profile,
                log::Level::Warn,
                format_args!(
                    "event=install_failed target={} stage=replace reason={error}",
                    job.tag
                ),
            );
        })?;
    }
    let mut child = match start_application(job, root, true) {
        Ok(child) => child,
        Err(error) => {
            record_install_failure(&job.data, &job.tag, "startup", &error);
            restore(job, root)?;
            start_application(job, root, false)?;
            write_atomic(&root.join("rolled_back"), job.nonce.as_bytes())?;
            return Err(error);
        }
    };
    let attempt: AppResult<()> = (|| {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("updated_application_exited".into());
            }
            if fs::read(root.join("ready")).is_ok_and(|v| v == job.nonce.as_bytes()) {
                write_atomic(&root.join("committed"), job.nonce.as_bytes())?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("updated_application_startup_timeout".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })();
    if let Err(error) = attempt {
        stop_application(&mut child);
        record_install_failure(&job.data, &job.tag, "startup_confirmation", &error);
        restore(job, root).map_err(|restore| format!("{error}; rollback={restore}"))?;
        start_application(job, root, false)?;
        write_atomic(&root.join("rolled_back"), job.nonce.as_bytes())?;
        return Err(error);
    }
    Ok(())
}

/// Acknowledge only after normal data loading, windows and tray initialization.
pub fn acknowledge(arguments: &[OsString], data: &Path) -> AppResult<()> {
    let Some(index) = arguments.iter().position(|s| s == "--complete-update") else {
        return Ok(());
    };
    let path = arguments.get(index + 1).ok_or("invalid_update_receipt")?;
    let token = arguments
        .get(index + 2)
        .and_then(|s| s.to_str())
        .ok_or("invalid_update_receipt")?;
    let (job, root) = read_job(Path::new(path))?;
    let current = crate::updater::Installation::current()?.canonical()?;
    if job.nonce != token
        || current.kind != job.kind
        || current.target != job.target
        || fs::canonicalize(data).map_err(|e| e.to_string())? != job.data
        || Release::from_tag(&job.tag)
            .map_err(|e| e.to_string())?
            .version
            != semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
        || digest_file(&payload_executable(job.kind, &job.target))? != job.payload_sha256
    {
        return Err("invalid_update_receipt".into());
    }
    write_atomic(&root.join("ready"), job.nonce.as_bytes())?;
    std::thread::Builder::new()
        .name("try_update_cleanup".into())
        .spawn(move || {
            // Remove only this validated sibling staging directory, after the helper
            // has committed and exited. Windows may retain its mapped helper briefly.
            let mut committed = false;
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(200));
                if fs::read(root.join("committed")).is_ok_and(|v| v == job.nonce.as_bytes())
                    && let Ok(lock) = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(root.join("helper.lock"))
                    && lock.try_lock().is_ok()
                {
                    drop(lock);
                    committed = true;
                    break;
                }
            }
            if committed {
                for _ in 0..50 {
                    if fs::remove_dir_all(&root).is_ok() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Suppress a blocking startup dialog so the helper can detect failure and recover.
pub fn completing(arguments: &[OsString]) -> bool {
    arguments.iter().any(|s| s == "--complete-update")
}

#[cfg(target_os = "windows")]
struct Parent(windows_sys::Win32::Foundation::HANDLE);
#[cfg(target_os = "windows")]
impl Parent {
    fn open(pid: u32) -> AppResult<Self> {
        // SAFETY: request only synchronization rights for the explicit parent PID.
        let handle = unsafe {
            windows_sys::Win32::System::Threading::OpenProcess(
                windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            Err("update_parent_missing".into())
        } else {
            Ok(Self(handle))
        }
    }
    fn wait(&self) -> AppResult<()> {
        // SAFETY: this guard owns a synchronization-capable process handle.
        if unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 30_000) }
            != windows_sys::Win32::Foundation::WAIT_OBJECT_0
        {
            return Err("update_parent_timeout".into());
        }
        Ok(())
    }
}
#[cfg(target_os = "windows")]
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(target_os = "windows"))]
struct Parent;
#[cfg(not(target_os = "windows"))]
impl Parent {
    fn open(_: u32) -> AppResult<Self> {
        Ok(Self)
    }
    // stdin reaches EOF only after the old UI and data lock have been released.
    fn wait(&self) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(directory: &Path) -> (Job, tempfile::TempDir) {
        let root = tempfile::Builder::new()
            .prefix(".try_update_")
            .tempdir_in(directory)
            .unwrap();
        let target = directory.join("try.exe");
        let data = directory.join("data");
        fs::create_dir(&data).unwrap();
        fs::write(&target, b"old program").unwrap();
        fs::write(root.path().join("payload.exe"), b"new program").unwrap();
        let job = Job {
            schema: 1,
            tag: "v1.2.3".into(),
            kind: PackageKind::Windows,
            target: fs::canonicalize(target).unwrap(),
            launch_target: directory.join("try.exe"),
            data: fs::canonicalize(data).unwrap(),
            parent_pid: 1,
            nonce: "a".repeat(64),
            payload_sha256: "b".repeat(64),
            helper_sha256: "c".repeat(64),
            old_sha256: "d".repeat(64),
            saved_files: vec![],
        };
        (job, root)
    }
    #[test]
    fn failed_replacement_restores_the_original_program() {
        let directory = tempfile::tempdir().unwrap();
        let (job, root) = fixture(directory.path());
        fs::remove_file(root.path().join("payload.exe")).unwrap();
        assert!(replace(&job, root.path()).is_err());
        assert_eq!(fs::read(&job.target).unwrap(), b"old program");
    }
    #[test]
    fn rollback_restores_profile_bytes_and_preserves_images() {
        let directory = tempfile::tempdir().unwrap();
        let (mut job, root) = fixture(directory.path());
        fs::write(job.data.join("state.json"), b"original settings and packs").unwrap();
        fs::write(job.data.join("learning_state.json"), b"original memory").unwrap();
        fs::create_dir(job.data.join("media")).unwrap();
        fs::write(job.data.join("media/fixture.png"), b"picture").unwrap();
        snapshot(&mut job, root.path()).unwrap();
        replace(&job, root.path()).unwrap();
        fs::write(job.data.join("state.json"), b"changed by failed startup").unwrap();
        fs::write(job.data.join("update_check_state.json"), b"new cache").unwrap();
        restore(&job, root.path()).unwrap();
        assert_eq!(fs::read(&job.target).unwrap(), b"old program");
        assert_eq!(
            fs::read(job.data.join("state.json")).unwrap(),
            b"original settings and packs"
        );
        assert_eq!(
            fs::read(job.data.join("learning_state.json")).unwrap(),
            b"original memory"
        );
        assert!(!job.data.join("update_check_state.json").exists());
        assert_eq!(
            fs::read(job.data.join("media/fixture.png")).unwrap(),
            b"picture"
        );
    }
    #[test]
    fn handoff_rejects_paths_outside_its_installation() {
        let directory = tempfile::tempdir().unwrap();
        let (mut job, root) = fixture(directory.path());
        save_job(&job, root.path()).unwrap();
        assert!(read_job(&root.path().join("job.json")).is_ok());
        job.target = directory.path().join("another/try.exe");
        save_job(&job, root.path()).unwrap();
        assert!(read_job(&root.path().join("job.json")).is_err());
    }
    #[test]
    fn rollback_does_not_overwrite_a_profile_open_in_another_process() {
        let directory = tempfile::tempdir().unwrap();
        let (mut job, root) = fixture(directory.path());
        fs::write(job.data.join("state.json"), b"saved settings").unwrap();
        snapshot(&mut job, root.path()).unwrap();
        replace(&job, root.path()).unwrap();
        let _other = Store::open(job.data.clone()).unwrap();
        assert!(restore(&job, root.path()).is_err());
        assert_eq!(fs::read(job.target).unwrap(), b"new program");
        assert!(root.path().join("previous").exists());
    }

    #[test]
    fn concurrent_or_stale_installations_never_replace_a_program() {
        let directory = tempfile::tempdir().unwrap();
        let (mut job, root) = fixture(directory.path());
        job.old_sha256 = digest_file(&job.target).unwrap();
        let other = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.path().join(".try_update.lock"))
            .unwrap();
        other.try_lock().unwrap();
        assert!(apply_and_start(&mut job, root.path()).is_err());
        assert_eq!(fs::read(&job.target).unwrap(), b"old program");
        assert!(!root.path().join("profile_backup").exists());
        drop(other);
        fs::write(&job.target, b"changed by another update").unwrap();
        assert!(apply_and_start(&mut job, root.path()).is_err());
        assert_eq!(fs::read(&job.target).unwrap(), b"changed by another update");
        assert!(!root.path().join("previous").exists());
    }

    #[test]
    fn failed_installation_logs_its_target_and_step_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let (mut job, root) = fixture(directory.path());
        job.old_sha256 = digest_file(&job.target).unwrap();
        fs::remove_file(root.path().join("payload.exe")).unwrap();
        assert!(apply_and_start(&mut job, root.path()).is_err());
        assert_eq!(fs::read(&job.target).unwrap(), b"old program");
        let text = fs::read_to_string(job.data.join("logs/try_rCURRENT.log")).unwrap();
        assert!(text.contains("event=install_started target=v1.2.3"));
        assert!(text.contains("event=install_failed target=v1.2.3 stage=replace"));
        assert!(!text.contains(&job.nonce));
        assert!(!text.contains(&job.helper_sha256));
    }

    #[test]
    fn rollback_logs_its_result_without_making_logging_required() {
        for mode in ["success", "restore_error", "unwritable_log"] {
            let directory = tempfile::tempdir().unwrap();
            let (mut job, root) = fixture(directory.path());
            fs::write(job.data.join("state.json"), b"saved settings").unwrap();
            snapshot(&mut job, root.path()).unwrap();
            replace(&job, root.path()).unwrap();
            if mode == "restore_error" {
                fs::remove_file(root.path().join("profile_backup/state.json")).unwrap();
            } else if mode == "unwritable_log" {
                fs::write(job.data.join("logs"), b"preserve this unrelated file").unwrap();
            }
            assert_eq!(restore(&job, root.path()).is_err(), mode == "restore_error");
            assert_eq!(fs::read(&job.target).unwrap(), b"old program");
            if mode == "unwritable_log" {
                assert_eq!(
                    fs::read(job.data.join("logs")).unwrap(),
                    b"preserve this unrelated file"
                );
                assert_eq!(
                    fs::read(job.data.join("state.json")).unwrap(),
                    b"saved settings"
                );
                continue;
            }
            let text = fs::read_to_string(job.data.join("logs/try_rCURRENT.log")).unwrap();
            assert!(text.contains("event=rollback_started target=v1.2.3"));
            assert_eq!(
                text.contains("event=rollback_failed"),
                mode == "restore_error"
            );
            assert_eq!(text.contains("event=rollback_succeeded"), mode == "success");
            assert!(!text.contains(&job.nonce));
        }
    }
}
