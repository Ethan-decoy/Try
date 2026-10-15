//! Stage a verified release beside its installation. User data is never staged here.
use crate::knowledge::AppResult;
use crate::tr;
use crate::update_check::Release;
use crate::update_transport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Windows,
    Macos,
    Linux,
}

impl PackageKind {
    pub fn asset(self, release: &Release) -> String {
        match self {
            Self::Windows => "try.exe".into(),
            Self::Macos => format!("try_macos_arm64_{}.zip", release.version),
            Self::Linux => format!("try_linux_x64_{}.AppImage", release.version),
        }
    }
    pub(crate) fn payload(self) -> &'static str {
        match self {
            Self::Windows => "payload.exe",
            Self::Macos => "Try.app",
            Self::Linux => "payload.AppImage",
        }
    }
    pub(crate) fn helper(self) -> &'static str {
        if self == Self::Windows {
            "helper.exe"
        } else {
            "helper"
        }
    }
}

#[derive(Clone, Debug)]
pub struct Installation {
    pub(crate) kind: PackageKind,
    pub(crate) target: PathBuf,
}

impl Installation {
    pub fn current() -> AppResult<Self> {
        if !cfg!(feature = "distribution") {
            return Err(tr!("开发构建请通过发布页面更新。").into());
        }
        if !(cfg!(all(target_os = "windows", target_arch = "x86_64"))
            || cfg!(all(target_os = "linux", target_arch = "x86_64"))
            || cfg!(all(target_os = "macos", target_arch = "aarch64")))
        {
            return Err(tr!("没有找到当前平台的更新文件。").into());
        }
        Self::platform()
    }

    #[cfg(target_os = "windows")]
    fn platform() -> AppResult<Self> {
        let target = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(Self {
            kind: PackageKind::Windows,
            target,
        })
    }

    #[cfg(target_os = "macos")]
    fn platform() -> AppResult<Self> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        if !executable.ends_with("Contents/MacOS/try") {
            return Err(tr!("请使用发布的应用包进行更新。").into());
        }
        let target = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or(tr!("无法定位当前程序。"))?
            .to_owned();
        Ok(Self {
            kind: PackageKind::Macos,
            target,
        })
    }

    #[cfg(target_os = "linux")]
    fn platform() -> AppResult<Self> {
        let target = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or(tr!("请使用发布的 AppImage 进行更新。"))?;
        let appdir = std::env::var_os("APPDIR")
            .map(PathBuf::from)
            .ok_or(tr!("无法定位当前程序。"))?;
        if !std::env::current_exe()
            .map_err(|e| e.to_string())?
            .starts_with(appdir)
        {
            return Err(tr!("无法定位当前程序。").into());
        }
        Ok(Self {
            kind: PackageKind::Linux,
            target,
        })
    }

    pub(crate) fn canonical(&self) -> AppResult<Self> {
        if fs::symlink_metadata(&self.target)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err(tr!("请从程序的原始位置更新。").into());
        }
        Ok(Self {
            kind: self.kind,
            target: fs::canonicalize(&self.target).map_err(|e| e.to_string())?,
        })
    }
}

pub struct PreparedUpdate {
    pub release: Release,
    pub(crate) installation: Installation,
    pub(crate) directory: tempfile::TempDir,
    pub(crate) payload_sha256: String,
}

