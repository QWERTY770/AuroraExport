//! Translations.
//!
//! The two upstream tools ship 13 languages each. `scripts/merge_i18n.py`
//! merges them into `assets/i18n/merged.json` (grouped into `shared` and
//! per-game maps) and `assets/i18n/app.json` holds the strings that only exist
//! in this merged tool.

use crate::model::Game;
use std::collections::HashMap;

/// Language code and native name, mirroring upstream `langMap`.
pub const LANGS: [(&str, &str); 13] = [
    ("zh-cn", "简体中文"),
    ("zh-tw", "繁體中文"),
    ("de-de", "Deutsch"),
    ("en-us", "English"),
    ("es-es", "Español"),
    ("fr-fr", "Français"),
    ("id-id", "Indonesia"),
    ("ja-jp", "日本語"),
    ("ko-kr", "한국어"),
    ("pt-pt", "Português"),
    ("ru-ru", "Pусский"),
    ("th-th", "ภาษาไทย"),
    ("vi-vn", "Tiếng Việt"),
];

const FALLBACK_LANG: &str = "en-us";
const BASE_LANG: &str = "zh-cn";

pub struct I18n {
    tables: HashMap<String, HashMap<String, String>>,
    lang: String,
}

impl I18n {
    pub fn new(lang: &str) -> I18n {
        let merged: serde_json::Value =
            serde_json::from_str(include_str!("../assets/i18n/merged.json"))
                .expect("merged.json is embedded and valid");
        let app: serde_json::Value = serde_json::from_str(include_str!("../assets/i18n/app.json"))
            .expect("app.json is embedded and valid");

        let mut tables: HashMap<String, HashMap<String, String>> = HashMap::new();
        if let Some(langs) = merged.as_object() {
            for (code, groups) in langs {
                let table = tables.entry(code.clone()).or_default();
                if let Some(groups) = groups.as_object() {
                    for (group, entries) in groups {
                        let prefix = match group.as_str() {
                            "genshin" => Some("genshin."),
                            "starrail" => Some("starrail."),
                            // `shared` keys are used by both games.
                            _ => None,
                        };
                        if let Some(entries) = entries.as_object() {
                            for (key, value) in entries {
                                if let Some(value) = value.as_str() {
                                    let full = match prefix {
                                        Some(p) => format!("{p}{key}"),
                                        None => key.clone(),
                                    };
                                    table.insert(full, value.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(langs) = app.as_object() {
            for (code, entries) in langs {
                let table = tables.entry(code.clone()).or_default();
                if let Some(entries) = entries.as_object() {
                    for (key, value) in entries {
                        if let Some(value) = value.as_str() {
                            table.insert(key.clone(), value.to_string());
                        }
                    }
                }
            }
        }

        let lang = if tables.contains_key(lang) {
            lang.to_string()
        } else {
            BASE_LANG.to_string()
        };
        I18n { tables, lang }
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    fn lookup(&self, lang: &str, key: &str) -> Option<&str> {
        self.tables
            .get(lang)
            .and_then(|table| table.get(key))
            .map(|s| s.as_str())
    }

    /// Game aware lookup with `en-us`/`zh-cn` fallback.
    pub fn t(&self, game: Game, key: &str) -> String {
        let game_key = format!("{}.{}", game.id(), key);
        self.lookup(&self.lang, &game_key)
            .or_else(|| self.lookup(&self.lang, key))
            .or_else(|| self.lookup(FALLBACK_LANG, &game_key))
            .or_else(|| self.lookup(FALLBACK_LANG, key))
            .or_else(|| self.lookup(BASE_LANG, &game_key))
            .or_else(|| self.lookup(BASE_LANG, key))
            .unwrap_or(key)
            .to_string()
    }

    /// Lookup without a game, plus literal fallback for runtime messages.
    pub fn t_any(&self, key: &str) -> String {
        self.lookup(&self.lang, key)
            .or_else(|| self.lookup(FALLBACK_LANG, key))
            .or_else(|| self.lookup(BASE_LANG, key))
            .unwrap_or(key)
            .to_string()
    }

    /// `t` version for messages that may already be literal text.
    pub fn t_or_raw(&self, game: Game, key_or_text: &str) -> String {
        let game_key = format!("{}.{}", game.id(), key_or_text);
        if let Some(found) = self
            .lookup(&self.lang, &game_key)
            .or_else(|| self.lookup(&self.lang, key_or_text))
            .or_else(|| self.lookup(FALLBACK_LANG, &game_key))
            .or_else(|| self.lookup(FALLBACK_LANG, key_or_text))
            .or_else(|| self.lookup(BASE_LANG, &game_key))
            .or_else(|| self.lookup(BASE_LANG, key_or_text))
        {
            return found.to_string();
        }
        key_or_text.to_string()
    }

    /// Replaces `${name}` style placeholders.
    pub fn t_fmt(&self, game: Game, key: &str, args: &[(&str, String)]) -> String {
        let mut text = self.t(game, key);
        for (name, value) in args {
            text = text.replace(&format!("${{{name}}}"), value);
        }
        text
    }

}
