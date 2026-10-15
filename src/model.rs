//! Core data model: games, gacha records and per-account data.

use serde::{Deserialize, Serialize};

/// Which game an account belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    Genshin,
    StarRail,
}

impl Game {
    pub const ALL: [Game; 2] = [Game::Genshin, Game::StarRail];

    pub fn id(self) -> &'static str {
        match self {
            Game::Genshin => "genshin",
            Game::StarRail => "starrail",
        }
    }

    pub fn from_id(s: &str) -> Option<Game> {
        match s {
            "genshin" | "hk4e" => Some(Game::Genshin),
            "starrail" | "hkrpg" => Some(Game::StarRail),
            _ => None,
        }
    }

    /// i18n key of the display name.
    pub fn name_key(self) -> &'static str {
        match self {
            Game::Genshin => "ui.game.genshin",
            Game::StarRail => "ui.game.starrail",
        }
    }

    /// Gacha type key of the beginner banner.
    pub fn novice_key(self) -> &'static str {
        match self {
            Game::Genshin => "100",
            Game::StarRail => "2",
        }
    }

    /// `game_biz` prefix used inside the captured URL.
    pub fn biz_prefix(self) -> &'static str {
        match self {
            Game::Genshin => "hk4e_",
            Game::StarRail => "hkrpg_",
        }
    }

    /// Folder holding the account files, relative to the data directory.
    pub fn data_subdir(self) -> &'static str {
        match self {
            Game::Genshin => "genshin",
            Game::StarRail => "starrail",
        }
    }
}

/// One gacha pull.
///
/// The struct is stored on disk as a positional JSON array to keep the files
/// small; the first six positions are identical to the legacy
/// `genshin-wish-export` layout, so old files keep working.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub time: String,
    pub name: String,
    pub item_type: String,
    pub rank: i64,
    pub gacha_type: String,
    pub id: String,
    pub item_id: String,
    pub gacha_id: String,
    pub count: String,
}

impl Record {
    pub fn to_json(&self) -> serde_json::Value {
        let mut arr: Vec<serde_json::Value> = vec![
            self.time.clone().into(),
            self.name.clone().into(),
            self.item_type.clone().into(),
            self.rank.into(),
            self.gacha_type.clone().into(),
            self.id.clone().into(),
        ];
        if !self.item_id.is_empty() || !self.gacha_id.is_empty() || !self.count.is_empty() {
            arr.push(self.item_id.clone().into());
            arr.push(self.gacha_id.clone().into());
            arr.push(self.count.clone().into());
        }
        serde_json::Value::Array(arr)
    }

