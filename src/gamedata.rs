//! Static game data: banner (gacha type) names per language and the Star Rail
//! item id dictionary, all embedded in the executable.

use crate::model::Game;
use std::collections::HashMap;

/// Banner list of one game in one language: `(key, localized name)`.
pub type TypeList = Vec<(String, String)>;

pub struct GameData {
    /// game -> language -> banners
    types: HashMap<Game, HashMap<String, TypeList>>,
    /// Star Rail item dictionary: language -> item_id -> (name, item_type)
    items: HashMap<String, HashMap<String, (String, String)>>,
    pub item_db_version: String,
}

impl GameData {
    pub fn load() -> GameData {
        let mut types = HashMap::new();
        types.insert(
            Game::Genshin,
            parse_genshin_types(include_str!("../assets/genshin/gachaType.json")),
        );
        types.insert(
            Game::StarRail,
            parse_starrail_types(include_str!("../assets/starrail/gachaType.json")),
        );

        let mut items: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
        let mut item_db_version = String::new();
        let raw: serde_json::Value = serde_json::from_str(include_str!("../assets/starrail/idJson.json"))
            .unwrap_or(serde_json::Value::Null);
        if let Some(map) = raw.as_object() {
            for (lang, value) in map {
                if lang == "version" {
                    item_db_version = value.as_str().unwrap_or_default().to_string();
                    continue;
                }
                let mut table = HashMap::new();
                if let Some(entries) = value.as_object() {
                    for (id, entry) in entries {
                        let name = entry
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let item_type = entry
                            .get("item_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        table.insert(id.clone(), (name, item_type));
                    }
                }
                items.insert(lang.clone(), table);
            }
        }

        GameData {
            types,
            items,
            item_db_version,
        }
    }

    /// Normalizes a language code such as `en` or `zh` to a known one.
    pub fn normalize_lang(lang: &str) -> String {
        if lang.is_empty() {
            return "zh-cn".to_string();
        }
        let lower = lang.to_ascii_lowercase();
        if lower.contains('-') {
            return lower;
        }
        match lower.as_str() {
            "zh" => "zh-cn",
            "en" => "en-us",
            "ja" => "ja-jp",
            "ko" => "ko-kr",
            "de" => "de-de",
            "es" => "es-es",
            "fr" => "fr-fr",
            "id" => "id-id",
            "pt" => "pt-pt",
            "ru" => "ru-ru",
            "th" => "th-th",
            "vi" => "vi-vn",
            "tr" => "tr-tr",
            "it" => "it-it",
            other => other,
        }
        .to_string()
    }

    /// Banner list for a language, falling back to the base language.
    pub fn banners(&self, game: Game, lang: &str) -> TypeList {
        let lang = Self::normalize_lang(lang);
        let by_lang = self.types.get(&game);
        if let Some(by_lang) = by_lang {
            if let Some(list) = by_lang.get(&lang) {
                return list.clone();
            }
            // Try the language part only (`zh-tw` -> `zh`, `en-GB` -> `en-us`).
            let short = lang.split('-').next().unwrap_or(&lang);
            if let Some(list) = by_lang
                .iter()
                .find(|(code, _)| code.split('-').next() == Some(short))
                .map(|(_, list)| list)
            {
                return list.clone();
            }
            if let Some(list) = by_lang.get("zh-cn") {
                return list.clone();
            }
        }
        Vec::new()
    }

    /// Localized name of one banner.
    pub fn banner_name(&self, game: Game, lang: &str, key: &str) -> String {
        self.banners(game, lang)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, name)| name)
            .unwrap_or_else(|| key.to_string())
    }

    /// Star Rail item lookup used when importing UIGF files.
    pub fn item(&self, lang: &str, item_id: &str) -> Option<&(String, String)> {
        let lang = GameData::normalize_lang(lang);
        if let Some(found) = self.items.get(&lang).and_then(|t| t.get(item_id)) {
            return Some(found);
        }
        for fallback in ["zh-cn", "en-us"] {
            if let Some(found) = self.items.get(fallback).and_then(|t| t.get(item_id)) {
                return Some(found);
            }
        }
        None
    }

    pub fn has_item_lang(&self, lang: &str) -> bool {
        self.items.contains_key(&GameData::normalize_lang(lang))
    }
}

/// Genshin `gachaType.json` is an object: `{ "zh-cn": [ {"key","name"} ] }`.
fn parse_genshin_types(raw: &str) -> HashMap<String, TypeList> {
    // Upstream fetches these five banners in this exact order; type 400 is a
    // second character banner whose pulls land in the 301 bucket.
    const ORDER: [&str; 5] = ["301", "302", "200", "500", "100"];
    let mut out = HashMap::new();
    let value: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
    if let Some(map) = value.as_object() {
        for (lang, list) in map {
            let mut parsed = parse_pairs(list);
            if parsed.len() > ORDER.len() {
                parsed.retain(|(key, _)| ORDER.contains(&key.as_str()));
            }
            sort_by_order(&mut parsed, &ORDER);
            out.insert(lang.clone(), parsed);
        }
    }
    out
}

/// Star Rail `gachaType.json` is an array of `[lang, [ {"key","name"} ]]` pairs.
fn parse_starrail_types(raw: &str) -> HashMap<String, TypeList> {
    // 11/12 normal banners, 1/2 standard and beginner, 21/22 collaboration.
    const ORDER: [&str; 6] = ["11", "12", "21", "22", "1", "2"];
    let mut out = HashMap::new();
    let value: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
    if let Some(list) = value.as_array() {
        for entry in list {
            let Some(pair) = entry.as_array() else { continue };
            let Some(lang) = pair.first().and_then(|v| v.as_str()) else {
                continue;
            };
            let mut parsed = parse_pairs(pair.get(1).unwrap_or(&serde_json::Value::Null));
            sort_by_order(&mut parsed, &ORDER);
            out.insert(lang.to_string(), parsed);
        }
    }
    out
}

fn parse_pairs(value: &serde_json::Value) -> TypeList {
    let mut out = TypeList::new();
    if let Some(list) = value.as_array() {
        for entry in list {
            let key = entry
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if !key.is_empty() {
                out.push((key, name));
            }
        }
    }
    out
}

fn sort_by_order(list: &mut TypeList, order: &[&str]) {
    list.sort_by_key(|(key, _)| {
        order
            .iter()
            .position(|k| k == key)
            .unwrap_or(order.len())
    });
}