pub fn prepare(
    release: Release,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> AppResult<PreparedUpdate> {
    let location = Installation::current()?.canonical()?;
    if release.version <= semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap() {
        return Err(tr!("这个版本无需更新。").into());
    }
    let base = format!(
        "https://github.com/Ethan-decoy/Try/releases/download/{}/",
        release.tag()
    );
    let asset = location.kind.asset(&release);
    let manifest = update_transport::metadata(&format!("{base}checksums.sha256"), cancel)
        .map_err(|e| transfer_error(&e))?;
    stage(release, location, cancel, &manifest, |archive| {
        update_transport::download(&format!("{base}{asset}"), archive, cancel, progress)
            .map_err(|e| transfer_error(&e))
    })
}

fn stage(
    release: Release,
    location: Installation,
    cancel: &AtomicBool,
    manifest: &[u8],
    download: impl FnOnce(&Path) -> AppResult<()>,
) -> AppResult<PreparedUpdate> {
    let expected = checksum(manifest, &location.kind.asset(&release))?;
    let parent = location.target.parent().ok_or(tr!("无法定位当前程序。"))?;
    let directory = tempfile::Builder::new()
        .prefix(".try_update_")
        .tempdir_in(parent)
        .map_err(|_| tr!("无法写入程序目录，请移到可写位置后重试。"))?;
    let archive = directory.path().join("download");
    download(&archive)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(tr!("已取消更新下载。").into());
    }
    if digest_file(&archive)? != expected {
        return Err(tr!("更新文件校验失败，请重新下载。").into());
    }
    let payload = directory.path().join(location.kind.payload());
    if location.kind == PackageKind::Macos {
        extract_bundle(&archive, directory.path())?;
        validate_binary(&payload.join("Contents/MacOS/try"), PackageKind::Macos)?;
        validate_macos_bundle(&payload, &release)?;
        fs::remove_file(&archive).map_err(|e| e.to_string())?;
    } else {
        validate_binary(&archive, location.kind)?;
        fs::rename(&archive, &payload).map_err(|e| e.to_string())?;
        executable_permissions(&payload)?;
    }
    let payload_sha256 = digest_file(&payload_executable(location.kind, &payload))?;
    Ok(PreparedUpdate {
        release,
        installation: location,
        directory,
        payload_sha256,
    })
}

#[cfg(feature = "native_checks")]
pub fn ui_fixture(parent: &Path) -> AppResult<PreparedUpdate> {
    let directory = tempfile::Builder::new()
        .prefix(".try_update_")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    Ok(PreparedUpdate {
        release: Release::from_tag("v99.0.0").unwrap(),
        installation: Installation {
            kind: PackageKind::Windows,
            target: directory.path().join("ui_fixture.exe"),
        },
        directory,
        payload_sha256: "0".repeat(64),
    })
}

#[cfg(all(feature = "native_checks", target_os = "windows"))]
pub fn prepare_fixture(source: &Path) -> AppResult<PreparedUpdate> {
    let location = Installation::current()?.canonical()?;
    let release = Release::from_tag(&format!("v{}", env!("CARGO_PKG_VERSION"))).unwrap();
    let manifest = format!("{}  try.exe\n", digest_file(source)?);
    stage(
        release,
        location,
        &AtomicBool::new(false),
        manifest.as_bytes(),
        |path| {
            fs::copy(source, path)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    )
}

fn transfer_error(error: &update_transport::Error) -> String {
    log::warn!(target: "try::updates", "event=download_failed reason={error}");
    match error {
        update_transport::Error::Cancelled => tr!("已取消更新下载。"),
        _ => tr!("更新下载失败，请重试。"),
    }
    .into()
}

fn checksum(bytes: &[u8], filename: &str) -> AppResult<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| tr!("更新校验文件无效。"))?;
    let mut found = None;
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2
            || parts[0].len() != 64
            || !parts[0].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(tr!("更新校验文件无效。").into());
        }
        if parts[1].trim_start_matches('*') == filename {
            if found.is_some() {
                return Err(tr!("更新校验文件无效。").into());
            }
            found = Some(parts[0].to_ascii_lowercase());
        }
    }
    found.ok_or_else(|| tr!("没有找到当前平台的更新文件。").into())
}

pub(crate) fn digest_file(path: &Path) -> AppResult<String> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn payload_executable(kind: PackageKind, path: &Path) -> PathBuf {
    if kind == PackageKind::Macos {
        path.join("Contents/MacOS/try")
    } else {
        path.to_owned()
    }
}

