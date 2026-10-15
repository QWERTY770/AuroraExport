//! AuroraExport - 原神祈愿 / 星穹铁道跃迁记录导出工具
//!
//! A merged rewrite of `genshin-wish-export` and `star-rail-warp-export`
//! without Electron: one small native executable per platform.

mod app;
mod excel;
mod fetch;
mod file_dialog;
mod gamedata;
mod i18n;
mod jobs;
mod model;
mod net;
mod permissions;
mod platform;
mod proxy;
mod selftest;
mod stats;
mod store;
mod ui;
mod uigf;
mod update;
mod urltools;

use crate::app::App;
use crate::gamedata::GameData;
use crate::i18n::I18n;
use crate::model::Game;
use crate::store::{resolve_data_dir, Config, Store};
use std::path::PathBuf;
use std::sync::Arc;

const APP_TITLE: &str = "AuroraExport";

struct Args {
    data_dir: Option<PathBuf>,
    game: Option<Game>,
    fetch_url: Option<String>,
    selftest: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    view: Option<String>,
    size: Option<[f32; 2]>,
    probe_url: bool,
    console_probe: bool,
    permission_probe: Option<PathBuf>,
    help: bool,
    version: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        data_dir: None,
        game: None,
        fetch_url: None,
        selftest: None,
        screenshot: None,
        view: None,
        size: None,
        probe_url: false,
        console_probe: false,
        permission_probe: None,
        help: false,
        version: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--data-dir" => {
                args.data_dir = Some(PathBuf::from(iter.next().ok_or("--data-dir needs a path")?));
            }
            "--game" => {
                let value = iter.next().ok_or("--game needs genshin|starrail")?;
                args.game = Some(Game::from_id(&value).ok_or("unknown game")?);
            }
            "--fetch" => {
                args.fetch_url = Some(iter.next().ok_or("--fetch needs a URL")?);
            }
            "--selftest" => {
                let next = iter.next();
                args.selftest = Some(match next {
                    Some(value) if !value.starts_with("--") => PathBuf::from(value),
                    _ => std::env::temp_dir().join("aurora-selftest"),
                });
            }
            "--screenshot" => {
                args.screenshot = Some(PathBuf::from(
                    iter.next().ok_or("--screenshot needs a path")?,
                ));
            }
            "--view" => {
                let value = iter.next().ok_or("--view needs overview|detail|settings")?;
                args.view = Some(value);
            }
            "--size" => {
                let value = iter.next().ok_or("--size needs WIDTHxHEIGHT")?;
                let (width, height) = value
                    .split_once(['x', 'X'])
                    .ok_or("--size needs WIDTHxHEIGHT")?;
                let width = width.trim().parse::<f32>().map_err(|_| "bad width")?;
                let height = height.trim().parse::<f32>().map_err(|_| "bad height")?;
                args.size = Some([width, height]);
            }
            "--probe-url" => args.probe_url = true,
            "--console-probe" => args.console_probe = true,
            "--permission-probe" => {
                args.permission_probe = Some(PathBuf::from(
                    iter.next().ok_or("--permission-probe needs a directory")?,
                ));
            }
            "--help" | "-h" => args.help = true,
            "--version" | "-V" => args.version = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn print_help() {
    println!("{APP_TITLE} v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  aurora-export                     start the graphical interface");
    println!("  aurora-export --fetch <URL>       fetch records headlessly from a captured URL");
    println!("  aurora-export --fetch auto        ... or read the URL from the game cache first");
    println!("  aurora-export --selftest [DIR]    run the built-in data round trip checks");
    println!();
    println!("Options:");
    println!("  --data-dir <DIR>   use DIR instead of the portable ./userData folder");
    println!("  --game <GAME>      genshin | starrail (defaults to the configured game)");
    println!("  --screenshot <PNG> render one frame to PNG and quit (interface testing)");
    println!("  --view <VIEW>      overview | detail | settings");
    println!("  --size <WxH>       initial window size, e.g. 1200x820");
    println!("  --probe-url        show the captured URL (authkey masked) and how it parses");
    println!("  --console-probe    report console/window handling and exit");
    println!("  --permission-probe <DIR> report process restrictions and test writing to DIR");
    println!("  -h, --help         show this help");
    println!("  -V, --version      show the version");
}