    /// Parses a stored record. Accepts both the compact array layout and the
    /// object layout used by `star-rail-warp-export`.
    pub fn from_json(value: &serde_json::Value) -> Option<Record> {
        match value {
            serde_json::Value::Array(arr) => {
                if arr.len() < 4 {
                    return None;
                }
                let s = |i: usize| -> String {
                    arr.get(i)
                        .and_then(|v| match v {
                            serde_json::Value::String(s) => Some(s.clone()),
                            serde_json::Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                Some(Record {
                    time: s(0),
                    name: s(1),
                    item_type: s(2),
                    rank: rank_of(&arr.get(3).cloned().unwrap_or_default()),
                    gacha_type: s(4),
                    id: s(5),
                    item_id: s(6),
                    gacha_id: s(7),
                    count: s(8),
                })
            }
            serde_json::Value::Object(map) => {
                let g = |k: &str| -> String {
                    map.get(k)
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default()
                };
                Some(Record {
                    time: g("time"),
                    name: g("name"),
                    item_type: g("item_type"),
                    rank: rank_of(map.get("rank_type").unwrap_or(&serde_json::Value::Null)),
                    gacha_type: g("gacha_type"),
                    id: g("id"),
                    item_id: g("item_id"),
                    gacha_id: g("gacha_id"),
                    count: g("count"),
                })
            }
            _ => None,
        }
    }

    /// Numeric sort key for the 19 digit ids. Returns `None` when the id is not
    /// numeric, in which case callers fall back to a string comparison.
    pub fn sort_key(&self) -> Option<u128> {
        if self.id.is_empty() || !self.id.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            self.id.parse::<u128>().ok()
        }
    }
}

fn rank_of(value: &serde_json::Value) -> i64 {
    match value {
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0),
        serde_json::Value::String(s) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

/// One gacha banner inside an account: its key, localized name and records.
#[derive(Clone, Debug, Default)]
pub struct Bucket {
    pub key: String,
    pub name: String,
    pub records: Vec<Record>,
}

/// Everything known about one game account (one UID).
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub uid: String,
    pub game: Game,
    pub lang: String,
    pub region: String,
    pub region_time_zone: i32,
    /// Milliseconds of the last successful update.
    pub time: i64,
    /// Soft delete flag used by the data manager.
    pub deleted: bool,
    pub buckets: Vec<Bucket>,
}

impl Default for Game {
    fn default() -> Self {
        Game::StarRail
    }
}

impl Account {
    pub fn new(uid: impl Into<String>, game: Game) -> Account {
        Account {
            uid: uid.into(),
            game,
            region_time_zone: default_timezone(&[]),
            ..Default::default()
        }
    }

    pub fn bucket(&self, key: &str) -> Option<&Bucket> {
        self.buckets.iter().find(|b| b.key == key)
    }

    pub fn bucket_mut(&mut self, key: &str) -> Option<&mut Bucket> {
        self.buckets.iter_mut().find(|b| b.key == key)
    }

    pub fn ensure_bucket(&mut self, key: &str, name: &str) -> &mut Bucket {
        if let Some(pos) = self.buckets.iter().position(|b| b.key == key) {
            if self.buckets[pos].name.is_empty() && !name.is_empty() {
                self.buckets[pos].name = name.to_string();
            }
            return &mut self.buckets[pos];
        }
        self.buckets.push(Bucket {
            key: key.to_string(),
            name: name.to_string(),
            records: Vec::new(),
        });
        self.buckets.last_mut().expect("just pushed")
    }

    pub fn total_records(&self) -> usize {
        self.buckets.iter().map(|b| b.records.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total_records() == 0
    }

    pub fn to_json(&self) -> serde_json::Value {
        let type_map: Vec<serde_json::Value> = self
            .buckets
            .iter()
            .map(|b| serde_json::json!([b.key, b.name]))
            .collect();
        let result: Vec<serde_json::Value> = self
            .buckets
            .iter()
            .map(|b| {
                let records: Vec<serde_json::Value> =
                    b.records.iter().map(|r| r.to_json()).collect();
                serde_json::json!([b.key, records])
            })
            .collect();
        serde_json::json!({
            "format": "aurora-v1",
            "game": self.game.id(),
            "time": self.time,
            "uid": self.uid,
            "lang": self.lang,
            "region": self.region,
            "region_time_zone": self.region_time_zone,
            "deleted": self.deleted,
            "typeMap": type_map,
            "result": result,
        })
    }

    pub fn from_json(value: &serde_json::Value, hint_game: Option<Game>) -> Option<Account> {
        let obj = value.as_object()?;
        let uid = obj
            .get("uid")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        if uid.is_empty() || uid == "0" {
            return None;
        }
        let game = obj
            .get("game")
            .and_then(|v| v.as_str())
            .and_then(Game::from_id)
            .or(hint_game)?;

        let lang = obj
            .get("lang")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let region = obj
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let time = obj.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        let deleted = obj
            .get("deleted")
            .map(|v| match v {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::String(s) => s == "true",
                serde_json::Value::Null => false,
                other => other.as_i64().unwrap_or(0) != 0,
            })
            .unwrap_or(false);

        // Banner names, in stored order.
        let mut names: Vec<(String, String)> = Vec::new();
        if let Some(serde_json::Value::Array(list)) = obj.get("typeMap") {
            for entry in list {
                if let serde_json::Value::Array(pair) = entry {
                    let key = pair.first().and_then(|v| v.as_str()).unwrap_or_default();
                    let name = pair.get(1).and_then(|v| v.as_str()).unwrap_or_default();
                    if !key.is_empty() {
                        names.push((key.to_string(), name.to_string()));
                    }
                }
            }
        }

        let mut account = Account {
            uid,
            game,
            lang,
            region,
            region_time_zone: obj
                .get("region_time_zone")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| default_timezone(&[])),
            time,
            deleted,
            buckets: Vec::new(),
        };

        if let Some(serde_json::Value::Array(list)) = obj.get("result") {
            for entry in list {
                let (key, raw_records) = match entry {
                    serde_json::Value::Array(pair) => {
                        let key = pair.first().and_then(|v| v.as_str()).unwrap_or_default();
                        let records = match pair.get(1) {
                            Some(serde_json::Value::Array(items)) => items.clone(),
                            _ => Vec::new(),
                        };
                        (key.to_string(), records)
                    }
                    serde_json::Value::Object(map) => {
                        let key = map
                            .get("key")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let records = match map.get("records") {
                            Some(serde_json::Value::Array(items)) => items.clone(),
                            _ => Vec::new(),
                        };
                        (key, records)
                    }
                    _ => continue,
                };
                if key.is_empty() {
                    continue;
                }
                let name = names
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                let bucket = account.ensure_bucket(&key, &name);
                for raw in &raw_records {
                    if let Some(record) = Record::from_json(raw) {
                        bucket.records.push(record);
                    }
                }
            }
        }

        // Legacy genshin files store no `deleted`; default timezone when missing.
        if account.region_time_zone == 0 && account.lang.is_empty() {
            account.region_time_zone = default_timezone(&[account.uid.clone()]);
        }
        account.sort_all();
        Some(account)
    }

    /// Sorts every banner chronologically (ascending by id, falling back to time).
    pub fn sort_all(&mut self) {
        for bucket in &mut self.buckets {
            sort_records(&mut bucket.records);
        }
    }
}

/// Timezone offset derived from the UID prefix, mirroring upstream behaviour:
/// 6 -> America (-5), 7 -> Europe (+1), everything else -> Asia (+8).
pub fn default_timezone(_uids: &[String]) -> i32 {
    8
}

pub fn timezone_for_uid(uid: &str) -> i32 {
    match uid.chars().next() {
        Some('6') => -5,
        Some('7') => 1,
        _ => 8,
    }
}

/// Ascending order by numeric id; records without a usable id keep their order.
pub fn sort_records(records: &mut [Record]) {
    records.sort_by(|a, b| match (a.sort_key(), b.sort_key()) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.time.cmp(&b.time),
    });
}

/// Removes records whose id already exists, preferring the incoming record.
pub fn merge_records(local: &[Record], incoming: &[Record]) -> Vec<Record> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<Record> = Vec::with_capacity(local.len() + incoming.len());
    for record in incoming.iter().chain(local.iter()) {
        let key = if record.id.is_empty() {
            format!("{}|{}|{}", record.time, record.name, record.gacha_type)
        } else {
            record.id.clone()
        };
        if seen.insert(key) {
            out.push(record.clone());
        }
    }
    sort_records(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, name: &str) -> Record {
        Record {
            id: id.to_string(),
            name: name.to_string(),
            rank: 5,
            time: format!("2024-01-01 00:00:{:02}", id.len()),
            ..Default::default()
        }
    }

