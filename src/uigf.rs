//! UIGF / SRGF / local JSON import and export.
//!
//! * UIGF v4.1 is used for both games (`hk4e` for Genshin, `hkrpg` for Star Rail).
//! * UIGF v3.0 export is Genshin only, exactly like `genshin-wish-export`.
//! * Star Rail item names are resolved from the embedded item dictionary,
//!   Genshin item ids from `api.uigf.org` (with a local cache), like upstream.

use crate::gamedata::GameData;
use crate::model::{Account, Game, Record};
use crate::net;
use crate::store::{timestamp_minute, write_atomic};
use std::collections::HashMap;
use std::path::Path;

pub const EXPORT_APP: &str = "AuroraExport";
pub const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Genshin in-game language codes used by the UIGF item dictionary.
pub fn uigf_lang(lang: &str) -> String {
    let normalized = GameData::normalize_lang(lang);
    match normalized.as_str() {
        "zh-cn" => "chs",
        "zh-tw" => "cht",
        "de-de" => "de",
        "en-us" => "en",
        "es-es" => "es",
        "fr-fr" => "fr",
        "id-id" => "id",
        "ja-jp" => "jp",
        "ko-kr" => "kr",
        "pt-pt" => "pt",
        "ru-ru" => "ru",
        "th-th" => "th",
        "vi-vn" => "vi",
        other => other,
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// item id dictionary
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ItemDict {
    /// language -> item name -> item id
    by_lang: HashMap<String, HashMap<String, String>>,
    /// language -> md5 of the remote dictionary
    md5: HashMap<String, String>,
    dirty: bool,
}

impl ItemDict {
    pub fn load(dir: &Path) -> ItemDict {
        let mut dict = ItemDict::default();
        let path = dir.join("item-id-dict.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return dict;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return dict;
        };
        if let Some(langs) = value.get("lang").and_then(|v| v.as_object()) {
            for (lang, entries) in langs {
                let mut table = HashMap::new();
                if let Some(entries) = entries.as_object() {
                    for (name, id) in entries {
                        let id = match id {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        table.insert(name.clone(), id);
                    }
                }
                dict.by_lang.insert(lang.clone(), table);
            }
        }
        if let Some(md5s) = value.get("md5").and_then(|v| v.as_object()) {
            for (lang, hash) in md5s {
                if let Some(hash) = hash.as_str() {
                    dict.md5.insert(lang.clone(), hash.to_string());
                }
            }
        }
        dict
    }

    pub fn save(&self, dir: &Path) {
        if !self.dirty {
            return;
        }
        let value = serde_json::json!({ "lang": self.by_lang, "md5": self.md5 });
        if let Ok(text) = serde_json::to_string(&value) {
            let _ = write_atomic(&dir.join("item-id-dict.json"), text.as_bytes());
        }
    }

    pub fn get(&self, lang: &str, name: &str) -> Option<String> {
        let lang = uigf_lang(lang);
        self.by_lang
            .get(&lang)
            .and_then(|table| table.get(name))
            .cloned()
            .filter(|id| !id.is_empty())
    }

    /// Downloads the dictionary (or reuses the cache when the md5 matches).
    pub fn refresh(&mut self, agent: &ureq::Agent) {
        let remote_md5 = net::get_json(agent, "https://api.uigf.org/md5/genshin")
            .ok()
            .and_then(|value| value.as_object().cloned())
            .map(|map| {
                map.into_iter()
                    .filter_map(|(lang, hash)| hash.as_str().map(|h| (lang, h.to_string())))
                    .collect::<HashMap<_, _>>()
            });
        let Some(remote_md5) = remote_md5 else { return };
        let up_to_date = !self.by_lang.is_empty()
            && remote_md5
                .iter()
                .all(|(lang, hash)| self.md5.get(lang) == Some(hash));
        if up_to_date {
            return;
        }
        let Ok(value) = net::get_json(agent, "https://api.uigf.org/dict/genshin/all.json") else {
            return;
        };
        let Some(entries) = value.as_object() else {
            return;
        };
        let mut fetched = HashMap::new();
        for (lang, dict) in entries {
            let mut table = HashMap::new();
            if let Some(dict) = dict.as_object() {
                for (name, id) in dict {
                    let id = match id {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    table.insert(name.clone(), id);
                }
            }
            fetched.insert(lang.clone(), table);
        }
        if fetched.is_empty() {
            return;
        }
        self.by_lang = fetched;
        self.md5 = remote_md5;
        self.dirty = true;
    }

    /// Looks up an item id, asking the UIGF service as a last resort.
    pub fn resolve(&mut self, agent: &ureq::Agent, lang: &str, name: &str) -> String {
        if let Some(id) = self.get(lang, name) {
            return id;
        }
        let code = uigf_lang(lang);
        let url = format!(
            "https://api.uigf.org/identify/genshin/{}",
            urlencoding(name)
        );
        if let Ok(value) = net::get_json(agent, &url) {
            if let Some(id) = value
                .get("matched")
                .and_then(|v| v.as_array())
                .and_then(|list| list.first())
                .and_then(|item| item.get("item_id"))
            {
                let id = match id {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                self.by_lang
                    .entry(code)
                    .or_default()
                    .insert(name.to_string(), id.clone());
                self.dirty = true;
                return id;
            }
        }
        String::new()
    }
}

fn urlencoding(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UigfVersion {
    V30,
    V41,
}

impl UigfVersion {
    pub fn parse(text: &str) -> UigfVersion {
        if text.starts_with('3') {
            UigfVersion::V30
        } else {
            UigfVersion::V41
        }
    }
}

fn fake_id_generator() -> impl FnMut() -> String {
    let mut next: u128 = 1_000_000_000_000_000_000;
    move || {
        next += 1;
        next.to_string()
    }
}

fn record_sort_key(record: &Record) -> (u128, String) {
    (
        record.sort_key().unwrap_or(0),
        record.time.clone(),
    )
}

fn sorted_records(account: &Account) -> Vec<(&Record, &str)> {
    let mut all: Vec<(&Record, &str)> = Vec::new();
    for bucket in &account.buckets {
        for record in &bucket.records {
            all.push((record, bucket.key.as_str()));
        }
    }
    all.sort_by(|a, b| record_sort_key(a.0).cmp(&record_sort_key(b.0)));
    all
}

/// Builds the UIGF document for the given accounts.
pub fn build(
    version: UigfVersion,
    accounts: &[&Account],
    dict: &mut ItemDict,
    agent: &ureq::Agent,
) -> Result<serde_json::Value, String> {
    if accounts.is_empty() || accounts.iter().all(|account| account.is_empty()) {
        return Err("log.export.empty".to_string());
    }
    let mut records_needing_ids = false;
    for account in accounts {
        if account.game == Game::Genshin && account.total_records() > 0 {
            records_needing_ids = true;
        }
    }
    if records_needing_ids {
        dict.refresh(agent);
    }

    match version {
        UigfVersion::V30 => build_v30(accounts, dict, agent),
        UigfVersion::V41 => build_v41(accounts, dict, agent),
    }
}

fn build_v41(
    accounts: &[&Account],
    dict: &mut ItemDict,
    agent: &ureq::Agent,
) -> Result<serde_json::Value, String> {
    let mut genshin = Vec::new();
    let mut starrail = Vec::new();
    let mut fake_id = fake_id_generator();

    for account in accounts {
        if account.is_empty() {
            continue;
        }
        let mut list = Vec::new();
        for (record, bucket_key) in sorted_records(account) {
            let mut item = serde_json::Map::new();
            match account.game {
                Game::Genshin => {
                    item.insert("uigf_gacha_type".into(), bucket_key.into());
                    item.insert("gacha_type".into(), bucket_key.into());
                    let item_id = if record.item_id.is_empty() {
                        dict.resolve(agent, &account.lang, &record.name)
                    } else {
                        record.item_id.clone()
                    };
                    item.insert("item_id".into(), item_id.into());
                }
                Game::StarRail => {
                    item.insert(
                        "gacha_id".into(),
                        if record.gacha_id.is_empty() {
                            record.gacha_type.clone().into()
                        } else {
                            record.gacha_id.clone().into()
                        },
                    );
                    item.insert("gacha_type".into(), record.gacha_type.clone().into());
                    item.insert("item_id".into(), record.item_id.clone().into());
                    item.insert(
                        "count".into(),
                        if record.count.is_empty() {
                            "1".into()
                        } else {
                            record.count.clone().into()
                        },
                    );
                }
            }
            item.insert("time".into(), record.time.clone().into());
            item.insert("name".into(), record.name.clone().into());
            item.insert("item_type".into(), record.item_type.clone().into());
            item.insert("rank_type".into(), record.rank.to_string().into());
            item.insert(
                "id".into(),
                if record.id.is_empty() {
                    fake_id().into()
                } else {
                    record.id.clone().into()
                },
            );
            list.push(serde_json::Value::Object(item));
        }
        if list.is_empty() {
            continue;
        }
        let timezone = if account.region_time_zone != 0 {
            account.region_time_zone
        } else {
            crate::model::timezone_for_uid(&account.uid)
        };
        let entry = serde_json::json!({
            "uid": account.uid,
            "timezone": timezone,
            "lang": GameData::normalize_lang(&account.lang),
            "list": list,
        });
        match account.game {
            Game::Genshin => genshin.push(entry),
            Game::StarRail => starrail.push(entry),
        }
    }

    let mut info = serde_json::Map::new();
    info.insert(
        "export_timestamp".into(),
        (chrono::Utc::now().timestamp()).into(),
    );
    info.insert("export_app".into(), EXPORT_APP.into());
    info.insert("export_app_version".into(), APP_VERSION.into());
    info.insert("version".into(), "v4.1".into());
    let mut root = serde_json::Map::new();
    root.insert("info".into(), serde_json::Value::Object(info));
    if !genshin.is_empty() {
        root.insert("hk4e".into(), serde_json::Value::Array(genshin));
    }
    if !starrail.is_empty() {
        root.insert("hkrpg".into(), serde_json::Value::Array(starrail));
    }
    Ok(serde_json::Value::Object(root))
}

fn build_v30(
    accounts: &[&Account],
    dict: &mut ItemDict,
    agent: &ureq::Agent,
) -> Result<serde_json::Value, String> {
    let account = accounts
        .iter()
        .find(|account| account.game == Game::Genshin && !account.is_empty())
        .ok_or_else(|| "log.export.empty".to_string())?;
    let mut fake_id = fake_id_generator();
    let mut list = Vec::new();
    for (record, bucket_key) in sorted_records(account) {
        let item_id = if record.item_id.is_empty() {
            dict.resolve(agent, &account.lang, &record.name)
        } else {
            record.item_id.clone()
        };
        list.push(serde_json::json!({
            "gacha_type": if record.gacha_type.is_empty() { bucket_key } else { record.gacha_type.as_str() },
            "time": record.time,
            "name": record.name,
            "item_type": record.item_type,
            "item_id": item_id,
            "rank_type": record.rank.to_string(),
            "id": if record.id.is_empty() { fake_id() } else { record.id.clone() },
            "uigf_gacha_type": bucket_key,
        }));
    }
    let timestamp = chrono::Utc::now().timestamp();
    Ok(serde_json::json!({
        "info": {
            "uid": account.uid,
            "lang": GameData::normalize_lang(&account.lang),
            "export_time": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "export_timestamp": timestamp,
            "export_app": EXPORT_APP,
            "export_app_version": APP_VERSION,
            "uigf_version": "v3.0",
            "region_time_zone": if account.region_time_zone != 0 { account.region_time_zone } else { crate::model::timezone_for_uid(&account.uid) },
        },
        "list": list,
    }))
}

pub fn default_file_name(version: UigfVersion, accounts: &[&Account]) -> String {
    let uids: Vec<&str> = accounts
        .iter()
        .filter(|account| !account.is_empty())
        .map(|account| account.uid.as_str())
        .collect();
    match (version, uids.len()) {
        (UigfVersion::V30, _) => format!(
            "UIGF_v3.0_{}_{}.json",
            uids.first().copied().unwrap_or("0"),
            timestamp_minute()
        ),
        (UigfVersion::V41, 1) => format!("UIGF_v4.1_{}_{}.json", uids[0], timestamp_minute()),
        (UigfVersion::V41, _) => format!("UIGF_v4.1_{}.json", timestamp_minute()),
    }
}

// ---------------------------------------------------------------------------
// import
// ---------------------------------------------------------------------------

fn text_of(value: &serde_json::Value, key: &str) -> String {
    match value.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn record_from_uigf(item: &serde_json::Value, bucket_key: &str) -> Record {
    let rank = text_of(item, "rank_type").trim().parse::<i64>().unwrap_or(0);
    Record {
        time: text_of(item, "time"),
        name: text_of(item, "name"),
        item_type: text_of(item, "item_type"),
        rank,
        gacha_type: {
            let value = text_of(item, "gacha_type");
            if value.is_empty() {
                bucket_key.to_string()
            } else {
                value
            }
        },
        id: text_of(item, "id"),
        item_id: text_of(item, "item_id"),
        gacha_id: text_of(item, "gacha_id"),
        count: text_of(item, "count"),
    }
}

/// Parses an imported file: either a local `gacha-list-*.json` or UIGF/SRGF.
pub fn parse_import_text(
    text: &str,
    data: &GameData,
    ui_lang: &str,
) -> Result<Vec<Account>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("JSON error: {err}"))?;
    if value.get("result").is_some() && value.get("typeMap").is_some() {
        let hint = value
            .get("game")
            .and_then(|v| v.as_str())
            .and_then(Game::from_id)
            .or_else(|| crate::store::detect_game(&value));
        let account =
            Account::from_json(&value, hint).ok_or_else(|| "invalid local data file".to_string())?;
        return Ok(vec![account]);
    }
    parse_import(&value, data, ui_lang)
}

/// Parses an imported UIGF / SRGF document.
pub fn parse_import(
    value: &serde_json::Value,
    data: &GameData,
    ui_lang: &str,
) -> Result<Vec<Account>, String> {
    let info = value.get("info");
    let version = info
        .and_then(|info| info.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let srgf = info
        .and_then(|info| info.get("srgf_version"))
        .and_then(|v| v.as_str())
        .is_some();
    let uigf3 = info
        .and_then(|info| info.get("uigf_version"))
        .and_then(|v| v.as_str())
        .map(|v| v.starts_with("v3"))
        .unwrap_or(false);

    if !version.is_empty() {
        if version != "v4.0" && version != "v4.1" {
            return Err(format!("unsupported UIGF version: {version}"));
        }
        let mut accounts = Vec::new();
        for (key, game) in [("hk4e", Game::Genshin), ("hkrpg", Game::StarRail)] {
            let Some(list) = value.get(key).and_then(|v| v.as_array()) else {
                continue;
            };
            for entry in list {
                accounts.push(account_from_group(entry, game, data, ui_lang)?);
            }
        }
        if accounts.is_empty() {
            return Err("UIGF file contains no account data".to_string());
        }
        return Ok(accounts);
    }

    if uigf3 || srgf {
        let game = if srgf { Game::StarRail } else { Game::Genshin };
        let info = info.ok_or_else(|| "missing info section".to_string())?;
        let uid = text_of(info, "uid");
        if uid.is_empty() {
            return Err("missing uid".to_string());
        }
        let lang = {
            let lang = text_of(info, "lang");
            if lang.is_empty() {
                ui_lang.to_string()
            } else {
                lang
            }
        };
        let list = value
            .get("list")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "missing list".to_string())?;
        let mut account = Account::new(uid, game);
        account.lang = GameData::normalize_lang(&lang);
        account.region_time_zone = info
            .get("region_time_zone")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(|| crate::model::timezone_for_uid(&account.uid));
        account.time = chrono::Utc::now().timestamp_millis();
        let banners = data.banners(game, &account.lang);
        for key in banners.iter().map(|(key, _)| key.clone()) {
            account.ensure_bucket(&key, "");
        }
        for item in list {
            let bucket_key = if game == Game::Genshin {
                let value = text_of(item, "uigf_gacha_type");
                if value.is_empty() {
                    text_of(item, "gacha_type")
                } else {
                    value
                }
            } else {
                text_of(item, "gacha_type")
            };
            let record = record_from_uigf(item, &bucket_key);
            ensure_bucket(&mut account, &bucket_key, &banners);
            if let Some(bucket) = account.bucket_mut(&bucket_key) {
                bucket.records.push(record);
            }
        }
        account.sort_all();
        return Ok(vec![account]);
    }

    Err("UIGF format error".to_string())
}

/// Makes sure a bucket exists *and* carries its localized name.
///
/// Importers create the buckets up front (without a name) so a banner with no
/// records still shows up, so the name has to be filled in even when the bucket
/// already exists.
fn ensure_bucket(account: &mut Account, key: &str, banners: &[(String, String)]) {
    let name = banners
        .iter()
        .find(|(banner_key, _)| banner_key == key)
        .map(|(_, name)| name.clone())
        .unwrap_or_default();
    account.ensure_bucket(key, &name);
}

fn account_from_group(
    entry: &serde_json::Value,
    game: Game,
    data: &GameData,
    ui_lang: &str,
) -> Result<Account, String> {
    let uid = text_of(entry, "uid");
    if uid.is_empty() {
        return Err("missing uid".to_string());
    }
    let mut lang = text_of(entry, "lang");
    if lang.is_empty() {
        lang = ui_lang.to_string();
    }
    lang = GameData::normalize_lang(&lang);
    // Star Rail files may lack names; the embedded dictionary fills them in.
    if game == Game::StarRail && !data.has_item_lang(&lang) {
        let usable = entry
            .get("list")
            .and_then(|v| v.as_array())
            .and_then(|list| list.first())
            .map(|item| {
                !text_of(item, "name").is_empty()
                    && !text_of(item, "item_type").is_empty()
                    && !text_of(item, "rank_type").is_empty()
            })
            .unwrap_or(false);
        if !usable {
            lang = GameData::normalize_lang(ui_lang);
        }
    }

    let mut account = Account::new(uid, game);
    account.lang = lang.clone();
    account.region_time_zone = entry
        .get("timezone")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
        .unwrap_or_else(|| crate::model::timezone_for_uid(&account.uid));
    account.time = chrono::Utc::now().timestamp_millis();

    let banners = data.banners(game, &lang);
    for (key, _) in &banners {
        account.ensure_bucket(key, "");
    }
    let list = entry
        .get("list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for item in &list {
        let bucket_key = if game == Game::Genshin {
            let value = text_of(item, "uigf_gacha_type");
            if value.is_empty() {
                text_of(item, "gacha_type")
            } else {
                value
            }
        } else {
            text_of(item, "gacha_type")
        };
        let mut record = record_from_uigf(item, &bucket_key);
        if game == Game::StarRail && (record.name.is_empty() || record.item_type.is_empty()) {
            if let Some((name, item_type)) = data.item(&lang, &record.item_id) {
                if record.name.is_empty() {
                    record.name = name.clone();
                }
                if record.item_type.is_empty() {
                    record.item_type = item_type.clone();
                }
            }
        }
        ensure_bucket(&mut account, &bucket_key, &banners);
        if let Some(bucket) = account.bucket_mut(&bucket_key) {
            bucket.records.push(record);
        }
    }
    account.buckets.retain(|bucket| !bucket.records.is_empty());
    account.sort_all();
    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load()
    }

    #[test]
    fn imports_uigf_v41_for_both_games() {
        let value = serde_json::json!({
            "info": {"export_timestamp": 1, "export_app": "x", "export_app_version": "v1", "version": "v4.1"},
            "hk4e": [{
                "uid": "100000001", "timezone": 8, "lang": "zh-cn",
                "list": [
                    {"uigf_gacha_type": "301", "gacha_type": "400", "item_id": "1001", "time": "2024-01-01 00:00:00", "name": "A", "item_type": "角色", "rank_type": "5", "id": "1682521800010412950"}
                ]
            }],
            "hkrpg": [{
                "uid": "800000000", "timezone": 8, "lang": "zh-cn",
                "list": [
                    {"gacha_id": "1001", "gacha_type": "11", "item_id": "1001", "count": "1", "time": "2024-01-01 00:00:00", "name": "B", "item_type": "角色", "rank_type": "5", "id": "1682521800010412850"}
                ]
            }]
        });
        let accounts = parse_import(&value, &data(), "zh-cn").unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].game, Game::Genshin);
        assert_eq!(accounts[0].bucket("301").unwrap().records.len(), 1);
        assert_eq!(accounts[1].game, Game::StarRail);
        assert_eq!(accounts[1].bucket("11").unwrap().records.len(), 1);
    }

    #[test]
    fn imports_srgf() {
        let value = serde_json::json!({
            "info": {"uid": "800000000", "lang": "zh-cn", "srgf_version": "v1.0", "region_time_zone": 8},
            "list": [
                {"gacha_id": "1001", "gacha_type": "11", "item_id": "1001", "count": "1", "time": "2024-01-01 00:00:00", "name": "B", "item_type": "角色", "rank_type": "5", "id": "1682521800010412850"}
            ]
        });
        let accounts = parse_import(&value, &data(), "zh-cn").unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].uid, "800000000");
        assert_eq!(accounts[0].bucket("11").unwrap().records.len(), 1);
    }

    #[test]
    fn rejects_unknown_version() {
        let value = serde_json::json!({"info": {"version": "v9.9"}});
        assert!(parse_import(&value, &data(), "zh-cn").is_err());
    }
}