pub(crate) fn executable_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn validate_binary(path: &Path, kind: PackageKind) -> AppResult<()> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 4096];
    let length = input.read(&mut header).map_err(|e| e.to_string())?;
    let valid = match kind {
        PackageKind::Windows => {
            let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as usize;
            header[..2] == *b"MZ"
                && offset <= length.saturating_sub(6)
                && header[offset..offset + 4] == *b"PE\0\0"
                && header[offset + 4..offset + 6] == [0x64, 0x86]
        }
        PackageKind::Linux => {
            length >= 20
                && header[..4] == *b"\x7fELF"
                && header[4..6] == [2, 1]
                && header[8..11] == *b"AI\x02"
                && header[18..20] == [62, 0]
        }
        PackageKind::Macos => {
            length >= 8 && header[..4] == [0xcf, 0xfa, 0xed, 0xfe] && header[4..8] == [12, 0, 0, 1]
        }
    };
    if !valid {
        return Err(tr!("更新文件与当前平台不匹配。").into());
    }
    Ok(())
}

fn extract_bundle(archive: &Path, output: &Path) -> AppResult<()> {
    let input = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(input).map_err(|_| tr!("更新应用包无效。"))?;
    if zip.len() > 4096 {
        return Err(tr!("更新应用包无效。").into());
    }
    let mut total = 0u64;
    let mut names = std::collections::BTreeSet::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|_| tr!("更新应用包无效。"))?;
        let entry_name = entry
            .name()
            .map_err(|_| tr!("更新应用包无效。"))?
            .into_owned();
        let name = entry_name.trim_end_matches('/');
        let path = Path::new(name);
        if name.is_empty()
            || name.contains(['\\', ':'])
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !names.insert(name.to_lowercase())
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err(tr!("更新应用包包含不安全的路径。").into());
        }
        if path.starts_with("__MACOSX") {
            continue;
        }
        if !path.starts_with("Try.app") {
            return Err(tr!("更新应用包无效。").into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or(tr!("更新应用包过大。"))?;
        if entry.size() > 128 * 1024 * 1024 || total > 256 * 1024 * 1024 {
            return Err(tr!("更新应用包过大。").into());
        }
        let destination = output.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        } else {
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|e| e.to_string())?;
            let copied = std::io::copy(&mut entry.by_ref().take(128 * 1024 * 1024 + 1), &mut file)
                .map_err(|e| e.to_string())?;
            if copied != entry.size() {
                return Err(tr!("更新应用包无效。").into());
            }
            file.flush().map_err(|e| e.to_string())?;
        }
    }
    executable_permissions(&output.join("Try.app/Contents/MacOS/try"))
}

