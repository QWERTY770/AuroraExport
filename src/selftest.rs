//! `--selftest`: exercises the data pipeline without opening a window.
//!
//! It checks the local storage round trip, the statistics, the Excel writer and
//! the UIGF export/import cycle against the accounts found in the data folder.

use crate::gamedata::GameData;
use crate::model::{Account, Game};
use crate::store::{Config, Store};
use std::path::Path;

struct Report {
    checks: Vec<(bool, String)>,
}

impl Report {
    fn new() -> Report {
        Report { checks: Vec::new() }
    }

    fn check(&mut self, ok: bool, label: impl Into<String>) {
        let label = label.into();
        println!("  [{}] {label}", if ok { "ok" } else { "FAIL" });
        self.checks.push((ok, label));
    }

    fn failed(&self) -> usize {
        self.checks.iter().filter(|(ok, _)| !ok).count()
    }
}

pub fn run(store: &Store, config: &Config, out_dir: &Path) -> bool {
    std::fs::create_dir_all(out_dir).ok();
    println!("data directory: {}", store.dir.display());
    println!("output directory: {}", out_dir.display());
    let mut report = Report::new();

    let data = GameData::load();
    println!("item dictionary version: {}", data.item_db_version);

    // Banner configuration of both games.
    for game in Game::ALL {
        let banners = data.banners(game, "zh-cn");
        report.check(
            banners.len() >= 4,
            format!("{game:?}: {} banners in zh-cn", banners.len()),
        );
    }

    // i18n completeness for the keys used by the interface.
    check_i18n(&mut report);

    let accounts: Vec<Account> = store
        .accounts
        .iter()
        .map(|stored| stored.account.clone())
        .collect();
    if accounts.is_empty() {
        println!("no accounts in the data folder - run the tool once to create some");
    }

    for account in &accounts {
        println!(
            "\naccount {} ({:?}, {} records, {} banners)",
            account.uid,
            account.game,
            account.total_records(),
            account.buckets.len()
        );

        // Local storage round trip.
        let json = account.to_json();
        let reloaded = Account::from_json(&json, Some(account.game));
        match &reloaded {
            Some(reloaded) => report.check(
                reloaded.total_records() == account.total_records()
                    && reloaded.uid == account.uid
                    && reloaded.game == account.game,
                format!("local round trip keeps {} records", account.total_records()),
            ),
            None => report.check(false, "local round trip re-parses the account"),
        }

        // Statistics.
        let stats = crate::stats::compute_all(account);
        let total_from_stats: usize = stats.iter().map(|stat| stat.total).sum();
        report.check(
            total_from_stats == account.total_records(),
            format!("statistics cover all {total_from_stats} records"),
        );
        let stars: usize = stats
            .iter()
            .map(|stat| stat.count3 + stat.count4 + stat.count5)
            .sum();
        report.check(
            stars == account.total_records(),
            "every record has a known rarity".to_string(),
        );
        let five_stars: usize = stats.iter().map(|stat| stat.count5).sum();
        let catalogue_matches: bool = account
            .buckets
            .iter()
            .all(|bucket| bucket.records.iter().all(|record| !record.name.is_empty()));
        report.check(catalogue_matches, "every record has a name".to_string());
        println!("     5 star items: {five_stars}");

        // Excel export.
        let excel_path = out_dir.join(format!("selftest-{}-{}.xlsx", account.game.id(), account.uid));
        let i18n = crate::i18n::I18n::new(&config.lang);
        match crate::excel::export(account, &i18n, &excel_path, account.game == Game::StarRail) {
            Ok(()) => {
                let size = std::fs::metadata(&excel_path).map(|m| m.len()).unwrap_or(0);
                report.check(size > 1000, format!("excel written ({} bytes)", size));
            }
            Err(err) => report.check(false, format!("excel export: {err}")),
        }

        // UIGF export + import round trip (no network: item ids may stay empty).
        check_uigf(&mut report, account, &data, out_dir, config);
    }

    let failed = report.failed();
    println!("\n{} checks, {failed} failed", report.checks.len());
    if failed == 0 {
        println!("SELFTEST PASSED");
    } else {
        println!("SELFTEST FAILED");
    }
    failed == 0
}

