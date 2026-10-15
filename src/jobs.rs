//! Background workers. Long running work (network, Excel writing) runs on its
//! own thread and reports progress through a channel.

use crate::fetch::{FetchFailure, FetchOptions, Fetched, Status};
use crate::gamedata::GameData;
use crate::i18n::I18n;
use crate::model::{Account, Game};
use crate::uigf::{self, ItemDict, UigfVersion};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub enum Outcome {
    Fetched {
        game: Game,
        url: String,
        /// Web cache folder the URL came from, remembered for the UI button.
        cache_folder: Option<PathBuf>,
        result: Result<Fetched, FetchFailure>,
    },
    ExcelExported {
        path: PathBuf,
    },
    UigfExported {
        path: PathBuf,
        accounts: usize,
    },
    Imported {
        path: PathBuf,
        accounts: Vec<Account>,
    },
    UpdateChecked(Result<Option<crate::update::Release>, String>),
    Failed(String),
    ExportFailed {
        purpose: crate::file_dialog::Purpose,
        path: PathBuf,
        error: String,
    },
}

pub enum Msg {
    Status(Status),
    Done(Box<Outcome>),
}

pub struct Job {
    pub rx: Receiver<Msg>,
    pub cancel: Arc<AtomicBool>,
    pub done: bool,
}

impl Job {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Non blocking poll; returns messages received so far.
    pub fn drain(&mut self) -> Vec<Msg> {
        let mut messages = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    if matches!(msg, Msg::Done(_)) {
                        self.done = true;
                    }
                    messages.push(msg);
                }
                Err(_) => break,
            }
        }
        messages
    }
}

fn spawn<F>(label: &'static str, work: F) -> Job
where
    F: FnOnce(Sender<Msg>, Arc<AtomicBool>) + Send + 'static,
{
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    std::thread::Builder::new()
        .name(format!("aurora-{label}"))
        .spawn(move || work(tx, worker_cancel))
        .expect("failed to spawn worker thread");
    Job {
        rx,
        cancel,
        done: false,
    }
}

/// Where the authkey URL comes from.
pub enum UrlSource {
    /// Read the game log / web cache.
    Auto,
    /// A URL pasted by the user.
    Manual(String),
    /// Start the MITM proxy and wait for the game to send the URL.
    Proxy,
}

pub struct FetchRequest {
    pub game: Game,
    pub source: UrlSource,
    pub options: FetchOptions,
    pub data: Arc<GameData>,
    /// Preferred server for the log lookup (`logType` setting).
    pub log_type: i32,
    /// Port used when the proxy mode has to be started.
    pub proxy_port: u16,
}

pub fn spawn_fetch(request: FetchRequest) -> Job {
    spawn("fetch", move |tx, cancel| {
        let status_tx = tx.clone();
        let mut status = move |status: Status| {
            let _ = status_tx.send(Msg::Status(status));
        };
        let mut cache_folder = None;
        let captured = match &request.source {
            UrlSource::Manual(url) => Ok(url.clone()),
            UrlSource::Auto => {
                status(Status::key("log.log.reading"));
                crate::urltools::find_url(request.game, request.log_type)
                    .map(|found| {
                        cache_folder = found.cache_folder;
                        status(Status::key(found.source));
                        found.url
                    })
                    .map_err(|key| FetchFailure {
                        text: key,
                        auth_timeout: false,
                    })
            }
            UrlSource::Proxy => {
                crate::proxy::capture_url(request.game, request.proxy_port, &cancel, &mut status)
                    .map_err(|text| FetchFailure {
                        text,
                        auth_timeout: false,
                    })
            }
        };

        let outcome = match captured {
            Ok(url) => {
                let result = crate::fetch::fetch_account(
                    request.game,
                    &url,
                    &request.data,
                    &request.options,
                    &cancel,
                    &mut status,
                );
                // Prefer the normalized URL (authkey re-encoded) for the config.
                let stored_url = match &result {
                    Ok(fetched) if !fetched.url.is_empty() => fetched.url.clone(),
                    _ => url,
                };
                Outcome::Fetched {
                    game: request.game,
                    url: stored_url,
                    cache_folder,
                    result,
                }
            }
            Err(err) => Outcome::Fetched {
                game: request.game,
                url: String::new(),
                cache_folder,
                result: Err(err),
            },
        };
        let _ = tx.send(Msg::Done(Box::new(outcome)));
    })
}

pub fn spawn_excel(account: Account, i18n: Arc<I18n>, path: PathBuf, with_raw_sheet: bool) -> Job {
    spawn("excel", move |tx, _cancel| {
        let outcome = match crate::excel::export(&account, &i18n, &path, with_raw_sheet) {
            Ok(()) => Outcome::ExcelExported { path },
            Err(err) => Outcome::ExportFailed {
                purpose: crate::file_dialog::Purpose::ExportExcel,
                path,
                error: err,
            },
        };
        let _ = tx.send(Msg::Done(Box::new(outcome)));
    })
}

pub fn spawn_uigf_export(
    version: UigfVersion,
    accounts: Vec<Account>,
    mut dict: ItemDict,
    path: PathBuf,
    data_dir: PathBuf,
    readable: bool,
) -> Job {
    spawn("uigf-export", move |tx, _cancel| {
        let agent = crate::net::agent(30);
        let refs: Vec<&Account> = accounts.iter().collect();
        let outcome = match uigf::build(version, &refs, &mut dict, &agent) {
            Ok(value) => {
                dict.save(&data_dir);
                let text = if readable {
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                } else {
                    serde_json::to_string(&value).unwrap_or_default()
                };
                match std::fs::write(&path, text.as_bytes()) {
                    Ok(()) => Outcome::UigfExported {
                        path,
                        accounts: accounts.len(),
                    },
                    Err(err) => Outcome::ExportFailed {
                        purpose: crate::file_dialog::Purpose::ExportUigf,
                        path,
                        error: err.to_string(),
                    },
                }
            }
            Err(err) => Outcome::Failed(err),
        };
        let _ = tx.send(Msg::Done(Box::new(outcome)));
    })
}

pub fn spawn_import(path: PathBuf, data: Arc<GameData>, ui_lang: String) -> Job {
    spawn("import", move |tx, _cancel| {
        let outcome = match std::fs::read_to_string(&path) {
            Ok(text) => match uigf::parse_import_text(&text, &data, &ui_lang) {
                Ok(accounts) => Outcome::Imported { path, accounts },
                Err(err) => Outcome::Failed(err),
            },
            Err(err) => Outcome::Failed(err.to_string()),
        };
        let _ = tx.send(Msg::Done(Box::new(outcome)));
    })
}

pub fn spawn_update_check(repo: String) -> Job {
    spawn("update", move |tx, _cancel| {
        let agent = crate::net::agent(20);
        let result = crate::update::check(&agent, &repo);
        let _ = tx.send(Msg::Done(Box::new(Outcome::UpdateChecked(result))));
    })
}
