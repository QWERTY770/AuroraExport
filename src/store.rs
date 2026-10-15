//! Paths, configuration, account persistence and the plain-text log.

use crate::model::{Account, Game};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `userData` directory used by the upstream tools.
pub const DATA_DIR_NAME: &str = "userData";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lang: String,
    /// Last selected game.
    pub game: String,
    /// Currently selected UID per game.
    pub current: HashMap<String, String>,
    /// `0` auto, `1` CN, `2` overseas, `3` cloud (Genshin only).
    pub log_type: HashMap<String, i32>,
    pub proxy_port: u16,
    pub proxy_mode: bool,
    pub auto_update: bool,
    pub auto_check_update: bool,
    pub fetch_full_history: bool,
    pub hide_novice: HashMap<String, bool>,
    pub readable_json: bool,
    /// Include the 3 star slice in the pie charts.
    pub pie_include_3: bool,
    /// Window size remembered between runs.
    pub window: Option<[f32; 2]>,
    /// Directory of the game web cache, remembered for the "open cache folder" button.
    pub cache_folder: Option<String>,
    /// `owner/name` of the GitHub repository used by the update check.
    pub update_repo: String,
    /// UIGF version selected in the settings ("4.1" or "3.0").
    pub uigf_version: String,
    /// Export every account instead of only the selected one.
    pub uigf_all_accounts: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut log_type = HashMap::new();
        let mut hide_novice = HashMap::new();
        for game in Game::ALL {
            log_type.insert(game.id().to_string(), 0);
            // Upstream defaults: Genshin hides the novice banner, Star Rail does not.
            hide_novice.insert(game.id().to_string(), game == Game::Genshin);
        }
        Config {
            lang: "zh-cn".to_string(),
            game: Game::StarRail.id().to_string(),
            current: HashMap::new(),
            log_type,
            proxy_port: 8325,
            proxy_mode: false,
            auto_update: true,
            auto_check_update: true,
            fetch_full_history: false,
            hide_novice,
            readable_json: false,
            pie_include_3: false,
            window: None,
            cache_folder: None,
            update_repo: String::new(),
            uigf_version: "4.1".to_string(),
            uigf_all_accounts: true,
        }
    }
}

impl Config {
    pub fn game(&self) -> Game {
        Game::from_id(&self.game).unwrap_or(Game::StarRail)
    }

    pub fn log_type(&self, game: Game) -> i32 {
        *self.log_type.get(game.id()).unwrap_or(&0)
    }

    pub fn set_log_type(&mut self, game: Game, value: i32) {
        self.log_type.insert(game.id().to_string(), value);
    }

    pub fn hide_novice(&self, game: Game) -> bool {
        *self
            .hide_novice
            .get(game.id())
            .unwrap_or(&(game == Game::Genshin))
    }

    pub fn set_hide_novice(&mut self, game: Game, value: bool) {
        self.hide_novice.insert(game.id().to_string(), value);
    }

    pub fn current_uid(&self, game: Game) -> Option<String> {
        self.current
            .get(game.id())
            .filter(|uid| !uid.is_empty() && uid.as_str() != "0")
            .cloned()
    }

    pub fn set_current_uid(&mut self, game: Game, uid: &str) {
        self.current.insert(game.id().to_string(), uid.to_string());
    }

