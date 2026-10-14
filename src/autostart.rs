use crate::knowledge::AppResult;
use crate::tr;
use std::path::Path;
#[cfg(any(test, feature = "native_checks", not(target_os = "windows")))]
use std::path::PathBuf;

enum Registration {
    #[cfg(target_os = "windows")]
    Registry { key: String },
    #[cfg(any(test, feature = "native_checks", not(target_os = "windows")))]
    File { path: PathBuf },
}

/// The OS registration is the source of truth, rather than a second boolean
/// in the library which could report success after a failed system write.
pub struct Autostart {
    registration: Registration,
    content: String,
}

impl Autostart {
    pub fn for_current_user(executable: &Path, directory: &Path) -> AppResult<Self> {
        let executable = path_text(executable)?;
        let directory = std::fs::canonicalize(directory)
            .map_err(|e| tr!("无法定位自启的数据目录：{e}", e = e))?;
        let directory = path_text(&directory)?;
        #[cfg(target_os = "windows")]
        return Ok(Self {
            registration: Registration::Registry {
                key: "Software\\Microsoft\\Windows\\CurrentVersion\\Run".into(),
            },
            content: windows_command(executable, directory)?,
        });
        #[cfg(target_os = "macos")]
        return Ok(Self {
            registration: Registration::File {
                path: directories::BaseDirs::new()
                    .ok_or(tr!("无法找到用户目录。"))?
                    .home_dir()
                    .join("Library/LaunchAgents/local.try.practice.plist"),
            },
            content: launch_agent(executable, directory),
        });
        #[cfg(target_os = "linux")]
        return Ok(Self {
            registration: Registration::File {
                path: directories::BaseDirs::new()
                    .ok_or(tr!("无法找到用户配置目录。"))?
                    .config_dir()
                    .join("autostart/try_practice.desktop"),
            },
            content: linux_desktop_entry(
                executable,
                directory,
                std::env::var_os("APPIMAGE").as_deref(),
            )?,
        });
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            let _ = (executable, directory);
            Err(tr!("当前系统暂不支持开机自启。").into())
        }
    }

    pub fn is_enabled(&self) -> AppResult<bool> {
        let content = match &self.registration {
            #[cfg(target_os = "windows")]
            Registration::Registry { key } => windows::read(key)?,
            #[cfg(any(test, feature = "native_checks", not(target_os = "windows")))]
            Registration::File { path } => match std::fs::read_to_string(path) {
                Ok(content) => Some(content),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(tr!("无法读取自启设置：{e}", e = e)),
            },
        };
        Ok(content.as_deref() == Some(&self.content))
    }

    pub fn set_enabled(&self, enabled: bool) -> AppResult<()> {
        match &self.registration {
            #[cfg(target_os = "windows")]
            Registration::Registry { key } => {
                windows::write(key, enabled.then_some(self.content.as_str()))
            }
            #[cfg(any(test, feature = "native_checks", not(target_os = "windows")))]
            Registration::File { path } => {
                if enabled {
                    let parent = path.parent().ok_or(tr!("自启配置路径无效。"))?;
                    std::fs::create_dir_all(parent)
                        .map_err(|e| tr!("无法创建自启配置目录：{e}", e = e))?;
                    crate::storage::write_atomic(path, self.content.as_bytes())
                } else {
                    match std::fs::remove_file(path) {
                        Ok(()) => Ok(()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(e) => Err(tr!("无法关闭开机自启：{e}", e = e)),
                    }
                }
            }
        }
    }

    #[cfg(any(test, feature = "native_checks"))]
    pub fn isolated(path: PathBuf) -> Self {
        Self {
            registration: Registration::File { path },
            content: "Try native test registration\n--background\n".into(),
        }
    }
}

fn path_text(path: &Path) -> AppResult<&str> {
    let text = path
        .to_str()
        .ok_or(tr!("自启程序与数据目录需要有效的 Unicode 路径。"))?;
    if !path.is_absolute() || text.chars().any(char::is_control) {
        return Err(tr!("自启程序与数据目录须为不含控制字符的完整路径。").into());
    }
    Ok(text)
}

#[cfg(any(test, target_os = "windows"))]
fn windows_command(executable: &str, directory: &str) -> AppResult<String> {
    fn quote(text: &str) -> String {
        let mut result = String::from("\"");
        let mut slashes = 0;
        for c in text.chars() {
            if c == '\\' {
                slashes += 1;
                continue;
            }
            result.extend(std::iter::repeat_n(
                '\\',
                if c == '"' { slashes * 2 + 1 } else { slashes },
            ));
            result.push(c);
            slashes = 0;
        }
        result.extend(std::iter::repeat_n('\\', slashes * 2));
        result.push('"');
        result
    }
    let command = format!(
        "{} --background --data-dir {}",
        quote(executable),
        quote(directory)
    );
    if command.encode_utf16().count() > 260 {
        return Err(tr!("自启命令超过系统长度限制，请缩短程序或数据目录路径。").into());
    }
    Ok(command)
}

