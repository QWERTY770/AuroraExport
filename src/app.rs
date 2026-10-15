//! Application state and the eframe glue.

use crate::fetch::{FetchOptions, Fetched};
use crate::gamedata::GameData;
use crate::i18n::I18n;
use crate::jobs::{self, Job, Msg, Outcome, UrlSource};
use crate::model::{merge_records, Account, Bucket, Game};
use crate::stats::{self, BannerStats};
use crate::store::{Config, Store};
use crate::ui;
use crate::update::Release;
use egui::{Align2, Context, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Detail,
}

pub enum Dialog {
    None,
    ManualUrl(String),
    CacheHint,
    DataManager,
    File(crate::file_dialog::Request),
    ExportError {
        purpose: crate::file_dialog::Purpose,
        path: PathBuf,
        text: String,
    },
    Message {
        title: String,
        text: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Loading,
    Loaded,
    Failed,
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub until: Instant,
}

pub struct App {
    pub store: Store,
    pub config: Config,
    pub game: Game,
    pub i18n: Arc<I18n>,
    pub data: Arc<GameData>,
    pub view: View,
    pub detail_key: Option<String>,
    pub show_settings: bool,
    pub dialog: Dialog,
    pub status: Status,
    pub status_text: String,
    pub job: Option<Job>,
    pub toast: Option<Toast>,
    pub auth_timeout: bool,
    pub update: Option<Release>,
    pub stats_cache: Option<(Game, String, Arc<Vec<BannerStats>>)>,
    /// Clipboard writes are queued here and flushed with the egui context.
    pending_clipboard: Option<String>,
    open_results: Vec<(String, std::sync::mpsc::Receiver<Result<(), String>>)>,
    /// `--screenshot <path>`: capture one frame and quit.
    screenshot_path: Option<PathBuf>,
    screenshot_frames: u32,
    screenshot_requested: bool,
    fonts_installed: bool,
    startup_update_checked: bool,
    last_window_save: Instant,
}

impl App {
    pub fn new(store: Store, config: Config, data: Arc<GameData>) -> App {
        let game = config.game();
        let i18n = Arc::new(I18n::new(&config.lang));
        let mut app = App {
            store,
            config,
            game,
            i18n,
            data,
            view: View::Overview,
            detail_key: None,
            show_settings: false,
            dialog: Dialog::None,
            status: Status::Idle,
            status_text: String::new(),
            job: None,
            toast: None,
            auth_timeout: false,
            update: None,
            stats_cache: None,
            pending_clipboard: None,
            open_results: Vec::new(),
            screenshot_path: None,
            screenshot_frames: 0,
            screenshot_requested: false,
            fonts_installed: false,
            startup_update_checked: false,
            last_window_save: Instant::now(),
        };
        if app.current_uid().is_none() {
            let first = app
                .store
                .accounts_of(app.game)
                .first()
                .map(|account| account.uid.clone());
            if let Some(uid) = first {
                app.config.set_current_uid(app.game, &uid);
            }
        }
        let loaded = app.account().is_some_and(|account| !account.is_empty());
        if loaded {
            app.status = Status::Loaded;
        }
        // First run: prefer the game that actually has local data.
        if app.store.accounts_of(app.game).is_empty() {
            let other = match app.game {
                Game::Genshin => Game::StarRail,
                Game::StarRail => Game::Genshin,
            };
            if !app.store.accounts_of(other).is_empty() {
                app.set_game(other);
            }
        }
        for error in app.store.take_errors() {
            app.store.log("ERROR", &error);
        }
        // Repair data imported by older builds (banners without a name).
        let repaired = app.store.repair_banner_names(&app.data);
        if repaired > 0 {
            app.store.log(
                "INFO",
                &format!("repaired banner names in {repaired} account(s)"),
            );
        }
        app
    }

    // ---------------------------------------------------------------- helpers

    pub fn save_config(&self) {
        let _ = self.store.save_config(&self.config);
    }

    pub fn set_language(&mut self, lang: &str) {
        self.config.lang = lang.to_string();
        self.i18n = Arc::new(I18n::new(lang));
    }

    pub fn set_game(&mut self, game: Game) {
        if self.game == game {
            return;
        }
        self.game = game;
        self.config.game = game.id().to_string();
        self.view = View::Overview;
        self.detail_key = None;
        self.stats_cache = None;
        self.status_text.clear();
        self.auth_timeout = false;
        self.status = if self.account().is_some_and(|account| !account.is_empty()) {
            Status::Loaded
        } else {
            Status::Idle
        };
        self.save_config();
    }

    pub fn current_uid(&self) -> Option<String> {
        let uid = self.config.current_uid(self.game)?;
        if self.store.get(self.game, &uid).is_some() {
            Some(uid)
        } else {
            None
        }
    }

    pub fn account(&self) -> Option<&Account> {
        let uid = self.current_uid()?;
        self.store.get(self.game, &uid)
    }