    /// Imports the flat configuration written by the upstream tools.
    fn absorb_legacy(&mut self, value: &serde_json::Value) {
        let Some(map) = value.as_object() else { return };
        if let Some(lang) = map.get("lang").and_then(|v| v.as_str()) {
            if !lang.is_empty() {
                self.lang = lang.to_string();
            }
        }
        if let Some(log_type) = map.get("logType").and_then(|v| v.as_i64()) {
            for game in Game::ALL {
                self.log_type.insert(game.id().to_string(), log_type as i32);
            }
        }
        if let Some(port) = map.get("proxyPort").and_then(|v| v.as_u64()) {
            if port > 0 && port < u16::MAX as u64 {
                self.proxy_port = port as u16;
            }
        }
        if let Some(mode) = map.get("proxyMode").and_then(|v| v.as_bool()) {
            self.proxy_mode = mode;
        }
        if let Some(update) = map.get("autoUpdate").and_then(|v| v.as_bool()) {
            self.auto_update = update;
        }
        if let Some(full) = map.get("fetchFullHistory").and_then(|v| v.as_bool()) {
            self.fetch_full_history = full;
        }
        if let Some(hide) = map.get("hideNovice").and_then(|v| v.as_bool()) {
            for game in Game::ALL {
                self.hide_novice.insert(game.id().to_string(), hide);
            }
        }
        if let Some(readable) = map.get("readableJSON").and_then(|v| v.as_bool()) {
            self.readable_json = readable;
        }
    }
}

/// One loaded account plus the file it came from.
pub struct StoredAccount {
    pub account: Account,
    pub path: PathBuf,
}

pub struct Store {
    pub dir: PathBuf,
    pub accounts: Vec<StoredAccount>,
    pub log_lines: Vec<String>,
    errors: Vec<String>,
}