    #[test]
    fn merge_prefers_incoming_and_sorts() {
        let local = vec![rec("1682521800010412900", "old")];
        let incoming = vec![
            rec("1682521800010412950", "new"),
            rec("1682521800010412900", "refreshed"),
        ];
        let merged = merge_records(&local, &incoming);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "refreshed");
        assert_eq!(merged[1].name, "new");
    }

    #[test]
    fn record_roundtrip() {
        let record = Record {
            time: "2024-05-01 12:00:00".into(),
            name: "测试".into(),
            item_type: "角色".into(),
            rank: 5,
            gacha_type: "301".into(),
            id: "1682521800010412950".into(),
            item_id: "1001".into(),
            gacha_id: "1001".into(),
            count: "1".into(),
        };
        let parsed = Record::from_json(&record.to_json()).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn parses_legacy_object_records() {
        let value = serde_json::json!({
            "time": "2024-05-01 12:00:00",
            "name": "希儿",
            "item_type": "角色",
            "rank_type": "5",
            "gacha_type": "11",
            "id": "1682521800010412950",
            "item_id": "1001",
            "gacha_id": "1001",
            "count": "1"
        });
        let parsed = Record::from_json(&value).unwrap();
        assert_eq!(parsed.name, "希儿");
        assert_eq!(parsed.rank, 5);
        assert_eq!(parsed.item_id, "1001");
    }
}
