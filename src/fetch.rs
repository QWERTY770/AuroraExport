//! Gacha history fetching: validates the captured URL, discovers the UID and
//! pages through every banner, merging incrementally like the upstream tools.

use crate::gamedata::GameData;
use crate::model::{Game, Record};
use crate::urltools::GachaUrl;
use std::collections::HashMap;
use std::time::Duration;

/// A progress message. `key` is an i18n key, or literal text when it comes from
/// the server.
#[derive(Clone, Debug)]
pub struct Status {
    pub key: String,
    pub args: Vec<(String, String)>,
}

impl Status {
    pub fn key(key: &str) -> Status {
        Status {
            key: key.to_string(),
            args: Vec::new(),
        }
    }

    pub fn with(key: &str, args: Vec<(&str, String)>) -> Status {
        Status {
            key: key.to_string(),
            args: args
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// Fetch the whole six month window instead of stopping at the newest local record.
    pub full_history: bool,
    /// Local newest record id per banner key.
    pub local_latest: HashMap<String, String>,
    /// UID of the currently selected account, used when the API returns none.
    pub uid_hint: Option<String>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        FetchOptions {
            full_history: false,
            local_latest: HashMap::new(),
            uid_hint: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fetched {
    pub uid: String,
    pub lang: String,
    pub region: String,
    pub region_time_zone: i32,
    /// `(banner key, records)` in fetch order, records oldest first.
    pub buckets: Vec<(String, Vec<Record>)>,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct FetchFailure {
    pub text: String,
    pub auth_timeout: bool,
}

impl FetchFailure {
    fn key(key: &str) -> FetchFailure {
        FetchFailure {
            text: key.to_string(),
            auth_timeout: false,
        }
    }

    fn message(text: impl Into<String>) -> FetchFailure {
        FetchFailure {
            text: text.into(),
            auth_timeout: false,
        }
    }

    fn auth_timeout(text: impl Into<String>) -> FetchFailure {
        FetchFailure {
            text: text.into(),
            auth_timeout: true,
        }
    }
}

const RETRY_LIMIT: u32 = 5;
const PAGE_SIZE: usize = 20;

struct Api {
    agent: ureq::Agent,
    domain: String,
}

impl Api {
    fn get(&self, path: &str, query: &str, extra: &str) -> Result<serde_json::Value, FetchFailure> {
        let url = if extra.is_empty() {
            format!("{}{path}?{query}", self.domain)
        } else {
            format!("{}{path}?{query}&{extra}", self.domain)
        };
        let mut response = self
            .agent
            .get(&url)
            .call()
            .map_err(|err| FetchFailure::message(format!("{err}")))?;
        let value = response
            .body_mut()
            .read_json::<serde_json::Value>()
            .map_err(|err| FetchFailure::message(format!("{err}")))?;
        check_status(&value)?;
        Ok(value)
    }
}

fn check_status(value: &serde_json::Value) -> Result<(), FetchFailure> {
    let retcode = value.get("retcode").and_then(|v| v.as_i64()).unwrap_or(0);
    if retcode == 0 {
        return Ok(());
    }
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("request failed")
        .to_string();
    if message == "authkey timeout" || message == "-101" {
        Err(FetchFailure::auth_timeout("log.fetch.authTimeout"))
    } else {
        Err(FetchFailure::message(message))
    }
}

fn record_from_item(item: &serde_json::Value) -> Option<Record> {
    let text = |key: &str| -> String {
        match item.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        }
    };
    let id = text("id");
    if id.is_empty() && item.get("name").is_none() {
        return None;
    }
    let rank = match item.get("rank_type") {
        Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    };
    Some(Record {
        time: text("time"),
        name: text("name"),
        item_type: text("item_type"),
        rank,
        gacha_type: text("gacha_type"),
        id,
        item_id: text("item_id"),
        gacha_id: text("gacha_id"),
        count: text("count"),
    })
}

fn list_of(value: &serde_json::Value) -> Vec<Record> {
    value
        .get("list")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(record_from_item).collect())
        .unwrap_or_default()
}

fn log_path(game: Game, banner: &str) -> &'static str {
    match game {
        Game::Genshin => "/gacha_info/api/getGachaLog",
        Game::StarRail => {
            if banner == "21" || banner == "22" {
                "/common/hkrpg_gacha_record/api/getLdGachaLog"
            } else {
                "/common/hkrpg_gacha_record/api/getGachaLog"
            }
        }
    }
}

/// Validates the URL by issuing a cheap request, mirroring upstream `tryRequest`.
fn validate(api: &Api, game: Game, query: &str) -> Result<(), FetchFailure> {
    match game {
        Game::Genshin => {
            api.get("/gacha_info/api/getConfigList", query, "")?;
        }
        Game::StarRail => {
            api.get(
                "/common/hkrpg_gacha_record/api/getGachaLog",
                query,
                "gacha_type=1&page=1&size=5&end_id=0",
            )?;
        }
    }
    Ok(())
}

/// Finds the UID by asking the first banner that has records.
fn discover_uid(api: &Api, game: Game, query: &str, data: &GameData, lang: &str) -> String {
    for (key, _) in data.banners(game, lang) {
        let path = match game {
            Game::Genshin => "/gacha_info/api/getGachaLog",
            Game::StarRail => "/common/hkrpg_gacha_record/api/getGachaLog",
        };
        let extra = format!("gacha_type={key}&page=1&size=6");
        if let Ok(value) = api.get(path, query, &extra) {
            if let Some(uid) = value
                .get("data")
                .and_then(|d| d.get("list"))
                .and_then(|l| l.as_array())
                .and_then(|l| l.first())
                .and_then(|item| item.get("uid"))
                .and_then(|uid| uid.as_str())
            {
                if !uid.is_empty() {
                    return uid.to_string();
                }
            }
        }
    }
    String::new()
}

/// Pages through one banner until the API stops returning records.
#[allow(clippy::too_many_arguments)]
fn fetch_banner(
    api: &Api,
    game: Game,
    banner_key: &str,
    banner_name: &str,
    query: &str,
    opts: &FetchOptions,
    cancel: &std::sync::atomic::AtomicBool,
    status: &mut dyn FnMut(Status),
) -> Result<(Vec<Record>, String, String, i32), FetchFailure> {
    let path = log_path(game, banner_key);
    let mut records: Vec<Record> = Vec::new();
    let mut page: u32 = 1;
    let mut end_id = String::new();
    let mut uid = String::new();
    let mut region = String::new();
    let mut region_time_zone = 0i32;
    let local_latest = opts.local_latest.get(banner_key).cloned().unwrap_or_default();

    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(FetchFailure::message("log.fetch.cancelled"));
        }
        if game == Game::Genshin && page % 10 == 0 {
            status(Status::with(
                "log.fetch.interval",
                vec![
                    ("name", banner_name.to_string()),
                    ("page", page.to_string()),
                ],
            ));
            std::thread::sleep(Duration::from_millis(1000));
        }
        status(Status::with(
            "log.fetch.current",
            vec![
                ("name", banner_name.to_string()),
                ("page", page.to_string()),
            ],
        ));
        let extra = if end_id.is_empty() {
            format!("gacha_type={banner_key}&page={page}&size={PAGE_SIZE}")
        } else {
            format!("gacha_type={banner_key}&page={page}&size={PAGE_SIZE}&end_id={end_id}")
        };

        let mut attempt = 0u32;
        let value = loop {
            match api.get(path, query, &extra) {
                Ok(value) => break value,
                Err(err) => {
                    if err.auth_timeout {
                        return Err(err);
                    }
                    if attempt < RETRY_LIMIT {
                        attempt += 1;
                        status(Status::with(
                            "log.fetch.retry",
                            vec![
                                ("name", banner_name.to_string()),
                                ("page", page.to_string()),
                                ("count", attempt.to_string()),
                            ],
                        ));
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                    status(Status::with(
                        "log.fetch.retryFailed",
                        vec![
                            ("name", banner_name.to_string()),
                            ("page", page.to_string()),
                        ],
                    ));
                    return Err(err);
                }
            }
        };

        std::thread::sleep(Duration::from_millis(300));

        let data = value.get("data").cloned().unwrap_or(serde_json::Value::Null);
        let page_records = list_of(&data);
        if page == 1 {
            if uid.is_empty() {
                uid = data
                    .get("list")
                    .and_then(|l| l.as_array())
                    .and_then(|l| l.first())
                    .and_then(|item| item.get("uid"))
                    .and_then(|uid| uid.as_str())
                    .unwrap_or_default()
                    .to_string();
            }
            region = data
                .get("region")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            region_time_zone = data
                .get("region_time_zone")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
        }
        let empty = page_records.is_empty();
        if let Some(last) = page_records.last() {
            end_id = last.id.clone();
        }
        records.extend(page_records);

        if empty {
            break;
        }
        if !opts.full_history && !local_latest.is_empty() && records.iter().any(|r| r.id == local_latest)
        {
            // Everything newer than the newest local record is already in hand.
            break;
        }
        page += 1;
    }

    // The API returns newest first; the tools store oldest first.
    records.reverse();
    Ok((records, uid, region, region_time_zone))
}

/// Full pipeline for one captured URL.
pub fn fetch_account(
    game: Game,
    captured: &str,
    data: &GameData,
    opts: &FetchOptions,
    cancel: &std::sync::atomic::AtomicBool,
    status: &mut dyn FnMut(Status),
) -> Result<Fetched, FetchFailure> {
    let parsed: GachaUrl = crate::urltools::parse_capture(captured, game)
        .map_err(|key| FetchFailure::key(&key))?;
    let api = Api {
        agent: crate::net::agent(15),
        domain: parsed.api_domain.clone(),
    };

    validate(&api, game, &parsed.query)?;

    let lang = if parsed.lang.is_empty() {
        "zh-cn".to_string()
    } else {
        parsed.lang.clone()
    };

    let mut uid = discover_uid(&api, game, &parsed.query, data, &lang);
    if uid.is_empty() {
        if let Some(hint) = &opts.uid_hint {
            uid = hint.clone();
        }
    }

    let banners = data.banners(game, &lang);
    if banners.is_empty() {
        return Err(FetchFailure::message("no banner configuration"));
    }

    let mut buckets: Vec<(String, Vec<Record>)> = Vec::new();
    let mut region = String::new();
    let mut region_time_zone = 0i32;
    for (key, name) in &banners {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(FetchFailure::message("log.fetch.cancelled"));
        }
        let (records, banner_uid, banner_region, timezone) =
            fetch_banner(&api, game, key, name, &parsed.query, opts, cancel, status)?;
        if uid.is_empty() && !banner_uid.is_empty() {
            uid = banner_uid;
        }
        if region.is_empty() && !banner_region.is_empty() {
            region = banner_region;
        }
        if region_time_zone == 0 && timezone != 0 {
            region_time_zone = timezone;
        }
        buckets.push((key.clone(), records));
    }

    if region_time_zone == 0 {
        region_time_zone = crate::model::timezone_for_uid(&uid);
    }

    Ok(Fetched {
        uid,
        lang,
        region,
        region_time_zone,
        buckets,
        url: parsed.original,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_records() {
        let item = serde_json::json!({
            "uid": "800000000",
            "gacha_id": "1001",
            "gacha_type": "11",
            "item_id": "1001",
            "count": "1",
            "time": "2024-05-01 12:00:00",
            "name": "希儿",
            "item_type": "角色",
            "rank_type": "5",
            "id": "1682521800010412950"
        });
        let record = record_from_item(&item).unwrap();
        assert_eq!(record.rank, 5);
        assert_eq!(record.gacha_type, "11");
        assert_eq!(record.count, "1");
        assert_eq!(record.name, "希儿");
    }

    #[test]
    fn surfaces_auth_timeout() {
        let value = serde_json::json!({"retcode": -101, "message": "authkey timeout"});
        let err = check_status(&value).unwrap_err();
        assert!(err.auth_timeout);
        assert_eq!(err.text, "log.fetch.authTimeout");
    }
}
