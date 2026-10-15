//! Optional update check against a GitHub repository's latest release.
//!
//! The upstream tools shipped a self-updater pointing at the author's server.
//! A merged tool has no such server, so the update source is configurable: set
//! `update_repo` (for example `someone/aurora-export`) and the tool can
//! tell you when a newer release exists and open its download page.

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
}

/// Parses `v1.2.3` / `1.2` into comparable numbers.
fn parse_version(text: &str) -> Vec<u64> {
    text.trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let mut candidate = parse_version(candidate);
    let mut current = parse_version(current);
    let len = candidate.len().max(current.len());
    candidate.resize(len, 0);
    current.resize(len, 0);
    candidate > current
}

pub fn check(agent: &ureq::Agent, repo: &str) -> Result<Option<Release>, String> {
    let repo = repo.trim().trim_matches('/');
    if repo.is_empty() {
        return Err("log.update.noRepo".to_string());
    }
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let (status, value) = crate::net::get_json_with_status(agent, &url)?;
    if status == 404 {
        return Err("log.update.failed".to_string());
    }
    let tag = value
        .get("tag_name")
        .or_else(|| value.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if tag.is_empty() {
        return Err("log.update.failed".to_string());
    }
    let url = value
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com")
        .to_string();
    let notes = value
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if is_newer(&tag, env!("CARGO_PKG_VERSION")) {
        Ok(Some(Release {
            version: tag,
            url,
            notes,
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("v1.1", "1.0.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("v0.9.9", "1.0.0"));
    }
}
