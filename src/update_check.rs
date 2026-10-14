//! A bounded release request shared by manual and scheduled checks. No downloads.

use semver::Version;
use serde::Deserialize;
use std::cmp::Ordering;

const RELEASES_API: &str = "https://api.github.com/repos/Ethan-decoy/try/releases?per_page=100";
const RELEASES_PAGE: &str = "https://github.com/Ethan-decoy/try/releases/tag/";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    tag: String,
}

impl Release {
    pub(crate) fn tag(&self) -> &str {
        &self.tag
    }

    pub fn from_tag(tag: &str) -> Result<Self, CheckError> {
        let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
            .map_err(|_| CheckError::InvalidResponse)?;
        Ok(Self {
            version,
            tag: tag.to_owned(),
        })
    }

    pub fn page_url(&self) -> String {
        // The tag has passed semver validation. Never open an API-supplied URL.
        format!("{RELEASES_PAGE}{}", self.tag)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    NoPublicRelease,
    UpToDate,
    Available(Release),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckError {
    Network,
    RateLimited,
    InvalidResponse,
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Network => "release request failed",
            Self::RateLimited => "release request rate limited",
            Self::InvalidResponse => "invalid release response",
        })
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

/// The first public version is a preview, so GitHub prereleases are eligible too.
/// Change this policy to Stable when shipping the first stable release.
pub fn check() -> Result<CheckOutcome, CheckError> {
    let (status, body) = transport::get(RELEASES_API)?;
    evaluate_response(
        env!("CARGO_PKG_VERSION"),
        ReleaseChannel::Preview,
        status,
        &body,
    )
}

fn evaluate_response(
    current: &str,
    channel: ReleaseChannel,
    status: u16,
    body: &[u8],
) -> Result<CheckOutcome, CheckError> {
    match status {
        404 => return Ok(CheckOutcome::NoPublicRelease),
        403 | 429 => return Err(CheckError::RateLimited),
        200 => {}
        _ => return Err(CheckError::Network),
    }
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(CheckError::InvalidResponse);
    }
    let current = Version::parse(current).map_err(|_| CheckError::InvalidResponse)?;
    let releases: Vec<ApiRelease> =
        serde_json::from_slice(body).map_err(|_| CheckError::InvalidResponse)?;
    let mut newest: Option<Release> = None;
    let mut invalid_tag = false;
    for release in releases {
        if release.draft || (channel == ReleaseChannel::Stable && release.prerelease) {
            continue;
        }
        let Ok(candidate) = Release::from_tag(&release.tag_name) else {
            invalid_tag = true;
            continue;
        };
        if channel == ReleaseChannel::Stable && !candidate.version.pre.is_empty() {
            continue;
        }
        if newest.as_ref().is_none_or(|other| {
            candidate.version.cmp_precedence(&other.version) == Ordering::Greater
        }) {
            newest = Some(candidate);
        }
    }
    let Some(newest) = newest else {
        return if invalid_tag {
            Err(CheckError::InvalidResponse)
        } else {
            Ok(CheckOutcome::NoPublicRelease)
        };
    };
    Ok(
        if newest.version.cmp_precedence(&current) == Ordering::Greater {
            CheckOutcome::Available(newest)
        } else {
            CheckOutcome::UpToDate
        },
    )
}

#[cfg(target_os = "windows")]
mod transport {
    use super::{CheckError, MAX_RESPONSE_BYTES};
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Networking::WinHttp::*;

    struct InternetHandle(*mut c_void);

    impl InternetHandle {
        fn new(handle: *mut c_void) -> Result<Self, CheckError> {
            if handle.is_null() {
                Err(CheckError::Network)
            } else {
                Ok(Self(handle))
            }
        }
    }

    impl Drop for InternetHandle {
        fn drop(&mut self) {
            // SAFETY: this wrapper owns a live WinHTTP handle and closes it once.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn succeeded(value: i32) -> Result<(), CheckError> {
        if value == 0 {
            Err(CheckError::Network)
        } else {
            Ok(())
        }
    }