#[cfg(any(test, target_os = "linux"))]
fn linux_desktop_entry(
    executable: &str,
    directory: &str,
    appimage: Option<&std::ffi::OsStr>,
) -> AppResult<String> {
    // current_exe points inside an AppImage's temporary mount. Its original
    // launcher remains available after this process exits and at the next login.
    let executable = match appimage {
        Some(path) if !path.is_empty() => path_text(Path::new(path))?,
        _ => executable,
    };
    desktop_entry(executable, directory)
}

#[cfg(any(test, target_os = "linux"))]
fn desktop_entry(executable: &str, directory: &str) -> AppResult<String> {
    if executable.contains('=') {
        return Err(tr!("Linux 自启程序路径不能含等号。").into());
    }
    fn quote(text: &str) -> String {
        let mut result = String::from("\"");
        for c in text.chars() {
            match c {
                '\\' => result.push_str("\\\\\\\\"),
                '"' | '`' | '$' => {
                    result.push_str("\\\\");
                    result.push(c);
                }
                '%' => result.push_str("%%"),
                c => result.push(c),
            }
        }
        result.push('"');
        result
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=Try\nExec={} --background --data-dir {}\nTerminal=false\n",
        quote(executable),
        quote(directory)
    ))
}

#[cfg(any(test, target_os = "macos"))]
fn launch_agent(executable: &str, directory: &str) -> String {
    fn xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>local.try.practice</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--background</string><string>--data-dir</string><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><false/>\n</dict></plist>\n",
        xml(executable),
        xml(directory)
    )
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW,
    };

    const NAME: &str = "Try.Practice";
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    fn error(code: u32) -> String {
        tr!(
            "无法修改或读取开机自启：{p0}",
            p0 = std::io::Error::from_raw_os_error(code as i32)
        )
    }

    fn open(key: &str, write: bool, create: bool) -> AppResult<Option<Key>> {
        let name = wide(key);
        let mut handle = null_mut();
        let access = if write {
            KEY_SET_VALUE
        } else {
            KEY_QUERY_VALUE
        };
        // SAFETY: names are terminated UTF-16 and output storage is live;
        // accesses are limited to our current-user registration value.
        let result = unsafe {
            if create {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    name.as_ptr(),
                    0,
                    null(),
                    REG_OPTION_NON_VOLATILE,
                    access,
                    null(),
                    &mut handle,
                    null_mut(),
                )
            } else {
                RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, access, &mut handle)
            }
        };
        match result {
            ERROR_SUCCESS => Ok(Some(Key(handle))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            code => Err(error(code)),
        }
    }

    pub(super) fn read(key: &str) -> AppResult<Option<String>> {
        let Some(key) = open(key, false, false)? else {
            return Ok(None);
        };
        let name = wide(NAME);
        let mut size = 0;
        let mut kind = 0;
        // SAFETY: first query only obtains the value size and type.
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != ERROR_SUCCESS {
            return Err(error(result));
        }
        if kind != REG_SZ || size > 4096 || size % 2 != 0 {
            return Err(tr!("自启配置不是有效的程序命令。").into());
        }
        let mut data = vec![0u16; size as usize / 2];
        // SAFETY: the buffer is sized in bytes from the first query; a change
        // between queries is reported as an error, not an unchecked read.
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(error(result));
        }
        if kind != REG_SZ || size % 2 != 0 {
            return Err(tr!("自启配置不是有效的程序命令。").into());
        }
        data.truncate(size as usize / 2);
        if data.last() == Some(&0) {
            data.pop();
        }
        String::from_utf16(&data)
            .map(Some)
            .map_err(|_| tr!("自启命令包含无效字符。").into())
    }

    pub(super) fn write(key: &str, content: Option<&str>) -> AppResult<()> {
        let Some(key) = open(key, true, content.is_some())? else {
            return Ok(());
        };
        let name = wide(NAME);
        // SAFETY: only our named value is set/deleted; all buffers and the
        // owned key remain live throughout the call.
        let result = unsafe {
            if let Some(content) = content {
                let data = wide(content);
                RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            } else {
                RegDeleteValueW(key.0, name.as_ptr())
            }
        };
        match result {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            code => Err(error(code)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn registration_roundtrip_uses_an_isolated_registry_key() {
            struct Cleanup(String);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    // SAFETY: delete only the unique non-startup test key.
                    unsafe {
                        windows_sys::Win32::System::Registry::RegDeleteKeyW(
                            HKEY_CURRENT_USER,
                            wide(&self.0).as_ptr(),
                        );
                    }
                }
            }
            let cleanup = Cleanup(format!(
                "Software\\Try\\Tests\\autostart_{:032x}",
                rand::random::<u128>()
            ));
            let entry = Autostart {
                registration: Registration::Registry {
                    key: cleanup.0.clone(),
                },
                content: windows_command(r"D:\My Try\try.exe", r"D:\知识\data").unwrap(),
            };
            assert!(!entry.is_enabled().unwrap());
            entry.set_enabled(true).unwrap();
            assert!(entry.is_enabled().unwrap());
            assert_eq!(
                read(&cleanup.0).unwrap().as_deref(),
                Some(entry.content.as_str())
            );
            entry.set_enabled(false).unwrap();
            assert!(!entry.is_enabled().unwrap());
            entry.set_enabled(false).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_autostart_survives_removing_the_temporary_mount() {
        let root = tempfile::tempdir().unwrap();
        let appimage = root.path().join("Try app.AppImage");
        std::fs::write(&appimage, b"persistent launcher").unwrap();
        let mount = tempfile::tempdir_in(root.path()).unwrap();
        let executable = mount.path().join("try");
        std::fs::write(&executable, b"temporary executable").unwrap();
        let directory = root.path().join("profile");
        let entry = Autostart {
            registration: Registration::File {
                path: root.path().join("autostart/try_entry"),
            },
            content: linux_desktop_entry(
                path_text(&executable).unwrap(),
                path_text(&directory).unwrap(),
                Some(appimage.as_os_str()),
            )
            .unwrap(),
        };
        entry.set_enabled(true).unwrap();
        drop(mount);
        assert!(!executable.exists());
        assert!(appimage.is_file());
        assert!(entry.is_enabled().unwrap());
        assert_eq!(
            std::fs::read_to_string(root.path().join("autostart/try_entry")).unwrap(),
            desktop_entry(
                path_text(&appimage).unwrap(),
                path_text(&directory).unwrap(),
            )
            .unwrap()
        );
        entry.set_enabled(false).unwrap();
        assert!(!entry.is_enabled().unwrap());
    }

    #[test]
    fn linux_autostart_preserves_normal_launchers_and_rejects_invalid_appimage_paths() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("try");
        let directory = root.path().join("profile");
        let executable = path_text(&executable).unwrap();
        let directory = path_text(&directory).unwrap();
        let expected = desktop_entry(executable, directory).unwrap();
        for appimage in [None, Some(std::ffi::OsStr::new(""))] {
            assert_eq!(
                linux_desktop_entry(executable, directory, appimage).unwrap(),
                expected
            );
        }
        let invalid = root.path().join("invalid\nlauncher.AppImage");
        for appimage in [
            std::ffi::OsStr::new("relative.AppImage"),
            invalid.as_os_str(),
        ] {
            assert!(linux_desktop_entry(executable, directory, Some(appimage)).is_err());
        }
    }

    #[test]
    fn file_registration_is_persistent_and_can_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let entry = Autostart::isolated(dir.path().join("autostart/try_entry"));
        assert!(!entry.is_enabled().unwrap());
        entry.set_enabled(true).unwrap();
        assert!(entry.is_enabled().unwrap());
        let restored = Autostart::isolated(dir.path().join("autostart/try_entry"));
        assert!(restored.is_enabled().unwrap());
        entry.set_enabled(false).unwrap();
        assert!(!restored.is_enabled().unwrap());
    }

    #[test]
    fn failed_registration_does_not_claim_success() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not_a_directory");
        std::fs::write(&file, b"preserve").unwrap();
        let entry = Autostart::isolated(file.join("try_entry"));
        assert!(entry.set_enabled(true).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"preserve");
    }

    #[test]
    fn startup_arguments_keep_background_and_data_paths_quoted() {
        assert_eq!(
            windows_command(r"D:\My Try\try.exe", r"D:\知识\data").unwrap(),
            "\"D:\\My Try\\try.exe\" --background --data-dir \"D:\\知识\\data\""
        );
        assert!(windows_command(&"x".repeat(261), "data").is_err());
        let linux = desktop_entry("/tmp/Try app", "/tmp/My data").unwrap();
        assert!(linux.contains("Exec=\"/tmp/Try app\" --background --data-dir \"/tmp/My data\"\n"));
        assert!(desktop_entry("/tmp/try=app", "/tmp/data").is_err());
        assert!(
            desktop_entry("/tmp/try", "/tmp/50%$\\data")
                .unwrap()
                .contains("50%%\\\\$\\\\\\\\data")
        );
        let mac = launch_agent("/tmp/Try & app", "/tmp/<data>");
        assert!(mac.contains("<string>/tmp/Try &amp; app</string>"));
        assert!(mac.contains("<string>/tmp/&lt;data&gt;</string>"));
        assert!(mac.contains("<key>KeepAlive</key><false/>"));
    }
}
