//! Manual "check for updates": one GET of the latest GitHub release, only when
//! the user presses the button. Nothing is sent but the request itself.
use std::io::Read;

pub const LATEST_API_URL: &str = "https://api.github.com/repos/radzap/Aloud-Type/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Failed,
}

/// `tag_name` from the GitHub release JSON (the one field we need).
pub fn parse_tag(json: &str) -> Option<String> {
    let rest = &json[json.find("\"tag_name\"")? + "\"tag_name\"".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn parts(v: &str) -> Option<Vec<u64>> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?
        .split('.')
        .map(|p| p.parse().ok())
        .collect()
}

/// True when `latest` is a higher version than `current` (both like `v1.2.3`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parts(latest), parts(current)) {
        (Some(mut a), Some(mut b)) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            a > b
        }
        _ => false,
    }
}

/// Asks GitHub for the latest release. `Available(tag)` when it is newer than `current`.
pub fn check(current: &str) -> UpdateStatus {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into();
    let body = agent
        .get(LATEST_API_URL)
        .header("User-Agent", "aloud-type")
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()
        .and_then(|r| {
            let mut s = String::new();
            r.into_body()
                .into_reader()
                .take(1 << 20)
                .read_to_string(&mut s)
                .ok()
                .map(|_| s)
        });
    match body.as_deref().and_then(parse_tag) {
        Some(tag) if is_newer(&tag, current) => UpdateStatus::Available(tag),
        Some(_) => UpdateStatus::UpToDate,
        None => UpdateStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tag() {
        assert_eq!(
            parse_tag(r#"{"url":"x","tag_name": "v0.2.0","name":"y"}"#).as_deref(),
            Some("v0.2.0")
        );
        assert_eq!(parse_tag("{}"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("v1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
