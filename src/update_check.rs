//! A bounded release request shared by manual and scheduled checks. No downloads.

use semver::Version;
use serde::Deserialize;
use std::cmp::Ordering;

const RELEASES_API: &str = "https://api.github.com/repos/Ethan-decoy/try/releases?per_page=100";
const RELEASES_PAGE: &str = "https://github.com/Ethan-decoy/Try/releases/tag/";
const RELEASES_FEED: &str = "https://github.com/Ethan-decoy/Try/releases.atom";
const RELEASES_DOWNLOAD: &str = "https://github.com/Ethan-decoy/Try/releases/download/";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_ASSET_PROBES: usize = 3;

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
    check_with_fetch(
        env!("CARGO_PKG_VERSION"),
        ReleaseChannel::Preview,
        transport::get,
    )
}

fn check_with_fetch(
    current: &str,
    channel: ReleaseChannel,
    mut fetch: impl FnMut(&str) -> Result<(u16, Vec<u8>), CheckError>,
) -> Result<CheckOutcome, CheckError> {
    let (status, body) = fetch(RELEASES_API)?;
    log::debug!(target: "try::updates", "event=release_response source=api status={status}");
    match evaluate_response(current, channel, status, &body) {
        Err(CheckError::RateLimited) if channel == ReleaseChannel::Preview => {
            // A proxy's shared IP can exhaust GitHub's anonymous REST allowance.
            // Keep the configured network route and check public web resources.
            log::info!(target: "try::updates", "event=api_rate_limited fallback=public_releases");
            check_public_feed(current, &mut fetch).map_err(|error| {
                log::warn!(target: "try::updates", "event=public_release_fallback_failed reason={error}");
                CheckError::RateLimited
            })
        }
        result => result,
    }
}

fn check_public_feed(
    current: &str,
    fetch: &mut impl FnMut(&str) -> Result<(u16, Vec<u8>), CheckError>,
) -> Result<CheckOutcome, CheckError> {
    let current = Version::parse(current).map_err(|_| CheckError::InvalidResponse)?;
    let (status, body) = fetch(RELEASES_FEED)?;
    if status != 200 {
        return Err(CheckError::Network);
    }
    let mut releases = parse_public_feed(&body)?;
    releases.sort_by(|left, right| right.version.cmp_precedence(&left.version));
    for release in releases.iter().take(MAX_ASSET_PROBES) {
        // GitHub's feed also lists tags with no published release, including
        // draft candidates. A public distribution asset must exist too.
        let url = format!("{RELEASES_DOWNLOAD}{}/checksums.sha256", release.tag());
        let (status, _) = fetch(&url)?;
        log::debug!(target: "try::updates", "event=release_response source=public_asset status={status}");
        match status {
            // Do not follow the download redirect or fetch the installer.
            200 | 302 => {
                return Ok(
                    if release.version.cmp_precedence(&current) == Ordering::Greater {
                        CheckOutcome::Available(release.clone())
                    } else {
                        CheckOutcome::UpToDate
                    },
                );
            }
            404 => continue,
            _ => return Err(CheckError::Network),
        }
    }
    if releases.len() > MAX_ASSET_PROBES {
        // An older published version might still exist beyond our request budget.
        Err(CheckError::InvalidResponse)
    } else {
        Ok(CheckOutcome::NoPublicRelease)
    }
}