    pub(super) fn get(url: &str) -> Result<(u16, Vec<u8>), CheckError> {
        let path = url
            .strip_prefix("https://api.github.com")
            .filter(|path| path.starts_with('/'))
            .ok_or(CheckError::Network)?;
        let agent = wide(concat!("Try/", env!("CARGO_PKG_VERSION")));
        let host = wide("api.github.com");
        let path = wide(path);
        let verb = wide("GET");
        let headers =
            wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        // SAFETY: strings are NUL-terminated and live through each synchronous call.
        // Handles own their resources; read/query pointers refer to initialized buffers.
        unsafe {
            let session = InternetHandle::new(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            ))?;
            succeeded(WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 5_000))?;
            let connection = InternetHandle::new(WinHttpConnect(session.0, host.as_ptr(), 443, 0))?;
            let request = InternetHandle::new(WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            ))?;
            // A fixed GitHub endpoint needs no redirects. Keep normal certificate checks.
            let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
            succeeded(WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                (&policy as *const u32).cast(),
                std::mem::size_of_val(&policy) as u32,
            ))?;
            succeeded(WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                null(),
                0,
                0,
                0,
            ))?;
            succeeded(WinHttpReceiveResponse(request.0, null_mut()))?;
            let mut status = 0u32;
            let mut size = std::mem::size_of_val(&status) as u32;
            succeeded(WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut size,
                null_mut(),
            ))?;
            if status != 200 {
                return Ok((status as u16, vec![]));
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut body = Vec::new();
            let mut buffer = [0u8; 8192];
            loop {
                if Instant::now() >= deadline {
                    return Err(CheckError::Network);
                }
                let mut read = 0;
                succeeded(WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                ))?;
                if read == 0 {
                    return Ok((status as u16, body));
                }
                if body.len() + read as usize > MAX_RESPONSE_BYTES {
                    return Err(CheckError::InvalidResponse);
                }
                body.extend_from_slice(&buffer[..read as usize]);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod transport {
    use super::{CheckError, MAX_RESPONSE_BYTES};
    use std::io::Read;
    use std::time::Duration;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    pub(super) fn get(url: &str) -> Result<(u16, Vec<u8>), CheckError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
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
        let mut response = agent
            .get(url)
            .header("User-Agent", concat!("Try/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|_| CheckError::Network)?;
        let status = response.status().as_u16();
        let mut body = vec![];
        if status == 200 {
            response
                .body_mut()
                .as_reader()
                .take(MAX_RESPONSE_BYTES as u64 + 1)
                .read_to_end(&mut body)
                .map_err(|_| CheckError::Network)?;
            if body.len() > MAX_RESPONSE_BYTES {
                return Err(CheckError::InvalidResponse);
            }
        }
        Ok((status, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str, prerelease: bool) -> serde_json::Value {
        json!({"tag_name": tag, "prerelease": prerelease, "draft": false})
    }

    fn evaluate(current: &str, channel: ReleaseChannel, values: serde_json::Value) -> CheckOutcome {
        evaluate_response(current, channel, 200, &serde_json::to_vec(&values).unwrap()).unwrap()
    }

    #[test]
    fn compares_semver_instead_of_release_order_or_text() {
        let result = evaluate(
            "0.9.0",
            ReleaseChannel::Preview,
            json!([release("v0.10.0", false), release("v0.9.1", false)]),
        );
        let CheckOutcome::Available(release) = result else {
            panic!("expected update")
        };
        assert_eq!(release.version.to_string(), "0.10.0");
        assert_eq!(
            release.page_url(),
            "https://github.com/Ethan-decoy/try/releases/tag/v0.10.0"
        );
    }

    #[test]
    fn preview_can_discover_github_preview_with_plain_version_tag() {
        let values = json!([release("v0.1.1", true), release("v0.1.0", false)]);
        assert!(matches!(
            evaluate("0.1.0", ReleaseChannel::Preview, values.clone()),
            CheckOutcome::Available(_)
        ));
        assert_eq!(
            evaluate("0.1.0", ReleaseChannel::Stable, values),
            CheckOutcome::UpToDate
        );
    }

    #[test]
    fn stable_excludes_semver_prereleases_even_if_api_flag_is_wrong() {
        let values = json!([release("v1.0.0-rc.2", false)]);
        assert_eq!(
            evaluate("0.1.0", ReleaseChannel::Stable, values),
            CheckOutcome::NoPublicRelease
        );
        assert!(matches!(
            evaluate(
                "1.0.0-rc.1",
                ReleaseChannel::Preview,
                json!([release("v1.0.0-rc.2", true)])
            ),
            CheckOutcome::Available(_)
        ));
    }

    #[test]
    fn ignores_drafts_and_non_version_tags() {
        assert_eq!(
            evaluate(
                "0.1.0",
                ReleaseChannel::Preview,
                json!([
                    {"tag_name": "v9.0.0", "draft": true, "prerelease": false},
                    release("latest", false), release("v0.1.0", false)
                ])
            ),
            CheckOutcome::UpToDate
        );
    }

    #[test]
    fn same_version_build_metadata_and_older_releases_do_not_request_downgrade() {
        for tag in ["v0.1.0+later", "0.0.9"] {
            assert_eq!(
                evaluate(
                    "0.1.0+local",
                    ReleaseChannel::Preview,
                    json!([release(tag, false)])
                ),
                CheckOutcome::UpToDate
            );
        }
    }

    #[test]
    fn missing_repository_and_empty_list_are_not_reported_as_up_to_date() {
        assert_eq!(
            evaluate_response("0.1.0", ReleaseChannel::Preview, 404, b"{}"),
            Ok(CheckOutcome::NoPublicRelease)
        );
        assert_eq!(
            evaluate("0.1.0", ReleaseChannel::Preview, json!([])),
            CheckOutcome::NoPublicRelease
        );
    }

    #[test]
    fn rejects_errors_malformed_missing_fields_and_large_responses() {
        for status in [403, 429] {
            assert_eq!(
                evaluate_response("0.1.0", ReleaseChannel::Preview, status, b""),
                Err(CheckError::RateLimited)
            );
        }
        assert_eq!(
            evaluate_response("0.1.0", ReleaseChannel::Preview, 500, b""),
            Err(CheckError::Network)
        );
        for body in [
            b"not json".as_slice(),
            b"{}",
            b"[{\"tag_name\":\"v0.1.1\"}]",
        ] {
            assert_eq!(
                evaluate_response("0.1.0", ReleaseChannel::Preview, 200, body),
                Err(CheckError::InvalidResponse)
            );
        }
        assert_eq!(
            evaluate_response(
                "0.1.0",
                ReleaseChannel::Preview,
                200,
                &vec![b' '; MAX_RESPONSE_BYTES + 1]
            ),
            Err(CheckError::InvalidResponse)
        );
    }

    #[test]
    fn rejects_invalid_tags_and_ignores_untrusted_browser_urls() {
        for tag in ["https://evil.example", "v1.0.0/../../", "1.0.0?redirect=x"] {
            assert_eq!(
                evaluate_response(
                    "0.1.0",
                    ReleaseChannel::Preview,
                    200,
                    &serde_json::to_vec(&json!([release(tag, false)])).unwrap()
                ),
                Err(CheckError::InvalidResponse)
            );
        }
        let result = evaluate(
            "0.1.0",
            ReleaseChannel::Preview,
            json!([{
                "tag_name": "v0.1.1", "prerelease": false, "draft": false,
                "html_url": "file:///private", "assets": [{"browser_download_url": "https://evil.example"}]
            }]),
        );
        let CheckOutcome::Available(release) = result else {
            panic!("expected update")
        };
        assert!(release.page_url().starts_with(RELEASES_PAGE));
    }

    #[test]
    #[ignore = "calls the public GitHub API; run explicitly with network access"]
    fn live_release_check() {
        println!("{:?}", check().expect("GitHub release request failed"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn native_https_transport_handles_failed_handshake_without_panicking() {
        use std::net::TcpListener;

        // A real TCP connection reaches the selected TLS connector. Closing it
        // during the handshake must report a network error, not a missing-provider panic.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..500 {
                match listener.accept() {
                    Ok((connection, _)) => {
                        drop(connection);
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => panic!("Cannot accept the test connection: {error}"),
                }
            }
            panic!("Native HTTPS transport did not connect to the local test server");
        });
        let result = super::transport::get(&format!("https://{address}/"));
        server.join().unwrap();
        assert_eq!(result, Err(CheckError::Network));
    }
}