    pub fn visible_accounts(&self) -> Vec<&Account> {
        self.store
            .accounts_of(self.game)
            .into_iter()
            .filter(|account| !account.deleted)
            .collect()
    }

    pub fn refresh_stats(&mut self) {
        self.stats_cache = None;
    }

    pub fn stats_snapshot(&mut self) -> Arc<Vec<BannerStats>> {
        let uid = self.current_uid().unwrap_or_default();
        if let Some((game, cached_uid, stats)) = &self.stats_cache {
            if *game == self.game && *cached_uid == uid {
                return stats.clone();
            }
        }
        let stats = match self.store.get(self.game, &uid) {
            Some(account) => stats::compute_all(account),
            None => Vec::new(),
        };
        let stats = Arc::new(stats);
        self.stats_cache = Some((self.game, uid, stats.clone()));
        stats
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_secs(if error { 8 } else { 4 }),
        });
    }

    pub fn uigf_versions(&self) -> Vec<&'static str> {
        match self.game {
            Game::Genshin => vec!["4.1", "3.0"],
            Game::StarRail => vec!["4.1"],
        }
    }

    // ---------------------------------------------------------------- actions

    pub fn start_fetch_auto(&mut self) {
        self.start_fetch(UrlSource::Auto);
    }

    pub fn start_fetch_manual(&mut self, url: String) {
        self.start_fetch(UrlSource::Manual(url));
    }

    pub fn start_fetch_proxy(&mut self) {
        self.start_fetch(UrlSource::Proxy);
    }

    fn start_fetch(&mut self, source: UrlSource) {
        let game = self.game;
        if self.job.is_some() {
            let text = self.i18n.t(game, "ui.hint.loading");
            self.toast(text, true);
            return;
        }
        let uid_hint = self.current_uid();
        let mut local_latest: HashMap<String, String> = HashMap::new();
        if let Some(uid) = &uid_hint {
            if let Some(account) = self.store.get(game, uid) {
                for bucket in &account.buckets {
                    if let Some(id) = bucket.records.last().map(|record| record.id.clone()) {
                        if !id.is_empty() {
                            local_latest.insert(bucket.key.clone(), id);
                        }
                    }
                }
            }
        }
        let request = jobs::FetchRequest {
            game,
            source,
            options: FetchOptions {
                full_history: self.config.fetch_full_history,
                local_latest,
                uid_hint,
            },
            data: self.data.clone(),
            log_type: self.config.log_type(game),
            proxy_port: self.config.proxy_port,
        };
        self.job = Some(jobs::spawn_fetch(request));
        self.status = Status::Loading;
        self.status_text = self.i18n.t(game, "ui.hint.loading");
        self.auth_timeout = false;
    }

    pub fn action_new_account(&mut self) {
        self.config.set_current_uid(self.game, "0");
        self.status = Status::Idle;
        self.status_text.clear();
        self.detail_key = None;
        self.save_config();
    }

    pub fn action_change_uid(&mut self, uid: &str) {
        self.config.set_current_uid(self.game, uid);
        self.refresh_stats();
        self.status = if uid == "0" {
            Status::Idle
        } else {
            Status::Loaded
        };
        self.status_text.clear();
        self.detail_key = None;
        self.auth_timeout = false;
        self.save_config();
    }

    /// Copies a freshly captured authkey URL. The URL is never stored on disk:
    /// it is re-read from the game cache when needed.
    pub fn action_copy_url(&mut self) {
        let game = self.game;
        match crate::urltools::find_url(game, self.config.log_type(game)) {
            Ok(found) => {
                self.pending_clipboard = Some(found.url);
                if let Some(folder) = found.cache_folder {
                    self.config.cache_folder = Some(folder.display().to_string());
                    self.save_config();
                }
                let text = self.i18n.t(game, "ui.extra.urlCopied");
                self.toast(text, false);
            }
            Err(key) => {
                let text = self.i18n.t_or_raw(game, &key);
                self.toast(text, true);
            }
        }
    }

    pub fn action_open_data_dir(&mut self) {
        let dir = self.store.dir.clone();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.report_open_error(&dir.display().to_string(), error.to_string());
            return;
        }
        self.open_path(&dir);
    }

    /// Opens a path with the OS handler and reports failures instead of
    /// silently ignoring them (a silent failure looks like "nothing happens").
    fn open_path(&mut self, path: &std::path::Path) {
        let text = path.display().to_string();
        self.open_target(&text);
    }

    pub fn open_target(&mut self, target: &str) {
        let target = target.to_string();
        let worker_target = target.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        match std::thread::Builder::new()
            .name("aurora-launcher".into())
            .spawn(move || {
                let _ = sender.send(crate::platform::open_target(&worker_target));
            }) {
            Ok(_) => self.open_results.push((target, receiver)),
            Err(error) => self.report_open_error(&target, error.to_string()),
        }
    }

    fn report_open_error(&mut self, target: &str, error: String) {
        let game = self.game;
        let message = format!("{}: {error}", self.i18n.t(game, "log.open.failed"));
        self.log_error(&message);
        let title = self.i18n.t(game, "ui.hint.failed");
        self.dialog = Dialog::Message {
            title,
            text: format!("{message}\n\n{target}"),
        };
    }

    fn poll_open_targets(&mut self, ctx: &Context) {
        let mut failures = Vec::new();
        self.open_results
            .retain(|(target, receiver)| match receiver.try_recv() {
                Ok(Ok(())) => false,
                Ok(Err(error)) => {
                    failures.push((target.clone(), error));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    failures.push((target.clone(), "system launcher stopped".into()));
                    false
                }
            });
        for (target, error) in failures {
            self.report_open_error(&target, error);
        }
        if !self.open_results.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub fn retry_export(&mut self, purpose: crate::file_dialog::Purpose, path: PathBuf) {
        let (key, extension) = match purpose {
            crate::file_dialog::Purpose::ExportExcel => ("ui.button.excel", "xlsx"),
            _ => ("ui.button.uigf", "json"),
        };
        self.dialog = Dialog::File(crate::file_dialog::Request {
            purpose,
            title: self.i18n.t(self.game, key),
            directory: crate::file_dialog::export_directory(&self.store.dir),
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            extension,
        });
    }

    pub fn action_open_cache_folder(&mut self) {
        let game = self.game;
        let found = crate::urltools::find_cache_dir(game).ok();
        let dir = found.or_else(|| {
            self.config
                .cache_folder
                .as_ref()
                .map(PathBuf::from)
                .filter(|path| path.exists())
        });
        match dir {
            Some(dir) => {
                self.config.cache_folder = Some(dir.display().to_string());
                self.save_config();
                self.open_path(&dir);
            }
            None => {
                let text = self.i18n.t(game, "log.file.notFound");
                self.toast(text, true);
            }
        }
    }

    pub fn action_clear_system_proxy(&mut self) {
        match crate::proxy::clear_system_proxy() {
            Ok(()) => {
                let text = self.i18n.t(self.game, "log.data.saved");
                self.toast(text, false);
            }
            Err(err) => self.toast(err, true),
        }
    }

    pub fn action_toggle_deleted(&mut self, uid: &str, deleted: bool) {
        let game = self.game;
        let updated = self.store.get_mut(game, uid).map(|account| {
            account.deleted = deleted;
            account.clone()
        });
        if let Some(account) = updated {
            let readable = self.config.readable_json;
            if let Err(err) = self.store.upsert(account, readable) {
                self.toast(err.to_string(), true);
            }
        }
        let key = if deleted {
            "log.data.deleted"
        } else {
            "log.data.restored"
        };
        let text = self.i18n.t(game, key);
        self.toast(text, false);
        self.refresh_stats();
        if self.visible_accounts().is_empty() {
            self.action_new_account();
        } else if self.current_uid().is_none() {
            let first = self
                .visible_accounts()
                .first()
                .map(|account| account.uid.clone());
            if let Some(uid) = first {
                self.action_change_uid(&uid);
            }
        }
    }

    pub fn action_export_excel(&mut self) {
        let game = self.game;
        if self.job.is_some() {
            return;
        }
        let account = match self.account() {
            Some(account) if !account.is_empty() => account.clone(),
            _ => {
                let text = self.i18n.t(game, "log.export.empty");
                self.toast(text, true);
                return;
            }
        };
        let _ = account;
        let default_name = format!("{}.xlsx", crate::excel::default_file_name(&self.i18n, game));
        let title = self.i18n.t(game, "ui.button.excel");
        self.dialog = Dialog::File(crate::file_dialog::Request {
            purpose: crate::file_dialog::Purpose::ExportExcel,
            title,
            directory: crate::file_dialog::export_directory(&self.store.dir),
            file_name: default_name,
            extension: "xlsx",
        });
    }

    pub fn action_export_uigf(&mut self) {
        let game = self.game;
        if self.job.is_some() {
            return;
        }
        let version = crate::uigf::UigfVersion::parse(&self.config.uigf_version);
        if version == crate::uigf::UigfVersion::V30 && game != Game::Genshin {
            let text = self.i18n.t(game, "log.export.failed");
            self.toast(text, true);
            return;
        }
        let accounts: Vec<Account> = if self.config.uigf_all_accounts {
            self.visible_accounts()
                .into_iter()
                .filter(|account| !account.is_empty())
                .cloned()
                .collect()
        } else {
            self.account()
                .filter(|account| !account.is_empty())
                .cloned()
                .into_iter()
                .collect()
        };
        if accounts.is_empty() {
            let text = self.i18n.t(game, "log.export.empty");
            self.toast(text, true);
            return;
        }
        let refs: Vec<&Account> = accounts.iter().collect();
        let default_name = crate::uigf::default_file_name(version, &refs);
        let title = self.i18n.t(game, "ui.button.uigf");
        self.dialog = Dialog::File(crate::file_dialog::Request {
            purpose: crate::file_dialog::Purpose::ExportUigf,
            title,
            directory: crate::file_dialog::export_directory(&self.store.dir),
            file_name: default_name,
            extension: "json",
        });
    }

    pub fn action_import_json(&mut self) {
        let game = self.game;
        if self.job.is_some() {
            return;
        }
        let title = self.i18n.t(game, "ui.button.import");
        self.dialog = Dialog::File(crate::file_dialog::Request {
            purpose: crate::file_dialog::Purpose::ImportJson,
            title,
            directory: crate::file_dialog::export_directory(&self.store.dir),
            file_name: String::new(),
            extension: "json",
        });
    }

    fn file_selected(&mut self, purpose: crate::file_dialog::Purpose, path: PathBuf) {
        use crate::file_dialog::Purpose;
        match purpose {
            Purpose::ExportExcel => self.start_excel_export(path),
            Purpose::ExportUigf => self.start_uigf_export(path),
            Purpose::ImportJson => self.start_import(path),
        }
    }

    fn start_excel_export(&mut self, path: PathBuf) {
        let game = self.game;
        let Some(account) = self.account().filter(|a| !a.is_empty()).cloned() else {
            let text = self.i18n.t(game, "log.export.empty");
            self.toast(text, true);
            return;
        };
        let with_raw_sheet = game == Game::StarRail;
        self.job = Some(jobs::spawn_excel(
            account,
            self.i18n.clone(),
            path,
            with_raw_sheet,
        ));
        self.status = Status::Loading;
        self.status_text = self.i18n.t(game, "ui.hint.loading");
    }

    fn start_uigf_export(&mut self, path: PathBuf) {
        let game = self.game;
        let version = crate::uigf::UigfVersion::parse(&self.config.uigf_version);
        let accounts: Vec<Account> = if self.config.uigf_all_accounts {
            self.visible_accounts()
                .into_iter()
                .filter(|account| !account.is_empty())
                .cloned()
                .collect()
        } else {
            self.account()
                .filter(|account| !account.is_empty())
                .cloned()
                .into_iter()
                .collect()
        };
        if accounts.is_empty() {
            let text = self.i18n.t(game, "log.export.empty");
            self.toast(text, true);
            return;
        }
        let dict = crate::uigf::ItemDict::load(&self.store.dir);
        let readable = self.config.readable_json;
        self.job = Some(jobs::spawn_uigf_export(
            version,
            accounts,
            dict,
            path,
            self.store.dir.clone(),
            readable,
        ));
        self.status = Status::Loading;
        self.status_text = self.i18n.t(game, "ui.hint.loading");
    }

    fn start_import(&mut self, path: PathBuf) {
        let game = self.game;
        self.job = Some(jobs::spawn_import(
            path,
            self.data.clone(),
            self.i18n.lang().to_string(),
        ));
        self.status = Status::Loading;
        self.status_text = self.i18n.t(game, "ui.hint.loading");
    }

    pub fn action_check_update(&mut self) {
        let game = self.game;
        if self.job.is_some() {
            return;
        }
        if self.config.update_repo.trim().is_empty() {
            let text = self.i18n.t(game, "log.update.failed");
            self.toast(text, true);
            return;
        }
        let repo = self.config.update_repo.clone();
        self.job = Some(jobs::spawn_update_check(repo));
        let text = self.i18n.t(game, "log.update.checking");
        self.toast(text, false);
    }

    pub fn action_open_download_page(&mut self) {
        if let Some(release) = &self.update {
            let url = release.url.clone();
            self.open_target(&url);
        } else if !self.config.update_repo.trim().is_empty() {
            let url = format!(
                "https://github.com/{}/releases",
                self.config.update_repo.trim()
            );
            self.open_target(&url);
        }
    }

    // ---------------------------------------------------------------- outcomes

    fn format_status(&self, key: &str, args: &[(String, String)]) -> String {
        let mut text = self.i18n.t_or_raw(self.game, key);
        for (name, value) in args {
            text = text.replace(&format!("${{{name}}}"), value);
        }
        text
    }

    fn poll_job(&mut self, ctx: &Context) {
        let Some(job) = self.job.as_mut() else { return };
        let messages = job.drain();
        let mut finished = job.done;
        let mut pending: Vec<Outcome> = Vec::new();
        let mut statuses: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for message in messages {
            match message {
                Msg::Status(status) => statuses.push((status.key, status.args)),
                Msg::Done(outcome) => {
                    finished = true;
                    pending.push(*outcome);
                }
            }
        }
        for (key, args) in statuses {
            self.status_text = self.format_status(&key, &args);
        }
        for outcome in pending {
            self.handle_outcome(outcome);
        }
        if finished {
            self.job = None;
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    fn handle_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Fetched {
                game,
                url,
                cache_folder,
                result,
            } => {
                if let Some(folder) = cache_folder {
                    self.config.cache_folder = Some(folder.display().to_string());
                    self.save_config();
                }
                match result {
                    Ok(fetched) => {
                        if !url.is_empty() {
                            let masked = crate::urltools::mask_authkey(&url);
                            self.store.log("INFO", &format!("{} {masked}", game.id()));
                        }
                        self.apply_fetched(game, fetched)
                    }
                    Err(failure) if failure.text == "log.fetch.cancelled" => {
                        // User pressed cancel: not an error, just stop.
                        self.status = if self.account().is_some_and(|a| !a.is_empty()) {
                            Status::Loaded
                        } else {
                            Status::Idle
                        };
                        self.status_text = self.i18n.t(game, "log.fetch.cancelled");
                    }
                    Err(failure) => {
                        self.status = Status::Failed;
                        self.auth_timeout = failure.auth_timeout;
                        self.status_text = self.i18n.t_or_raw(game, &failure.text);
                        let text = format!(
                            "{} - {}",
                            self.i18n.t(game, "ui.hint.failed"),
                            self.status_text
                        );
                        self.log_error(&text);
                        self.toast(text, true);
                    }
                }
            }
            Outcome::ExcelExported { path } => {
                let game = self.game;
                let text = format!("{}: {}", self.i18n.t(game, "log.export.ok"), path.display());
                self.toast(text, false);
                self.status_text = self.i18n.t(game, "log.export.ok");
                self.status = Status::Loaded;
            }
            Outcome::UigfExported { path, accounts } => {
                let game = self.game;
                let text = format!(
                    "{} ({} UIGF): {}",
                    self.i18n.t(game, "log.export.ok"),
                    accounts,
                    path.display()
                );
                self.toast(text, false);
                self.status_text = self.i18n.t(game, "log.export.ok");
                self.status = Status::Loaded;
            }
            Outcome::Imported { path, accounts } => {
                let count = accounts.len();
                let mut error: Option<String> = None;
                for account in accounts {
                    let game = account.game;
                    let uid = account.uid.clone();
                    let _ = self.store.backup(&uid);
                    let existing = self.store.get(game, &uid).cloned();
                    let merged = merge_imported(existing.as_ref(), &account);
                    if let Err(err) = self.store.upsert(merged, self.config.readable_json) {
                        error = Some(err.to_string());
                    }
                }
                self.refresh_stats();
                if let Some(err) = error {
                    self.toast(err, true);
                    self.status = Status::Failed;
                } else {
                    let text = format!(
                        "{}: {} ({})",
                        self.i18n.t_any("log.import.ok"),
                        path.display(),
                        count
                    );
                    self.toast(text, false);
                    self.status_text = self.i18n.t_any("log.import.ok");
                    self.status = Status::Loaded;
                    if self.current_uid().is_none() {
                        let first = self
                            .visible_accounts()
                            .first()
                            .map(|account| account.uid.clone());
                        if let Some(uid) = first {
                            self.action_change_uid(&uid);
                        }
                    }
                }
            }
            Outcome::UpdateChecked(result) => {
                let game = self.game;
                match result {
                    Ok(Some(release)) => {
                        let text = self.i18n.t_fmt(
                            game,
                            "log.update.available",
                            &[("version", release.version.clone())],
                        );
                        self.update = Some(release);
                        self.toast(text, false);
                    }
                    Ok(None) => {
                        let text = self.i18n.t(game, "log.update.latest");
                        self.toast(text, false);
                    }
                    Err(err) => {
                        let text = self.i18n.t_or_raw(game, &err);
                        self.toast(text, true);
                    }
                }
            }
            Outcome::Failed(text) => {
                self.status = Status::Failed;
                self.status_text = self.i18n.t_or_raw(self.game, &text);
                let detail = self.status_text.clone();
                self.log_error(&detail);
                let title = self.i18n.t(self.game, "ui.hint.failed");
                self.dialog = Dialog::Message {
                    title,
                    text: detail,
                };
            }
            Outcome::ExportFailed {
                purpose,
                path,
                error,
            } => {
                let mut error = error;
                match crate::permissions::process_permissions() {
                    Ok(Some(permissions)) => {
                        self.store
                            .log("INFO", &format!("export process: {permissions}"));
                        if permissions.restricted || permissions.app_container {
                            error.push_str("\n\n");
                            error.push_str(&self.i18n.t_any("log.export.restrictedProcess"));
                        }
                    }
                    Ok(None) => {}
                    Err(diagnostic_error) => self.store.log(
                        "INFO",
                        &format!("export process diagnostics: {diagnostic_error}"),
                    ),
                }
                self.status = Status::Failed;
                self.status_text = self.i18n.t(self.game, "log.export.failed");
                self.log_error(&format!("{}: {error}", path.display()));
                self.dialog = Dialog::ExportError {
                    purpose,
                    path,
                    text: error,
                };
            }
        }
    }

    /// Mirrors upstream behaviour: failures are appended to `userData/log.txt`.
    fn log_error(&mut self, text: &str) {
        let masked = mask_authkey(text);
        self.store.log("ERROR", &masked);
    }

    fn apply_fetched(&mut self, game: Game, fetched: Fetched) {
        if fetched.uid.is_empty() {
            self.status = Status::Failed;
            let text = self.i18n.t(game, "log.url.incorrect");
            self.status_text = text.clone();
            self.toast(text, true);
            return;
        }
        let uid = fetched.uid.clone();
        let banners = self.data.banners(game, &fetched.lang);
        let existing = self.store.get(game, &uid).cloned();
        let mut account = existing.clone().unwrap_or_else(|| Account::new(&uid, game));
        account.game = game;
        account.uid = uid.clone();
        account.lang = fetched.lang.clone();
        if !fetched.region.is_empty() {
            account.region = fetched.region.clone();
        }
        if fetched.region_time_zone != 0 {
            account.region_time_zone = fetched.region_time_zone;
        }
        account.time = chrono::Utc::now().timestamp_millis();
        account.deleted = false;
        account.buckets = merge_fetched(existing.as_ref(), &fetched, &banners);
        let total = account.total_records();

        let readable = self.config.readable_json;
        if let Err(err) = self.store.upsert(account, readable) {
            self.toast(err.to_string(), true);
            self.status = Status::Failed;
            return;
        }
        // The captured URL is deliberately not persisted: it embeds a short
        // lived credential. Only the account and its records are written.
        self.config.set_current_uid(game, &uid);
        self.save_config();
        self.refresh_stats();
        self.status = Status::Loaded;
        let text = format!(
            "{}: {} ({} {})",
            self.i18n.t(game, "log.data.saved"),
            uid,
            total,
            self.i18n.t(game, "ui.data.times")
        );
        self.status_text = text.clone();
        self.toast(text, false);
    }

    // ---------------------------------------------------------------- eframe

    /// Enables `--screenshot` mode: one rendered frame is written to `path`.
    pub fn capture_to(&mut self, path: PathBuf, view: Option<&str>) {
        self.screenshot_path = Some(path);
        match view {
            Some("detail") => {
                self.view = View::Detail;
                self.show_settings = false;
            }
            Some("settings") => {
                self.show_settings = true;
            }
            _ => {
                self.view = View::Overview;
                self.show_settings = false;
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &Context) {
        if self.screenshot_path.is_none() || self.screenshot_requested {
            return;
        }
        self.screenshot_frames += 1;
        if self.screenshot_frames < 8 {
            ctx.request_repaint();
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        self.screenshot_requested = true;
        ctx.request_repaint();
    }

    fn save_screenshot(&mut self, ctx: &Context) {
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            ctx.request_repaint();
            return;
        };
        match ui::save_png(&path, &image) {
            Ok(()) => println!("screenshot saved to {}", path.display()),
            Err(err) => eprintln!("screenshot failed: {err}"),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                let game = self.game;
                let busy = self.job.is_some();
                ui.horizontal(|ui| {
                    // Game switcher.
                    for candidate in Game::ALL {
                        let label = self.i18n.t(candidate, candidate.name_key());
                        if ui
                            .selectable_label(
                                self.game == candidate,
                                RichText::new(label).size(13.0),
                            )
                            .clicked()
                        {
                            self.set_game(candidate);
                        }
                    }
                    ui.separator();

                    let has_account = self.account().is_some_and(|account| !account.is_empty());
                    let label = if has_account {
                        self.i18n.t(game, "ui.button.update")
                    } else {
                        self.i18n.t(game, "ui.button.load")
                    };
                    if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                        self.start_fetch_auto();
                    }

                    let files_label = self.i18n.t(game, "ui.button.files");
                    let excel_label = self.i18n.t(game, "ui.button.excel");
                    let uigf_label = self.i18n.t(game, "ui.button.uigf");
                    let import_label = self.i18n.t(game, "ui.button.import");
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.menu_button(files_label, |ui| {
                            if ui.button(excel_label).clicked() {
                                self.action_export_excel();
                                ui.close();
                            }
                            if ui.button(uigf_label).clicked() {
                                self.action_export_uigf();
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(import_label).clicked() {
                                self.action_import_json();
                                ui.close();
                            }
                        });
                    });

                    let new_account = self.i18n.t(game, "ui.hint.newAccount");
                    if ui.button("+").on_hover_text(new_account).clicked() {
                        self.action_new_account();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let option_label = self.i18n.t(game, "ui.button.option");
                        let setting_label = self.i18n.t(game, "ui.button.setting");
                        let url_label = self.i18n.t(game, "ui.button.url");
                        let copy_label = self.i18n.t(game, "ui.button.copyUrl");
                        let proxy_label = self.i18n.t(game, "ui.button.startProxy");
                        let update_label = self.i18n.t(game, "ui.button.checkUpdate");
                        ui.menu_button(option_label, |ui| {
                            if ui.button(setting_label).clicked() {
                                self.show_settings = true;
                                ui.close();
                            }
                            if ui.button(url_label).clicked() {
                                self.dialog = Dialog::ManualUrl(String::new());
                                ui.close();
                            }
                            if ui.button(copy_label).clicked() {
                                self.action_copy_url();
                                ui.close();
                            }
                            if ui.button(proxy_label).clicked() {
                                self.start_fetch_proxy();
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(update_label).clicked() {
                                self.action_check_update();
                                ui.close();
                            }
                        });

                        // Account selector.
                        let accounts: Vec<(String, String)> = self
                            .visible_accounts()
                            .into_iter()
                            .map(|account| {
                                (
                                    account.uid.clone(),
                                    format!("{}  ({})", account.uid, account.total_records()),
                                )
                            })
                            .collect();
                        let current = self.current_uid().unwrap_or_default();
                        let new_account_text = self.i18n.t(game, "ui.select.newAccount");
                        let selected_text = if current.is_empty() {
                            new_account_text.clone()
                        } else {
                            accounts
                                .iter()
                                .find(|(uid, _)| uid == &current)
                                .map(|(_, label)| label.clone())
                                .unwrap_or_else(|| current.clone())
                        };
                        let mut chosen = current.clone();
                        egui::ComboBox::from_id_salt("account-select")
                            .selected_text(selected_text)
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut chosen, String::new(), new_account_text);
                                for (uid, label) in &accounts {
                                    ui.selectable_value(&mut chosen, uid.clone(), label);
                                }
                            });
                        if chosen != current {
                            self.action_change_uid(&chosen);
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let game = self.game;
        egui::Panel::top("status")
            .frame(
                egui::Frame::new()
                    .fill(ui::PANEL_BG)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (view, key) in [
                        (View::Overview, "ui.tab.overview"),
                        (View::Detail, "ui.tab.detail"),
                    ] {
                        let label = self.i18n.t(game, key);
                        if ui.selectable_label(self.view == view, label).clicked() {
                            self.view = view;
                        }
                    }
                    ui.separator();

                    if self.job.is_some() {
                        ui.spinner();
                        let cancel_label = self.i18n.t(game, "ui.common.cancel");
                        if ui.small_button(cancel_label).clicked() {
                            if let Some(job) = &self.job {
                                job.cancel();
                            }
                        }
                    }
                    let text = if self.status_text.is_empty() {
                        match self.status {
                            Status::Idle => self.i18n.t(game, "ui.hint.init"),
                            Status::Loaded => self
                                .account()
                                .map(|account| {
                                    format!(
                                        "{}: {}",
                                        self.i18n.t(game, "ui.hint.lastUpdate"),
                                        chrono::DateTime::from_timestamp_millis(account.time)
                                            .map(|dt| dt
                                                .with_timezone(&chrono::Local)
                                                .format("%Y-%m-%d %H:%M:%S")
                                                .to_string())
                                            .unwrap_or_default()
                                    )
                                })
                                .unwrap_or_default(),
                            _ => String::new(),
                        }
                    } else {
                        self.status_text.clone()
                    };
                    let color = match self.status {
                        Status::Failed => ui::DANGER,
                        Status::Loading => ui::ACCENT,
                        _ => ui::TEXT_MUTED,
                    };
                    ui.label(RichText::new(text).size(11.0).color(color));
                    if self.auth_timeout {
                        let label = self.i18n.t(game, "ui.button.solution");
                        if ui.small_button(label).clicked() {
                            self.dialog = Dialog::CacheHint;
                        }
                    }
                });
            });
    }

    fn draw_toast(&mut self, ctx: &Context) {
        let Some((text, error, until)) = self
            .toast
            .as_ref()
            .map(|toast| (toast.text.clone(), toast.error, toast.until))
        else {
            return;
        };
        if Instant::now() > until {
            self.toast = None;
            return;
        }
        let fill = if error {
            egui::Color32::from_rgb(0xFD, 0xEC, 0xEC)
        } else {
            egui::Color32::from_rgb(0xEC, 0xF7, 0xF1)
        };
        let color = if error { ui::DANGER } else { ui::SUCCESS };
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        ui.label(RichText::new(text).size(12.0).color(color));
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn remember_window_size(&mut self, ctx: &Context) {
        if self.screenshot_path.is_some() {
            return;
        }
        if self.last_window_save.elapsed() < Duration::from_secs(3) {
            return;
        }
        self.last_window_save = Instant::now();
        let size = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
        if let Some(size) = size {
            let stored = self.config.window.unwrap_or([0.0, 0.0]);
            if (stored[0] - size.x).abs() > 1.0 || (stored[1] - size.y).abs() > 1.0 {
                self.config.window = Some([size.x, size.y]);
                self.save_config();
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.fonts_installed {
            ui::install_fonts(&ctx);
            ui::apply_style(&ctx);
            self.fonts_installed = true;
        }
        if let Some(text) = self.pending_clipboard.take() {
            ctx.copy_text(text);
        }
        if !self.startup_update_checked {
            self.startup_update_checked = true;
            if self.config.auto_check_update && !self.config.update_repo.trim().is_empty() {
                let repo = self.config.update_repo.clone();
                self.job = Some(jobs::spawn_update_check(repo));
            }
        }

        self.poll_job(&ctx);
        self.poll_open_targets(&ctx);
        self.toolbar(ui);
        self.status_bar(ui);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ui::PANEL_BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                if self.show_settings {
                    ui::settings::settings(self, ui);
                } else {
                    match self.view {
                        View::Overview => {
                            let stats = self.stats_snapshot();
                            ui::overview::overview(self, ui, &stats);
                        }
                        View::Detail => ui::detail::detail(self, ui),
                    }
                }
            });
        if matches!(self.dialog, Dialog::File(_)) {
            if let Dialog::File(request) = std::mem::replace(&mut self.dialog, Dialog::None) {
                let purpose = request.purpose;
                let save = request.is_save();
                let extension = request.extension;
                match crate::file_dialog::show(request, frame) {
                    Ok(Some(path)) => {
                        let path = if save && cfg!(windows) {
                            crate::file_dialog::selected_path(path, extension)
                        } else {
                            path
                        };
                        self.file_selected(purpose, path);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.log_error(&error);
                        self.dialog = Dialog::Message {
                            title: self.i18n.t(self.game, "ui.hint.failed"),
                            text: error,
                        };
                    }
                }
                ctx.request_repaint();
            }
        }
        ui::dialogs::dialogs(self, &ctx);
        self.draw_toast(&ctx);
        self.remember_window_size(&ctx);
        self.handle_screenshot(&ctx);
        self.save_screenshot(&ctx);
    }
}

/// Replaces the authkey inside a URL before it reaches the log file.
fn mask_authkey(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find("authkey=") {
        let start = position + "authkey=".len();
        result.push_str(&rest[..start]);
        result.push_str("***");
        match rest[start..].find('&') {
            Some(end) => {
                rest = &rest[start + end..];
            }
            None => {
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result
}

/// Merges freshly fetched buckets with the local ones, keeping order and names.
fn merge_fetched(
    existing: Option<&Account>,
    fetched: &Fetched,
    banners: &[(String, String)],
) -> Vec<Bucket> {
    let mut keys: Vec<String> = fetched.buckets.iter().map(|(key, _)| key.clone()).collect();
    if let Some(existing) = existing {
        for bucket in &existing.buckets {
            if !keys.contains(&bucket.key) && !bucket.records.is_empty() {
                keys.push(bucket.key.clone());
            }
        }
    }
    let mut buckets = Vec::new();
    for key in keys {
        let incoming = fetched
            .buckets
            .iter()
            .find(|(banner_key, _)| banner_key == &key)
            .map(|(_, records)| records.clone())
            .unwrap_or_default();
        let local = existing
            .and_then(|account| account.bucket(&key))
            .map(|bucket| bucket.records.clone())
            .unwrap_or_default();
        let name = banners
            .iter()
            .find(|(banner_key, _)| banner_key == &key)
            .map(|(_, name)| name.clone())
            .or_else(|| {
                existing
                    .and_then(|account| account.bucket(&key))
                    .map(|bucket| bucket.name.clone())
            })
            .unwrap_or_default();
        let records = merge_records(&local, &incoming);
        buckets.push(Bucket { key, name, records });
    }
    buckets
}

/// Merges an imported account into an existing one (never losing records).
fn merge_imported(existing: Option<&Account>, imported: &Account) -> Account {
    let mut account = existing
        .cloned()
        .unwrap_or_else(|| Account::new(&imported.uid, imported.game));
    account.game = imported.game;
    account.uid = imported.uid.clone();
    if !imported.lang.is_empty() {
        account.lang = imported.lang.clone();
    }
    if !imported.region.is_empty() {
        account.region = imported.region.clone();
    }
    if imported.region_time_zone != 0 {
        account.region_time_zone = imported.region_time_zone;
    }
    account.time = chrono::Utc::now().timestamp_millis();
    account.deleted = false;
    let mut buckets: Vec<Bucket> = account.buckets.clone();
    for imported_bucket in &imported.buckets {
        match buckets
            .iter_mut()
            .find(|bucket| bucket.key == imported_bucket.key)
        {
            Some(bucket) => {
                if bucket.name.is_empty() {
                    bucket.name = imported_bucket.name.clone();
                }
                bucket.records = merge_records(&bucket.records, &imported_bucket.records);
            }
            None => buckets.push(imported_bucket.clone()),
        }
    }
    account.buckets = buckets;
    account
}