fn parse_public_feed(body: &[u8]) -> Result<Vec<Release>, CheckError> {
    use roxmltree::{Document, ParsingOptions};

    const ATOM: &str = "http://www.w3.org/2005/Atom";
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(CheckError::InvalidResponse);
    }
    let text = std::str::from_utf8(body).map_err(|_| CheckError::InvalidResponse)?;
    let document = Document::parse_with_options(
        text,
        ParsingOptions {
            nodes_limit: 10_000,
            ..ParsingOptions::default()
        },
    )
    .map_err(|_| CheckError::InvalidResponse)?;
    let root = document.root_element();
    let feed_id = root
        .children()
        .find(|node| node.has_tag_name((ATOM, "id")))
        .and_then(|node| node.text());
    if !root.has_tag_name((ATOM, "feed"))
        || feed_id != Some("tag:github.com,2008:https://github.com/Ethan-decoy/Try/releases")
    {
        return Err(CheckError::InvalidResponse);
    }
    let mut releases = Vec::new();
    let mut invalid_tag = false;
    for entry in root
        .children()
        .filter(|node| node.has_tag_name((ATOM, "entry")))
    {
        let tag = entry
            .children()
            .filter(|node| node.has_tag_name((ATOM, "link")))
            .filter(|node| node.attribute("rel") == Some("alternate"))
            .filter_map(|node| node.attribute("href"))
            .find_map(|href| href.strip_prefix(RELEASES_PAGE))
            .ok_or(CheckError::InvalidResponse)?;
        let Ok(release) = Release::from_tag(tag) else {
            invalid_tag = true;
            continue;
        };
        if !releases
            .iter()
            .any(|other: &Release| other.tag == release.tag)
        {
            releases.push(release);
        }
    }
    if releases.is_empty() && invalid_tag {
        Err(CheckError::InvalidResponse)
    } else {
        Ok(releases)
    }
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

