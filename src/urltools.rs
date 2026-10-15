//! Locating the gacha history URL (authkey) inside the game log and the
//! embedded browser cache, and parsing it into API request parameters.

use crate::model::Game;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CapturedUrl {
    pub url: String,
    pub cache_folder: Option<PathBuf>,
    /// i18n key describing where the URL came from (used in the status line).
    pub source: &'static str,
}

#[derive(Debug, Clone)]
pub struct GachaUrl {
    /// API host that answers the gacha requests.
    pub api_domain: String,
    /// Query string with `page`/`size`/`gacha_type`/`end_id` removed.
    pub query: String,
    pub lang: String,
    /// `true` when the captured URL belongs to the overseas client.
    #[allow(dead_code)]
    pub overseas: bool,
    pub original: String,
}

/// Characters CEF may leave around a URL inside the binary cache file.
fn is_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
}

/// Extracts every `authkey=...` gacha URL from a text blob.
///
/// The upstream tools anchor on `game_biz=` appearing after the authkey, which
/// is not always the order the game uses. This scanner accepts both orders: it
/// prefers the `game_biz` terminator, and otherwise stops at the first byte
/// that cannot be part of a URL.
pub fn extract_urls(text: &str, game: Game) -> Vec<String> {
    let bytes = text.as_bytes();
    let biz = format!("game_biz={}", game.biz_prefix());
    let mut found: Vec<String> = Vec::new();
    let mut search_from = 0usize;
    while let Some(found_at) = text[search_from..].find("authkey=") {
        let authkey_at = search_from + found_at;
        search_from = authkey_at + "authkey=".len();

        // Walk backwards to the scheme, stopping at any byte that cannot be
        // part of a URL (binary junk always contains one).
        let mut start = None;
        let mut i = authkey_at;
        while i > 0 {
            i -= 1;
            let byte = bytes[i];
            if !is_url_byte(byte) {
                break;
            }
            if byte == b'h' && text[i..].starts_with("http") {
                start = Some(i);
            }
        }
        let Some(start) = start else { continue };

        let end = match text[authkey_at..].find(&biz) {
            Some(offset) => {
                // `game_biz=hk4e_cn` closes the URL.
                let mut end = authkey_at + offset + biz.len();
                while end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
                    end += 1;
                }
                end
            }
            None => {
                let mut end = authkey_at;
                while end < bytes.len() && is_url_byte(bytes[end]) {
                    end += 1;
                }
                end
            }
        };
        let candidate = &text[start..end];
        if candidate.contains("auth_appid=webview_gacha")
            && !found.iter().any(|existing| existing == candidate)
        {
            found.push(candidate.to_string());
        }
    }
    found
}

