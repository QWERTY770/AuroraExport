//! Live URL capture ("proxy mode").
//!
//! The upstream tools install a MITM certificate and redirect the system proxy
//! so they can read the authkey out of the game's HTTPS traffic. This tool gets
//! the same result without touching TLS or the system proxy: the game's
//! embedded browser writes the authkey URL into its web cache, so we snapshot
//! the cache and wait for a fresh URL while the user opens the in game history
//! page. That works on Windows, macOS and Linux alike and needs no certificate.
//!
//! A helper to clear a proxy left behind by the older tools is kept as well.

use crate::fetch::Status;
use crate::model::Game;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn newest_cache_file(game: Game) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for dir in cache_candidates(game) {
        let mut stack = vec![dir];
        let mut visited = 0;
        while let Some(current) = stack.pop() {
            visited += 1;
            if visited > 8000 {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    stack.push(path);
                } else if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == "data_2" || n.starts_with("data_"))
                {
                    let modified = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    if best.as_ref().is_none_or(|(time, _)| modified > *time) {
                        best = Some((modified, path));
                    }
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

fn cache_candidates(game: Game) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // 1. The installation directory recorded in the game log.
    if let Ok(found) = crate::urltools::find_cache_dir(game) {
        dirs.push(found);
    }
    // 2. Well known install locations.
    let roots = match game {
        Game::Genshin => vec![
            "C:/Program Files/Genshin Impact/GenshinImpact_Data",
            "C:/Program Files/Genshin Impact/YuanShen_Data",
            "D:/Program Files/Genshin Impact/GenshinImpact_Data",
            "D:/Genshin Impact/GenshinImpact_Data",
            "D:/Genshin Impact/YuanShen_Data",
            "D:/Program Files/Genshin Impact/YuanShen_Data",
        ],
        Game::StarRail => vec![
            "C:/Program Files/Star Rail/Game/StarRail_Data",
            "D:/Program Files/Star Rail/Game/StarRail_Data",
            "D:/Star Rail/Game/StarRail_Data",
            "D:/Star Rail/StarRail_Data",
        ],
    };
    for root in roots {
        let path = PathBuf::from(root).join("webCaches");
        if path.exists() {
            dirs.push(path);
        }
    }
    dirs
}

/// Waits until a new authkey URL shows up in the game's web cache.
pub fn capture_url(
    game: Game,
    _port: u16,
    cancel: &AtomicBool,
    status: &mut dyn FnMut(Status),
) -> Result<String, String> {
    let baseline_file = newest_cache_file(game);
    let baseline_url = baseline_file
        .as_ref()
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map(|text| crate::urltools::extract_urls(&text, game))
        .and_then(|urls| urls.last().cloned());

    status(Status::with(
        "log.proxy.hint",
        vec![("ip", "127.0.0.1".to_string()), ("port", "-".to_string())],
    ));

    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("log.url.notFound".to_string());
        }
        if Instant::now() > deadline {
            return Err("log.url.notFound".to_string());
        }
        if let Some(path) = newest_cache_file(game) {
            if let Ok(bytes) = std::fs::read(&path) {
                let text = String::from_utf8_lossy(&bytes);
                let urls = crate::urltools::extract_urls(&text, game);
                if let Some(latest) = urls.last() {
                    let changed = match (&baseline_url, &baseline_file) {
                        (Some(before), Some(before_file)) => {
                            latest != before || path != *before_file
                        }
                        _ => true,
                    };
                    if changed {
                        return Ok(latest.clone());
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(800));
    }
}

// ---------------------------------------------------------------------------
// system proxy helpers
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn set_system_proxy(enable: bool, host: &str, port: u16) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            KEY_SET_VALUE,
        )
        .map_err(|err| err.to_string())?;
    key.set_value("ProxyEnable", &if enable { 1u32 } else { 0u32 })
        .map_err(|err| err.to_string())?;
    if enable {
        let ignore = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";
        key.set_value("ProxyServer", &format!("{host}:{port}"))
            .map_err(|err| err.to_string())?;
        key.set_value("ProxyOverride", &ignore)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_system_proxy(_enable: bool, _host: &str, _port: u16) -> Result<(), String> {
    Err("unsupported platform".to_string())
}

/// Clears a proxy left behind by a previous run of the upstream tools.
pub fn clear_system_proxy() -> Result<(), String> {
    set_system_proxy(false, "", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_candidates_do_not_panic() {
        let _ = cache_candidates(Game::Genshin);
        let _ = cache_candidates(Game::StarRail);
    }
}
