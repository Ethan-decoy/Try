//! Bounded HTTPS transfers for update metadata and release assets. No credentials.
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub(crate) enum Error {
    Network,
    TooLarge,
    Cancelled,
    Status(u16),
    File,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network => f.write_str("network"),
            Self::TooLarge => f.write_str("response_too_large"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Status(status) => write!(f, "http_{status}"),
            Self::File => f.write_str("file_write"),
        }
    }
}

struct Response {
    status: u16,
    location: Option<String>,
}

type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

fn endpoint(url: &str) -> Result<(&str, &str), Error> {
    if url.bytes().any(|c| c <= 32 || c == 127) || url.contains('#') {
        return Err(Error::Network);
    }
    let rest = url.strip_prefix("https://").ok_or(Error::Network)?;
    let (host, _) = rest.split_once('/').ok_or(Error::Network)?;
    if !matches!(
        host,
        "api.github.com"
            | "github.com"
            | "release-assets.githubusercontent.com"
            | "objects.githubusercontent.com"
            | "github-releases.githubusercontent.com"
    ) {
        return Err(Error::Network);
    }
    Ok((host, &rest[host.len()..]))
}

pub(crate) fn get(url: &str) -> Result<(u16, Vec<u8>), Error> {
    let mut body = Vec::new();
    let response = platform::request(
        url,
        &mut body,
        1024 * 1024,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )?;
    Ok((response.status, body))
}

#[cfg(all(test, not(target_os = "windows")))]
pub(crate) fn probe_native_tls_handshake(url: &str) -> Result<(u16, Vec<u8>), Error> {
    // This fixture reaches the real connector without widening the production
    // endpoint allowlist to include a local test server.
    let mut body = Vec::new();
    let response = platform::request_to_host(
        url,
        "localhost",
        &mut body,
        1024 * 1024,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )?;
    Ok((response.status, body))
}

fn followed(
    url: &str,
    output: &mut dyn Write,
    limit: u64,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<(), Error> {
    let mut current = url.to_owned();
    for _ in 0..=5 {
        endpoint(&current)?;
        let response = platform::request(&current, output, limit, cancel, progress)?;
        match response.status {
            200 => return Ok(()),
            301 | 302 | 303 | 307 | 308 => {
                current = response.location.ok_or(Error::Network)?;
            }
            status => return Err(Error::Status(status)),
        }
    }
    Err(Error::Network)
}

pub(crate) fn metadata(url: &str, cancel: &AtomicBool) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    followed(url, &mut body, 16 * 1024, cancel, &mut |_, _| {})?;
    Ok(body)
}