#[cfg(target_os = "macos")]
fn validate_macos_bundle(path: &Path, release: &Release) -> AppResult<()> {
    let status = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        return Err(tr!("更新应用包签名无效。").into());
    }
    for (key, expected) in [
        ("CFBundleIdentifier", "io.github.ethan-decoy.try".to_owned()),
        ("CFBundleShortVersionString", release.version.to_string()),
    ] {
        let output = std::process::Command::new("/usr/bin/plutil")
            .args(["-extract", key, "raw", "-o", "-"])
            .arg(path.join("Contents/Info.plist"))
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != expected {
            return Err(tr!("更新应用包版本无效。").into());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn validate_macos_bundle(_: &Path, _: &Release) -> AppResult<()> {
    Err(tr!("更新文件与当前平台不匹配。").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_requires_exact_platform_asset_and_rejects_ambiguity() {
        let hash = "a".repeat(64);
        assert_eq!(
            checksum(format!("{hash}  try.exe\n").as_bytes(), "try.exe").unwrap(),
            hash
        );
        for manifest in [
            format!("{hash}  other.exe"),
            format!("{hash}  try.exe\n{hash}  try.exe"),
            "bad try.exe".into(),
            format!("{hash}  ../try.exe"),
        ] {
            assert!(checksum(manifest.as_bytes(), "try.exe").is_err());
        }
    }
    #[test]
    fn package_names_follow_the_fixed_release_and_architecture() {
        let release = Release::from_tag("v1.2.3").unwrap();
        assert_eq!(PackageKind::Windows.asset(&release), "try.exe");
        assert_eq!(
            PackageKind::Macos.asset(&release),
            "try_macos_arm64_1.2.3.zip"
        );
        assert_eq!(
            PackageKind::Linux.asset(&release),
            "try_linux_x64_1.2.3.AppImage"
        );
    }
    #[test]
    fn failed_or_cancelled_download_never_changes_program_or_data() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("try.exe");
        let data = root.path().join("state.json");
        fs::write(&target, b"original program").unwrap();
        fs::write(&data, b"saved settings").unwrap();
        for mode in 0..3 {
            let cancel = AtomicBool::new(false);
            let location = Installation {
                kind: PackageKind::Windows,
                target: target.clone(),
            };
            let manifest = format!("{}  try.exe\n", "a".repeat(64));
            let result = stage(
                Release::from_tag("v99.0.0").unwrap(),
                location,
                &cancel,
                manifest.as_bytes(),
                |path| {
                    fs::write(path, b"partial download").unwrap();
                    if mode == 0 {
                        return Err("interrupted transfer".into());
                    }
                    if mode == 1 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(fs::read(&target).unwrap(), b"original program");
            assert_eq!(fs::read(&data).unwrap(), b"saved settings");
            assert_eq!(
                fs::read_dir(root.path()).unwrap().count(),
                2,
                "failed stage is removed"
            );
        }
    }
    #[test]
    fn verified_download_is_staged_until_restart() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("try.exe");
        fs::write(&target, b"original program").unwrap();
        let mut binary = vec![0u8; 4096];
        binary[..2].copy_from_slice(b"MZ");
        binary[60..64].copy_from_slice(&128u32.to_le_bytes());
        binary[128..134].copy_from_slice(b"PE\0\0\x64\x86");
        let expected = format!("{:x}", Sha256::digest(&binary));
        let manifest = format!("{expected}  try.exe\n");
        let prepared = stage(
            Release::from_tag("v99.0.0").unwrap(),
            Installation {
                kind: PackageKind::Windows,
                target: target.clone(),
            },
            &AtomicBool::new(false),
            manifest.as_bytes(),
            |path| fs::write(path, &binary).map_err(|e| e.to_string()),
        )
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original program");
        assert_eq!(prepared.payload_sha256, expected);
        let staged = prepared.directory.path().to_owned();
        drop(prepared);
        assert!(!staged.exists());
    }
    #[test]
    #[ignore = "downloads the published Windows executable over real HTTPS"]
    fn live_release_download() {
        let root = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let base = "https://github.com/Ethan-decoy/Try/releases/download/v0.2.1/";
        let manifest =
            update_transport::metadata(&format!("{base}checksums.sha256"), &cancel).unwrap();
        let expected = checksum(&manifest, "try.exe").unwrap();
        let path = root.path().join("try.exe");
        let mut received = 0;
        update_transport::download(&format!("{base}try.exe"), &path, &cancel, &mut |n, _| {
            received = n
        })
        .unwrap();
        assert_eq!(digest_file(&path).unwrap(), expected);
        assert_eq!(received, fs::metadata(path).unwrap().len());
        println!("verified streamed release download: {received} bytes");
    }
    #[test]
    fn mismatched_or_truncated_programs_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("payload");
        for bytes in [vec![], b"MZ".to_vec(), vec![0; 4096]] {
            fs::write(&path, bytes).unwrap();
            for kind in [PackageKind::Windows, PackageKind::Linux, PackageKind::Macos] {
                assert!(validate_binary(&path, kind).is_err());
            }
        }
    }
    #[test]
    fn bundle_extraction_rejects_paths_outside_the_stage() {
        use zip::write::SimpleFileOptions;
        let directory = tempfile::tempdir().unwrap();
        for name in [
            "../escape",
            "Try.app/../../escape",
            "/Try.app/file",
            "Try.app\\escape",
            "other.app/file",
        ] {
            let archive = directory.path().join("test.zip");
            let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"fixture").unwrap();
            zip.finish().unwrap();
            assert!(
                extract_bundle(&archive, directory.path()).is_err(),
                "{name}"
            );
        }
        assert!(!directory.path().parent().unwrap().join("escape").exists());
    }
}