fn check_i18n(report: &mut Report) {
    let keys = [
        "ui.button.load",
        "ui.button.update",
        "ui.button.excel",
        "ui.button.uigf",
        "ui.button.import",
        "ui.button.setting",
        "ui.button.option",
        "ui.button.url",
        "ui.button.copyUrl",
        "ui.button.startProxy",
        "ui.button.solution",
        "ui.button.cacheFolder",
        "ui.data.total",
        "ui.data.times",
        "ui.data.sum",
        "ui.data.no5star",
        "ui.data.star5",
        "ui.data.star4",
        "ui.data.star3",
        "ui.data.history",
        "ui.data.average",
        "ui.setting.title",
        "ui.setting.language",
        "ui.setting.logType",
        "ui.setting.autoUpdate",
        "ui.setting.hideNovice",
        "ui.setting.fetchFullHistory",
        "ui.setting.proxyMode",
        "ui.setting.closeProxy",
        "ui.urlDialog.title",
        "ui.urlDialog.hint",
        "ui.common.ok",
        "ui.common.cancel",
        "ui.common.dataManage",
        "ui.common.deleted",
        "ui.common.normal",
        "ui.common.delete",
        "ui.common.restore",
        "log.fetch.current",
        "log.fetch.retry",
        "log.fetch.retryFailed",
        "log.fetch.authTimeout",
        "log.url.notFound",
        "log.file.notFound",
        "excel.header.time",
        "excel.header.name",
        "excel.header.type",
        "excel.header.rank",
        "excel.header.total",
        "excel.header.pity",
        "excel.header.remark",
        "excel.customFont",
        "excel.filePrefix",
    ];
    for (code, _) in crate::i18n::LANGS {
        let i18n = crate::i18n::I18n::new(code);
        let mut missing: Vec<&str> = Vec::new();
        for game in Game::ALL {
            for key in keys {
                let text = i18n.t(game, key);
                if text == key {
                    missing.push(key);
                }
            }
        }
        report.check(
            missing.is_empty(),
            format!(
                "i18n {code}: {} keys resolved{}",
                keys.len() * 2,
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(" (missing {:?})", &missing[..missing.len().min(4)])
                }
            ),
        );
    }
}

fn check_uigf(
    report: &mut Report,
    account: &Account,
    data: &GameData,
    out_dir: &Path,
    config: &Config,
) {
    let version = match account.game {
        Game::Genshin => crate::uigf::UigfVersion::V41,
        Game::StarRail => crate::uigf::UigfVersion::V41,
    };
    let agent = crate::net::agent(5);
    let mut dict = crate::uigf::ItemDict::default();
    let refs = [account];
    let value = match crate::uigf::build(version, &refs, &mut dict, &agent) {
        Ok(value) => value,
        Err(err) => {
            report.check(false, format!("UIGF build: {err}"));
            return;
        }
    };
    let key = match account.game {
        Game::Genshin => "hk4e",
        Game::StarRail => "hkrpg",
    };
    let exported: usize = value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|entry| {
                    entry
                        .get("list")
                        .and_then(|v| v.as_array())
                        .map(|list| list.len())
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0);
    report.check(
        exported == account.total_records(),
        format!("UIGF v4.1 exports {exported} records"),
    );

    let path = out_dir.join(format!("selftest-{}-{}.json", account.game.id(), account.uid));
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    if std::fs::write(&path, text.as_bytes()).is_err() {
        report.check(false, "UIGF file written".to_string());
        return;
    }
    report.check(true, "UIGF file written".to_string());

    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match crate::uigf::parse_import_text(&text, data, &config.lang) {
        Ok(accounts) => {
            let imported = accounts.first();
            let same = imported.is_some_and(|imported| {
                imported.uid == account.uid && imported.total_records() == account.total_records()
            });
            report.check(
                same,
                format!(
                    "UIGF import restores {} records",
                    imported.map(|imported| imported.total_records()).unwrap_or(0)
                ),
            );
            // Per banner counts must match as well.
            if let Some(imported) = imported {
                let mut mismatch = Vec::new();
                for bucket in &account.buckets {
                    let expected = bucket.records.len();
                    let actual = imported
                        .bucket(&bucket.key)
                        .map(|bucket| bucket.records.len())
                        .unwrap_or(0);
                    if expected != actual {
                        mismatch.push(format!("{}: {expected} != {actual}", bucket.key));
                    }
                }
                report.check(
                    mismatch.is_empty(),
                    format!("UIGF banner counts match{}", format_mismatch(&mismatch)),
                );
                let names_ok = imported
                    .buckets
                    .iter()
                    .all(|bucket| bucket.records.iter().all(|record| !record.name.is_empty()));
                report.check(names_ok, "imported records keep their names".to_string());
            }
        }
        Err(err) => report.check(false, format!("UIGF import: {err}")),
    }
}

fn format_mismatch(mismatch: &[String]) -> String {
    if mismatch.is_empty() {
        String::new()
    } else {
        format!(" ({})", mismatch.join(", "))
    }
}