impl Store {
    pub fn open(dir: PathBuf) -> Store {
        let _ = std::fs::create_dir_all(&dir);
        let mut store = Store {
            dir,
            accounts: Vec::new(),
            log_lines: Vec::new(),
            errors: Vec::new(),
        };
        store.load_accounts();
        store
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    pub fn load_config(&self) -> Config {
        let mut config = Config::default();
        if let Ok(text) = std::fs::read_to_string(self.config_path()) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                // Upstream configs are flat and store the authkey URL (which we
                // deliberately do not keep on disk). Only known settings survive.
                let legacy = value.get("game").is_none();
                if let Ok(parsed) = serde_json::from_value::<Config>(value.clone()) {
                    config = parsed;
                }
                if legacy {
                    config.absorb_legacy(&value);
                }
            }
        }
        if !crate::i18n::LANGS.iter().any(|(code, _)| *code == config.lang) {
            config.lang = "zh-cn".to_string();
        }
        config
    }

    pub fn save_config(&self, config: &Config) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(config).map_err(json_error)?;
        write_atomic(&self.config_path(), text.as_bytes())
    }

    fn account_file_name(uid: &str) -> String {
        format!("gacha-list-{uid}.json")
    }

    fn game_account_file_name(game: Game, uid: &str) -> String {
        format!("gacha-list-{}-{uid}.json", game.id())
    }

    pub fn load_accounts(&mut self) {
        self.accounts.clear();
        self.errors.clear();
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with("gacha-list-") {
                    paths.push(path);
                }
            }
        }
        // Sub-directories per game are also accepted so that a UID shared by
        // both games keeps its own file.
        for game in Game::ALL {
            let dir = self.dir.join(game.data_subdir());
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("gacha-list-") && n.ends_with(".json"))
                    {
                        paths.push(path);
                    }
                }
            }
        }
        paths.sort();

        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                self.errors.push(format!("{}: invalid JSON", path.display()));
                continue;
            };
            // Legacy files carry no game field; infer it from the banner keys
            // and from the file name.
            let hint = value
                .get("game")
                .and_then(|v| v.as_str())
                .and_then(Game::from_id)
                .or_else(|| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| {
                            if n.starts_with("gacha-list-genshin-") {
                                Some(Game::Genshin)
                            } else if n.starts_with("gacha-list-starrail-") {
                                Some(Game::StarRail)
                            } else {
                                None
                            }
                        })
                })
                .or_else(|| detect_game(&value));
            match Account::from_json(&value, hint) {
                Some(account) => {
                    if self
                        .accounts
                        .iter()
                        .any(|a| a.account.uid == account.uid && a.account.game == account.game)
                    {
                        continue;
                    }
                    self.accounts.push(StoredAccount { account, path });
                }
                None => self
                    .errors
                    .push(format!("{}: unrecognized account data", path.display())),
            }
        }
        self.accounts
            .sort_by(|a, b| (a.account.game.id(), &a.account.uid).cmp(&(b.account.game.id(), &b.account.uid)));
    }

    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Fills in banner names that are missing from a stored file.
    ///
    /// Data imported by older builds could end up with unnamed banners, which
    /// shows up as an empty banner title; loading repairs it once.
    pub fn repair_banner_names(&mut self, data: &crate::gamedata::GameData) -> usize {
        let mut fixed: Vec<Account> = Vec::new();
        for stored in &self.accounts {
            let mut account = stored.account.clone();
            let banners = data.banners(account.game, &account.lang);
            let mut changed = false;
            for bucket in account.buckets.iter_mut() {
                if bucket.name.is_empty() {
                    if let Some((_, name)) = banners.iter().find(|(key, _)| key == &bucket.key) {
                        bucket.name = name.clone();
                        changed = true;
                    }
                }
            }
            if changed {
                fixed.push(account);
            }
        }
        let count = fixed.len();
        for account in fixed {
            let _ = self.upsert(account, false);
        }
        count
    }

    pub fn get(&self, game: Game, uid: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.account.game == game && a.account.uid == uid)
            .map(|a| &a.account)
    }

    pub fn get_mut(&mut self, game: Game, uid: &str) -> Option<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.account.game == game && a.account.uid == uid)
            .map(|a| &mut a.account)
    }

    pub fn accounts_of(&self, game: Game) -> Vec<&Account> {
        self.accounts
            .iter()
            .filter(|a| a.account.game == game)
            .map(|a| &a.account)
            .collect()
    }

    /// Inserts or replaces an account and writes it to disk.
    pub fn upsert(&mut self, account: Account, readable: bool) -> std::io::Result<PathBuf> {
        let path = self.path_for(&account);
        self.write_account(&account, &path, readable)?;
        match self
            .accounts
            .iter_mut()
            .find(|a| a.account.game == account.game && a.account.uid == account.uid)
        {
            Some(existing) => {
                existing.account = account;
                existing.path = path.clone();
            }
            None => self.accounts.push(StoredAccount { account, path: path.clone() }),
        }
        Ok(path)
    }

    fn path_for(&self, account: &Account) -> PathBuf {
        if let Some(existing) = self
            .accounts
            .iter()
            .find(|a| a.account.game == account.game && a.account.uid == account.uid)
        {
            return existing.path.clone();
        }
        let default = self.dir.join(Self::account_file_name(&account.uid));
        if default.exists() {
            // A file for this UID already exists but belongs to the other game.
            let other_game = std::fs::read_to_string(&default)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|value| {
                    value
                        .get("game")
                        .and_then(|v| v.as_str())
                        .and_then(Game::from_id)
                        .or_else(|| detect_game(&value))
                });
            if other_game.is_some_and(|game| game != account.game) {
                return self
                    .dir
                    .join(Self::game_account_file_name(account.game, &account.uid));
            }
        }
        default
    }

    pub fn write_account(
        &self,
        account: &Account,
        path: &Path,
        readable: bool,
    ) -> std::io::Result<()> {
        let value = account.to_json();
        let text = if readable {
            serde_json::to_string_pretty(&value).map_err(json_error)?
        } else {
            serde_json::to_string(&value).map_err(json_error)?
        };
        write_atomic(path, text.as_bytes())
    }

    /// Copies the current file of an account into `backup/<uid>/` before it is
    /// overwritten (used by the importers).
    pub fn backup(&self, uid: &str) -> std::io::Result<Option<PathBuf>> {
        let Some(existing) = self.accounts.iter().find(|a| a.account.uid == uid) else {
            return Ok(None);
        };
        if !existing.path.exists() {
            return Ok(None);
        }
        let dir = self.dir.join("backup").join(uid);
        std::fs::create_dir_all(&dir)?;
        let name = format!("gacha-list-{uid}-{}.json", timestamp_compact());
        let target = dir.join(name);
        std::fs::copy(&existing.path, &target)?;
        Ok(Some(target))
    }

    pub fn log(&mut self, kind: &str, text: &str) {
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let line = format!("[{kind}][{stamp}]{text}");
        self.log_lines.push(line.clone());
        if self.log_lines.len() > 500 {
            self.log_lines.drain(0..100);
        }
        let path = self.dir.join("log.txt");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

pub fn detect_game(value: &serde_json::Value) -> Option<Game> {
    let type_map = value.get("typeMap")?.as_array()?;
    let mut keys = Vec::new();
    for entry in type_map {
        if let Some(key) = entry.as_array().and_then(|pair| pair.first()).and_then(|v| v.as_str()) {
            keys.push(key.to_string());
        } else if let Some(key) = entry.get("key").and_then(|v| v.as_str()) {
            keys.push(key.to_string());
        }
    }
    if keys.iter().any(|k| k == "301" || k == "302" || k == "500") {
        return Some(Game::Genshin);
    }
    if keys.iter().any(|k| k == "11" || k == "12" || k == "21" || k == "22") {
        return Some(Game::StarRail);
    }
    // Fall back to the record shape: Star Rail files store objects.
    let first = value
        .get("result")?
        .as_array()?
        .first()?
        .as_array()?
        .get(1)?
        .as_array()?
        .first()?
        .clone();
    if first.is_object() {
        Some(Game::StarRail)
    } else {
        None
    }
}

fn json_error(err: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, err)
}