pub(crate) fn download(
    url: &str,
    path: &Path,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<(), Error> {
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| Error::File)?;
    followed(url, &mut output, 128 * 1024 * 1024, cancel, progress)?;
    output.sync_all().map_err(|_| Error::File)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Networking::WinHttp::*;

    struct Handle(*mut c_void);
    impl Handle {
        fn new(raw: *mut c_void) -> Result<Self, Error> {
            if raw.is_null() {
                Err(Error::Network)
            } else {
                Ok(Self(raw))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: this guard exclusively owns a live WinHTTP handle.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn ok(result: i32) -> Result<(), Error> {
        if result == 0 {
            Err(Error::Network)
        } else {
            Ok(())
        }
    }

    unsafe fn header(request: &Handle, query: u32) -> Option<String> {
        let mut buffer = [0u16; 4096];
        let mut size = std::mem::size_of_val(&buffer) as u32;
        // SAFETY: the initialized buffer and byte length agree; no pointer escapes.
        if unsafe {
            WinHttpQueryHeaders(
                request.0,
                query,
                null(),
                buffer.as_mut_ptr().cast(),
                &mut size,
                null_mut(),
            )
        } == 0
        {
            return None;
        }
        String::from_utf16(&buffer[..(size as usize / 2).min(buffer.len())])
            .ok()
            .map(|s| s.trim_end_matches('\0').to_owned())
    }

    pub(super) fn request(
        url: &str,
        output: &mut dyn Write,
        limit: u64,
        cancel: &AtomicBool,
        progress: Progress<'_>,
    ) -> Result<Response, Error> {
        let (host_name, path) = endpoint(url)?;
        let agent = wide(concat!("Try/", env!("CARGO_PKG_VERSION")));
        let host = wide(host_name);
        let path = wide(path);
        let verb = wide("GET");
        let headers = wide(if host_name == "api.github.com" {
            "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
        } else {
            "Accept: */*\r\n"
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        // SAFETY: all strings are NUL terminated and live through these synchronous
        // calls. Certificate checks stay enabled and automatic redirects stay off.
        unsafe {
            let session = Handle::new(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            ))?;
            ok(WinHttpSetTimeouts(session.0, 5_000, 10_000, 15_000, 15_000))?;
            let connection = Handle::new(WinHttpConnect(session.0, host.as_ptr(), 443, 0))?;
            let request = Handle::new(WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            ))?;
            let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
            ok(WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                (&policy as *const u32).cast(),
                std::mem::size_of_val(&policy) as u32,
            ))?;
            ok(WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                null(),
                0,
                0,
                0,
            ))?;
            ok(WinHttpReceiveResponse(request.0, null_mut()))?;
            let mut status = 0u32;
            let mut size = std::mem::size_of_val(&status) as u32;
            ok(WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut size,
                null_mut(),
            ))?;
            let response = Response {
                status: status as u16,
                location: header(&request, WINHTTP_QUERY_LOCATION),
            };
            if status != 200 {
                return Ok(response);
            }
            let length =
                header(&request, WINHTTP_QUERY_CONTENT_LENGTH).and_then(|s| s.parse::<u64>().ok());
            if length.is_some_and(|n| n > limit) {
                return Err(Error::TooLarge);
            }
            let deadline =
                Instant::now() + Duration::from_secs(if limit <= 1024 * 1024 { 15 } else { 300 });
            let mut total = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Err(Error::Network);
                }
                let mut read = 0;
                ok(WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                ))?;
                if read == 0 {
                    if length.is_some_and(|n| n != total) {
                        return Err(Error::Network);
                    }
                    return Ok(response);
                }
                total += u64::from(read);
                if total > limit {
                    return Err(Error::TooLarge);
                }
                output
                    .write_all(&buffer[..read as usize])
                    .map_err(|_| Error::File)?;
                progress(total, length);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;
    use std::io::Read;
    use std::time::Duration;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    pub(super) fn request(
        url: &str,
        output: &mut dyn Write,
        limit: u64,
        cancel: &AtomicBool,
        progress: Progress<'_>,
    ) -> Result<Response, Error> {
        let (host, _) = endpoint(url)?;
        request_to_host(url, host, output, limit, cancel, progress)
    }

    pub(super) fn request_to_host(
        url: &str,
        host: &str,
        output: &mut dyn Write,
        limit: u64,
        cancel: &AtomicBool,
        progress: Progress<'_>,
    ) -> Result<Response, Error> {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(if limit <= 1024 * 1024 {
                15
            } else {
                300
            })))
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        let mut request = agent
            .get(url)
            .header("User-Agent", concat!("Try/", env!("CARGO_PKG_VERSION")))
            .header(
                "Accept",
                if host == "api.github.com" {
                    "application/vnd.github+json"
                } else {
                    "*/*"
                },
            );
        if host == "api.github.com" {
            request = request.header("X-GitHub-Api-Version", "2022-11-28");
        }
        let mut response = request.call().map_err(|_| Error::Network)?;
        let result = Response {
            status: response.status().as_u16(),
            location: response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
        };
        if result.status != 200 {
            return Ok(result);
        }
        let length = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        if length.is_some_and(|n| n > limit) {
            return Err(Error::TooLarge);
        }
        let mut reader = response.body_mut().as_reader();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let read = reader.read(&mut buffer).map_err(|_| Error::Network)?;
            if read == 0 {
                if length.is_some_and(|n| n != total) {
                    return Err(Error::Network);
                }
                return Ok(result);
            }
            total += read as u64;
            if total > limit {
                return Err(Error::TooLarge);
            }
            output.write_all(&buffer[..read]).map_err(|_| Error::File)?;
            progress(total, length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirects_cannot_leave_trusted_https_hosts() {
        for url in [
            "http://github.com/a",
            "https://github.com.evil/a",
            "https://github.com@evil/a",
            "https://user@github.com/a",
            "https://github.com:444/a",
            "https://127.0.0.1:443/a",
            "file:///tmp/a",
            "https://github.com/a\r\nHeader: bad",
        ] {
            assert!(endpoint(url).is_err(), "{url}");
        }
        assert!(endpoint("https://release-assets.githubusercontent.com/github-production-release-asset/a?signature=opaque").is_ok());
    }
}