fn main() {
    // Hide the console window that Windows creates when the exe is double
    // clicked; when started from a terminal the console is shared with the
    // shell and stays visible so the command line modes keep working.
    let console_hidden = platform::hide_console_if_standalone();
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    if args.help {
        print_help();
        return;
    }
    if args.version {
        println!("{APP_TITLE} v{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if let Some(directory) = args.permission_probe {
        let ok = permission_probe(&directory);
        std::process::exit(if ok { 0 } else { 1 });
    }

    let data_dir = resolve_data_dir(args.data_dir.as_deref());
    let store = Store::open(data_dir);
    let config = store.load_config();
    let game = args.game.unwrap_or_else(|| config.game());

    if let Some(url) = args.fetch_url {
        run_headless_fetch(store, config, game, &url);
        return;
    }
    if let Some(dir) = args.selftest {
        let ok = selftest::run(&store, &config, &dir);
        std::process::exit(if ok { 0 } else { 1 });
    }
    if args.probe_url {
        probe_url(game, config.log_type(game));
        return;
    }
    if args.console_probe {
        println!(
            "console processes={} hidden={}",
            platform::console_process_count(),
            console_hidden
        );
        return;
    }

    // Only one instance may own the portable data folder at a time.
    if let Ok(instance) = single_instance::SingleInstance::new("aurora-export") {
        if !instance.is_single() {
            eprintln!("AuroraExport is already running.");
            std::process::exit(0);
        }
    }

    let data = Arc::new(GameData::load());
    let window_size = args
        .size
        .unwrap_or_else(|| config.window.unwrap_or([1024.0, 680.0]));
    let viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size([window_size[0].max(760.0), window_size[1].max(520.0)])
        .with_min_inner_size([760.0, 520.0])
        .with_icon(ui::app_icon());
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let mut app_config = config.clone();
    if args.game.is_some() {
        app_config.game = game.id().to_string();
    }
    let screenshot = args.screenshot.clone();
    let view = args.view.clone();
    let result = eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |_cc| {
            let mut app = App::new(store, app_config, data);
            if let Some(path) = screenshot {
                app.capture_to(path, view.as_deref());
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(err) = result {
        eprintln!("failed to start the interface: {err}");
        std::process::exit(1);
    }
}

fn permission_probe(directory: &std::path::Path) -> bool {
    match permissions::process_permissions() {
        Ok(Some(context)) => {
            println!("process: {context}");
            if context.restricted || context.app_container {
                println!("This process has sandbox/token restrictions. The file picker cannot lift them. Launch directly from Windows Explorer or your normal terminal to compare.");
            }
        }
        Ok(None) => println!("Windows token diagnostics are not applicable on this platform."),
        Err(error) => println!("process diagnostics unavailable: {error}"),
    }
    match file_dialog::probe_directory(directory) {
        Ok(()) => {
            println!(
                "write probe OK: {} (temporary file removed)",
                directory.display()
            );
            true
        }
        Err(error) => {
            eprintln!(
                "write probe FAILED: {}: {error} ({:?})",
                directory.display(),
                error.kind()
            );
            false
        }
    }
}

/// `--probe-url`: prints what the cache scanner found, with the authkey masked.
fn probe_url(game: Game, log_type: i32) {
    println!("game: {:?}, log type: {log_type}", game);
    match urltools::find_url(game, log_type) {
        Ok(found) => {
            let masked = urltools::mask_authkey(&found.url);
            println!("cache folder: {:?}", found.cache_folder);
            println!("url length: {}", found.url.len());
            println!("url: {masked}");
            match urltools::parse_capture(&found.url, game) {
                Ok(parsed) => {
                    println!("parse: ok, api domain {}", parsed.api_domain);
                    println!("query: {}", urltools::mask_authkey(&parsed.query));
                }
                Err(err) => println!("parse error: {err}"),
            }
        }
        Err(key) => println!("no URL found: {key}"),
    }
}

fn run_headless_fetch(store: Store, config: Config, game: Game, url: &str) {
    let data = GameData::load();
    let i18n = I18n::new(&config.lang);
    // `--fetch auto` reads the authkey URL from the game log / web cache, which
    // avoids pasting a credential on the command line.
    let captured = if url.eq_ignore_ascii_case("auto") {
        println!("reading the game log / web cache...");
        match urltools::find_url(game, config.log_type(game)) {
            Ok(found) => {
                println!("found the URL in the game cache");
                found.url
            }
            Err(key) => {
                eprintln!("fetch failed: {}", i18n.t_or_raw(game, &key));
                std::process::exit(1);
            }
        }
    } else {
        url.to_string()
    };
    let url = captured.as_str();
    let uid_hint = config.current_uid(game);
    let mut local_latest = std::collections::HashMap::new();
    if let Some(uid) = &uid_hint {
        if let Some(account) = store.get(game, uid) {
            for bucket in &account.buckets {
                if let Some(id) = bucket.records.last().map(|record| record.id.clone()) {
                    if !id.is_empty() {
                        local_latest.insert(bucket.key.clone(), id);
                    }
                }
            }
        }
    }
    let options = fetch::FetchOptions {
        full_history: config.fetch_full_history,
        local_latest,
        uid_hint,
    };
    let mut status = |status: fetch::Status| {
        let mut text = i18n.t_or_raw(game, &status.key);
        for (name, value) in &status.args {
            text = text.replace(&format!("${{{name}}}"), value);
        }
        println!("{text}");
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    match fetch::fetch_account(game, url, &data, &options, &cancel, &mut status) {
        Ok(fetched) => {
            let total: usize = fetched
                .buckets
                .iter()
                .map(|(_, records)| records.len())
                .sum();
            println!(
                "uid={} lang={} region={} tz={} records={}",
                fetched.uid, fetched.lang, fetched.region, fetched.region_time_zone, total
            );
            for (key, records) in &fetched.buckets {
                let name = data.banner_name(game, &fetched.lang, key);
                println!("  {key} {name}: {}", records.len());
            }
            if std::env::var("AURORA_FETCH_SAVE").is_ok() {
                let mut store = store;
                let mut config = config;
                let existing = store.get(game, &fetched.uid).cloned();
                let banners = data.banners(game, &fetched.lang);
                let mut account = existing
                    .clone()
                    .unwrap_or_else(|| model::Account::new(&fetched.uid, game));
                account.lang = fetched.lang.clone();
                account.region = fetched.region.clone();
                account.region_time_zone = fetched.region_time_zone;
                account.time = chrono::Utc::now().timestamp_millis();
                account.buckets = merge_for_headless(existing.as_ref(), &fetched, &banners);
                match store.upsert(account, config.readable_json) {
                    Ok(path) => {
                        println!("saved to {}", path.display());
                        // Remember the account so the next run fetches incrementally.
                        config.set_current_uid(game, &fetched.uid);
                        let _ = store.save_config(&config);
                    }
                    Err(err) => eprintln!("failed to save: {err}"),
                }
            }
        }
        Err(failure) => {
            let text = i18n.t_or_raw(game, &failure.text);
            eprintln!("fetch failed: {text}");
            std::process::exit(1);
        }
    }
}

fn merge_for_headless(
    existing: Option<&model::Account>,
    fetched: &fetch::Fetched,
    banners: &[(String, String)],
) -> Vec<model::Bucket> {
    fetched
        .buckets
        .iter()
        .map(|(key, records)| {
            let local = existing
                .and_then(|account| account.bucket(key))
                .map(|bucket| bucket.records.clone())
                .unwrap_or_default();
            let name = banners
                .iter()
                .find(|(banner_key, _)| banner_key == key)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            model::Bucket {
                key: key.clone(),
                name,
                records: model::merge_records(&local, records),
            }
        })
        .collect()
}
