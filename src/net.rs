//! Small HTTP helpers built on `ureq` with rustls (no system TLS dependency).

use std::time::Duration;

/// Shared user agent string.
pub const USER_AGENT: &str = concat!("AuroraExport/", env!("CARGO_PKG_VERSION"));

pub fn agent(timeout_secs: u64) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build();
    config.into()
}

pub fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|err| format!("request failed: {err}"))?;
    response
        .body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|err| format!("invalid JSON response: {err}"))
}

pub fn get_json_with_status(
    agent: &ureq::Agent,
    url: &str,
) -> Result<(u16, serde_json::Value), String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|err| format!("request failed: {err}"))?;
    let status = response.status().as_u16();
    let value = response
        .body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|err| format!("invalid JSON response: {err}"))?;
    Ok((status, value))
}