mod transport {
    use super::CheckError;
    pub(super) fn get(url: &str) -> Result<(u16, Vec<u8>), CheckError> {
        crate::update_transport::get(url).map_err(|error| match error {
            crate::update_transport::Error::TooLarge => CheckError::InvalidResponse,
            _ => CheckError::Network,
        })
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

    fn feed(tags: &[&str]) -> Vec<u8> {
        let entries = tags
            .iter()
            .map(|tag| format!(r#"<entry><link rel="alternate" href="https://github.com/Ethan-decoy/Try/releases/tag/{tag}"/></entry>"#))
            .collect::<String>();
        format!(r#"<feed xmlns="http://www.w3.org/2005/Atom"><id>tag:github.com,2008:https://github.com/Ethan-decoy/Try/releases</id>{entries}</feed>"#).into_bytes()
    }

    #[test]
    fn rate_limited_api_checks_public_assets_and_excludes_draft_tags() {
        let mut requests = Vec::new();
        let outcome = check_with_fetch("0.1.0", ReleaseChannel::Preview, |url| {
            requests.push(url.to_owned());
            match url {
                RELEASES_API => Ok((403, Vec::new())),
                "https://github.com/Ethan-decoy/Try/releases.atom" => {
                    Ok((200, feed(&["v0.2.0", "v0.1.0"])))
                }
                "https://github.com/Ethan-decoy/Try/releases/download/v0.2.0/checksums.sha256" => {
                    Ok((404, Vec::new()))
                }
                "https://github.com/Ethan-decoy/Try/releases/download/v0.1.0/checksums.sha256" => {
                    Ok((302, Vec::new()))
                }
                _ => panic!("unexpected update endpoint: {url}"),
            }
        });
        assert_eq!(outcome, Ok(CheckOutcome::UpToDate));
        assert_eq!(requests.len(), 4);
    }

    #[test]
    fn rate_limited_api_can_find_a_newer_public_download() {
        let outcome = check_with_fetch("0.1.0", ReleaseChannel::Preview, |url| match url {
            RELEASES_API => Ok((429, Vec::new())),
            "https://github.com/Ethan-decoy/Try/releases.atom" => {
                Ok((200, feed(&["v0.2.0", "v0.1.0"])))
            }
            "https://github.com/Ethan-decoy/Try/releases/download/v0.2.0/checksums.sha256" => {
                Ok((302, Vec::new()))
            }
            _ => panic!("unexpected update endpoint: {url}"),
        });
        assert_eq!(
            outcome,
            Ok(CheckOutcome::Available(
                Release::from_tag("v0.2.0").unwrap()
            ))
        );
    }

    #[test]
    fn only_preview_rate_limits_use_the_public_fallback() {
        for (channel, status, body, expected) in [
            (
                ReleaseChannel::Stable,
                403,
                b"".as_slice(),
                Err(CheckError::RateLimited),
            ),
            (ReleaseChannel::Preview, 500, b"", Err(CheckError::Network)),
            (
                ReleaseChannel::Preview,
                404,
                b"",
                Ok(CheckOutcome::NoPublicRelease),
            ),
            (
                ReleaseChannel::Preview,
                200,
                b"not json",
                Err(CheckError::InvalidResponse),
            ),
        ] {
            let mut count = 0;
            let outcome = check_with_fetch("0.1.0", channel, |url| {
                count += 1;
                assert_eq!(url, RELEASES_API);
                Ok((status, body.to_vec()))
            });
            assert_eq!(outcome, expected);
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn fallback_failure_or_budget_exhaustion_preserves_the_original_error() {
        for (body, asset_status, expected_probes) in [
            (b"not xml".to_vec(), 404, 0),
            (feed(&["v0.2.0"]), 500, 1),
            (
                feed(&["v0.5.0", "v0.4.0", "v0.3.0", "v0.2.0"]),
                404,
                MAX_ASSET_PROBES,
            ),
        ] {
            let mut probes = 0;
            let outcome = check_with_fetch("0.1.0", ReleaseChannel::Preview, |url| match url {
                RELEASES_API => Ok((403, Vec::new())),
                RELEASES_FEED => Ok((200, body.clone())),
                _ => {
                    assert!(url.starts_with(RELEASES_DOWNLOAD));
                    probes += 1;
                    Ok((asset_status, Vec::new()))
                }
            });
            assert_eq!(outcome, Err(CheckError::RateLimited));
            assert_eq!(probes, expected_probes);
        }
    }

    #[test]
    fn public_feed_uses_version_order_and_deduplicates_tags() {
        let releases = parse_public_feed(&feed(&["v0.9.0", "v0.10.0", "v0.10.0"])).unwrap();
        assert_eq!(releases.len(), 2);
        let outcome = check_public_feed("0.9.0", &mut |url| match url {
            RELEASES_FEED => Ok((200, feed(&["v0.9.0", "v0.10.0"]))),
            "https://github.com/Ethan-decoy/Try/releases/download/v0.10.0/checksums.sha256" => {
                Ok((302, Vec::new()))
            }
            _ => panic!("expected newest public version first: {url}"),
        });
        assert_eq!(
            outcome,
            Ok(CheckOutcome::Available(
                Release::from_tag("v0.10.0").unwrap()
            ))
        );
    }

    #[test]
    fn public_feed_rejects_untrusted_links_entities_and_oversized_documents() {
        let valid = String::from_utf8(feed(&["v0.2.0"])).unwrap();
        let invalid = [
            valid.replace("/tag/v0.2.0", "/tag/v0.2.0?redirect=elsewhere"),
            valid.replace(
                "github.com/Ethan-decoy/Try/releases/tag/",
                "evil.example/releases/tag/",
            ),
            valid.replace(
                "tag:github.com,2008:https://github.com/Ethan-decoy/Try/releases",
                "another repository",
            ),
            format!(r#"<!DOCTYPE feed [<!ENTITY injected SYSTEM "file:///private">]>{valid}"#),
            "<html>not a release feed</html>".into(),
            " ".repeat(MAX_RESPONSE_BYTES + 1),
        ];
        for document in invalid {
            assert_eq!(
                parse_public_feed(document.as_bytes()),
                Err(CheckError::InvalidResponse)
            );
        }
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
            "https://github.com/Ethan-decoy/Try/releases/tag/v0.10.0"
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

    #[test]
    #[ignore = "calls GitHub's public feed and asset endpoints; run explicitly with network access"]
    fn live_rate_limited_api_fallback() {
        let result = check_with_fetch("0.1.0", ReleaseChannel::Preview, |url| {
            if url == RELEASES_API {
                // Reproduce the captured primary-limit response without making
                // another request against the exhausted API allowance.
                Ok((403, Vec::new()))
            } else {
                transport::get(url)
            }
        });
        let outcome = result.expect("Public feed fallback failed on the configured network route");
        assert_ne!(outcome, CheckOutcome::NoPublicRelease);
        println!("Public release fallback after API 403: {outcome:?}");
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
        let result =
            crate::update_transport::probe_native_tls_handshake(&format!("https://{address}/"))
                .map_err(|_| CheckError::Network);
        server.join().unwrap();
        assert_eq!(result, Err(CheckError::Network));
    }
}