fn read_blob(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Home directory of the Windows user, also handling Wine prefixes.
pub fn user_home() -> PathBuf {
    if let Ok(prefix) = std::env::var("WINEPREFIX") {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        return PathBuf::from(prefix).join("drive_c/users").join(user);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

struct Candidate {
    /// Path of the log file (or a fixed file used by the cloud client).
    log: PathBuf,
}

fn candidates(game: Game, log_type: i32) -> Vec<Candidate> {
    let home = user_home();
    let local_low = home.join("AppData/LocalLow");
    let mut list: Vec<Candidate> = Vec::new();
    match game {
        Game::Genshin => {
            let cn = local_low.join("miHoYo/原神/output_log.txt");
            let global = local_low.join("miHoYo/Genshin Impact/output_log.txt");
            let (first, second) = if log_type == 2 {
                (global, cn)
            } else {
                (cn, global)
            };
            if log_type != 3 {
                list.push(Candidate { log: first });
                list.push(Candidate { log: second });
            }
            list.push(Candidate {
                log: home.join("AppData/Local/miHoYo/GenshinImpactCloudGame/config/logs/MiHoYoSDK.log"),
            });
        }
        Game::StarRail => {
            let cn = local_low.join("miHoYo/崩坏：星穹铁道/Player.log");
            let global = local_low.join("Cognosphere/Star Rail/Player.log");
            let (first, second) = if log_type == 2 {
                (global, cn)
            } else {
                (cn, global)
            };
            list.push(Candidate { log: first });
            list.push(Candidate { log: second });
        }
    }
    if log_type == 0 {
        // Prefer the client that matches the system language, like upstream.
        let chinese = sys_locale_is_chinese();
        if !chinese {
            let mut sorted = Vec::new();
            sorted.push(list.pop().unwrap_or(Candidate {
                log: PathBuf::new(),
            }));
            sorted.extend(list);
            sorted.retain(|c| !c.log.as_os_str().is_empty());
            return sorted;
        }
    }
    list
}

fn sys_locale_is_chinese() -> bool {
    for key in ["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(value) = std::env::var(key) {
            let lower = value.to_ascii_lowercase();
            if lower.contains("zh") {
                return true;
            }
        }
    }
    // Windows: read the user default UI language.
    #[cfg(windows)]
    {
        if let Ok(value) = std::env::var("USERLANGUAGE") {
            if value.to_ascii_lowercase().contains("zh") {
                return true;
            }
        }
    }
    false
}

/// Finds the newest `webCaches/**/Cache_Data/data_2` below a game data folder.
fn newest_cache(game_dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut stack = vec![game_dir.join("webCaches")];
    let mut visited = 0;
    while let Some(dir) = stack.pop() {
        visited += 1;
        if visited > 5000 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
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
                .is_some_and(|n| n == "data_2")
                && path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == "Cache_Data")
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
    best.map(|(_, path)| path)
}

/// Reads the game log to locate the installation, then scrapes the web cache.
fn url_from_log(game: Game, log_path: &Path) -> Result<Option<CapturedUrl>, String> {
    let Some(text) = read_blob(log_path) else {
        return Err("log.file.readFailed".to_string());
    };
    let game_dir = game_data_dir(game, &text);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = game_dir {
        if let Some(cache) = newest_cache(&dir) {
            candidates.push(cache);
        }
    }
    // Old logs of the cloud client contain the URL directly.
    candidates.push(log_path.to_path_buf());
    for path in candidates {
        let Some(blob) = read_blob(&path) else { continue };
        let urls = extract_urls(&blob, game);
        if let Some(url) = urls.last() {
            let cache_folder = path
                .parent()
                .filter(|_| path.file_name().and_then(|n| n.to_str()) == Some("data_2"))
                .map(|p| p.to_path_buf());
            return Ok(Some(CapturedUrl {
                url: url.clone(),
                cache_folder,
                source: "log.cache.reading",
            }));
        }
    }
    Ok(None)
}

/// Pulls `X:/.../(GenshinImpact_Data|YuanShen_Data|StarRail_Data)/` out of a log.
fn game_data_dir(game: Game, text: &str) -> Option<PathBuf> {
    let markers: &[&str] = match game {
        Game::Genshin => &["GenshinImpact_Data", "YuanShen_Data"],
        Game::StarRail => &["StarRail_Data", "starrail_data"],
    };
    let upper = text.to_ascii_uppercase();
    let mut best: Option<usize> = None;
    for marker in markers {
        let marker_upper = marker.to_ascii_uppercase();
        if let Some(pos) = upper.find(&marker_upper) {
            best = Some(best.map_or(pos, |current: usize| current.min(pos)));
        }
    }
    let end_of_marker = best?;
    let bytes = text.as_bytes();
    // Walk back to the drive letter that starts the path (`D:/...`).
    let mut start = None;
    let mut i = end_of_marker;
    while i > 2 {
        i -= 1;
        if bytes[i] == b'/' && bytes[i - 1] == b':' && bytes[i - 2].is_ascii_alphabetic() {
            start = Some(i - 2);
            break;
        }
    }
    let start = start?;
    let mut end = end_of_marker;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let raw = &text[start..end];
    let normalized = raw.replace('/', std::path::MAIN_SEPARATOR_STR);
    Some(PathBuf::from(normalized))
}

/// Looks for the authkey URL in every known location of the given game.
pub fn find_url(game: Game, log_type: i32) -> Result<CapturedUrl, String> {
    let mut any_log = false;
    for candidate in candidates(game, log_type) {
        if !candidate.log.exists() {
            continue;
        }
        any_log = true;
        match url_from_log(game, &candidate.log) {
            Ok(Some(found)) => return Ok(found),
            Ok(None) => continue,
            Err(_) => continue,
        }
    }
    if !any_log {
        return Err("log.file.notFound".to_string());
    }
    Err("log.url.notFound".to_string())
}

/// Locates the `webCaches` directory of an installed game, if the log names it.
pub fn find_cache_dir(game: Game) -> Result<PathBuf, String> {
    for candidate in candidates(game, 0) {
        if !candidate.log.exists() {
            continue;
        }
        let Some(text) = read_blob(&candidate.log) else {
            continue;
        };
        if let Some(dir) = game_data_dir(game, &text) {
            let web_caches = dir.join("webCaches");
            if web_caches.exists() {
                return Ok(web_caches);
            }
        }
    }
    Err("log.file.notFound".to_string())
}

/// Replaces the authkey value with `***` so URLs can be logged safely.
pub fn mask_authkey(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find("authkey=") {
        let start = position + "authkey=".len();
        result.push_str(&rest[..start]);
        result.push_str("***");
        match rest[start..].find('&') {
            Some(end) => rest = &rest[start + end..],
            None => rest = "",
        }
    }
    result.push_str(rest);
    result
}

/// Upstream `fixAuthkey`: percent-encode a raw authkey that contains `=`.
pub fn fix_authkey(url: &str) -> String {
    let Some(pos) = url.find("authkey=") else {
        return url.to_string();
    };
    let start = pos + "authkey=".len();
    let end = url[start..]
        .find('&')
        .map(|offset| start + offset)
        .unwrap_or(url.len());
    let value = &url[start..end];
    if value.contains('=') && !value.contains('%') {
        let encoded = value.replace('=', "%3D");
        format!("{}{}{}", &url[..start], encoded, &url[end..])
    } else {
        url.to_string()
    }
}

/// Parses a captured URL into the API host plus the reusable query string.
pub fn parse_capture(url: &str, game: Game) -> Result<GachaUrl, String> {
    let fixed = fix_authkey(url.trim());
    let parsed = url::Url::parse(&fixed).map_err(|_| "log.url.incorrect".to_string())?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let overseas = host.contains("webstatic-sea")
        || host.contains("hk4e-api-os")
        || host.contains("hkrpg-api-os")
        || host.contains("api-os-takumi")
        || host.contains("hoyoverse.com");
    let api_domain = match (game, overseas) {
        (Game::Genshin, false) => "https://public-operation-hk4e.mihoyo.com",
        (Game::Genshin, true) => "https://public-operation-hk4e-sg.hoyoverse.com",
        (Game::StarRail, false) => "https://public-operation-hkrpg.mihoyo.com",
        (Game::StarRail, true) => "https://public-operation-hkrpg-sg.hoyoverse.com",
    }
    .to_string();

    let raw_query = parsed.query().unwrap_or_default();
    if !raw_query.contains("authkey=") {
        return Err("log.url.lackAuth".to_string());
    }
    let mut kept: Vec<String> = Vec::new();
    let mut lang = String::new();
    for pair in raw_query.split('&') {
        let key = pair.split('=').next().unwrap_or_default();
        match key {
            "page" | "size" | "gacha_type" | "end_id" => continue,
            "lang" => {
                lang = pair
                    .split_once('=')
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default();
                kept.push(pair.to_string());
            }
            _ => kept.push(pair.to_string()),
        }
    }
    Ok(GachaUrl {
        api_domain,
        query: kept.join("&"),
        lang,
        overseas,
        original: fixed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_genshin_url_from_noisy_blob() {
        let blob = "junk\u{0}\u{1}https://webstatic.mihoyo.com/hk4e/event/e20190909gacha-v2/index.html?authkey_ver=1&sign_type=2&auth_appid=webview_gacha&init_type=301&timestamp=1700000000&lang=zh-cn&authkey=AbCd%3D&game_uid=100000001&region=cn_gf01&game_biz=hk4e_cn\u{0}more";
        let urls = extract_urls(blob, Game::Genshin);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].ends_with("game_biz=hk4e_cn"), "{}", urls[0]);
        assert!(urls[0].starts_with("https://webstatic.mihoyo.com/"));
    }

    #[test]
    fn extracts_url_when_game_biz_comes_first() {
        // Some clients put `game_biz` before the authkey.
        let blob = "x\u{0}https://webstatic.mihoyo.com/hk4e/event/e20190909gacha-v2/index.html?auth_appid=webview_gacha&game_biz=hk4e_cn&authkey=AbCd&game_uid=1&region=cn_gf01\u{0}y";
        let urls = extract_urls(blob, Game::Genshin);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("https://webstatic.mihoyo.com/"));
        assert!(urls[0].contains("authkey=AbCd"));
        assert!(urls[0].ends_with("region=cn_gf01"), "{}", urls[0]);
    }

    #[test]
    fn extracts_the_last_starrail_url() {
        let blob = "https://webstatic.mihoyo.com/hkrpg/event/e20211215gacha-v2/index.html?authkey_ver=1&auth_appid=webview_gacha&authkey=OLD&game_biz=hkrpg_cn \
                    https://webstatic.mihoyo.com/hkrpg/event/e20211215gacha-v2/index.html?authkey_ver=1&auth_appid=webview_gacha&authkey=NEW&game_biz=hkrpg_cn";
        let urls = extract_urls(blob, Game::StarRail);
        assert_eq!(urls.len(), 2);
        assert!(urls.last().unwrap().contains("authkey=NEW"));
    }

    #[test]
    fn parses_query_and_drops_paging() {
        let url = "https://webstatic.mihoyo.com/hk4e/event/e20190909gacha-v2/index.html?authkey_ver=1&auth_appid=webview_gacha&authkey=abc%3D&lang=zh-cn&game_biz=hk4e_cn&page=9&size=20&gacha_type=301&end_id=5";
        let parsed = parse_capture(url, Game::Genshin).unwrap();
        assert_eq!(parsed.api_domain, "https://public-operation-hk4e.mihoyo.com");
        assert!(parsed.query.contains("authkey=abc%3D"));
        assert!(!parsed.query.contains("page="));
        assert!(!parsed.query.contains("gacha_type="));
        assert_eq!(parsed.lang, "zh-cn");
    }

    #[test]
    fn detects_overseas_host() {
        let url = "https://webstatic-sea.hoyoverse.com/hk4e/event/e20190909gacha-v2/index.html?auth_appid=webview_gacha&authkey=x&game_biz=hk4e_global";
        let parsed = parse_capture(url, Game::Genshin).unwrap();
        assert!(parsed.overseas);
        assert_eq!(parsed.api_domain, "https://public-operation-hk4e-sg.hoyoverse.com");
    }

    #[test]
    fn encodes_raw_authkey() {
        let url = "https://x/y?authkey=abc=def&game_biz=hk4e_cn";
        assert!(fix_authkey(url).contains("authkey=abc%3Ddef"));
    }
}