/// Writes a file through a temporary neighbour so a crash cannot truncate it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}

pub fn timestamp_compact() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

pub fn timestamp_minute() -> String {
    chrono::Local::now().format("%Y%m%d%H%M").to_string()
}

/// Picks the directory that holds `config.json`, `gacha-list-*.json` and the log.
///
/// A portable `userData` folder next to the executable is preferred so the tool
/// behaves like the upstream portable builds; when that location is not
/// writable (for example when installed into Program Files) the OS data
/// directory is used instead.
pub fn resolve_data_dir(override_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir.to_path_buf();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join(DATA_DIR_NAME);
            if dir_is_writable(&candidate) {
                return candidate;
            }
        }
    }
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let legacy = base.join("AuroraWarpExport");
    let current = base.join("AuroraExport");
    if legacy.is_dir() && !current.exists() {
        legacy
    } else {
        current
    }
}

fn dir_is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    crate::file_dialog::writable_directory(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_is_absorbed() {
        let value = serde_json::json!({
            "logType": 2,
            "lang": "en-us",
            "proxyPort": 8325,
            "hideNovice": false,
            "readableJSON": true
        });
        let mut config = Config::default();
        config.absorb_legacy(&value);
        assert_eq!(config.lang, "en-us");
        assert_eq!(config.log_type(Game::Genshin), 2);
        assert!(!config.hide_novice(Game::StarRail));
        assert!(config.readable_json);
    }

    #[test]
    fn legacy_config_keeps_defaults_for_missing_fields() {
        // The upstream config has a flat `current` string and camelCase keys;
        // unknown/mismatched fields must not wipe the defaults.
        let value = serde_json::json!({
            "urls": [],
            "logType": 0,
            "lang": "zh-cn",
            "current": "841347454",
            "proxyPort": 8325,
            "proxyMode": false,
            "autoUpdate": true,
            "fetchFullHistory": false,
            "hideNovice": true
        });
        let mut config = Config::default();
        if let Ok(parsed) = serde_json::from_value::<Config>(value.clone()) {
            config = parsed;
        }
        config.absorb_legacy(&value);
        assert!(config.auto_check_update, "auto check update must stay enabled");
        assert!(config.auto_update);
        assert_eq!(config.lang, "zh-cn");
        assert_eq!(config.proxy_port, 8325);
    }

    #[test]
    fn detects_game_from_banner_keys() {
        let genshin = serde_json::json!({ "typeMap": [["301", "x"], ["200", "y"]] });
        let starrail = serde_json::json!({ "typeMap": [["11", "x"], ["1", "y"]] });
        assert_eq!(detect_game(&genshin), Some(Game::Genshin));
        assert_eq!(detect_game(&starrail), Some(Game::StarRail));
    }
}
