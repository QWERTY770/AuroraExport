//! Settings page.

use crate::app::App;
use crate::model::Game;
use crate::ui;
use egui::{Grid, RichText, ScrollArea, Ui};

fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(18.0);
    ui.label(RichText::new(text).size(15.0).strong().color(ui::ACCENT));
    ui.add_space(2.0);
    ui.separator();
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(ui::TEXT_MUTED));
}

pub fn settings(app: &mut App, ui: &mut Ui) {
    let game = app.game;
    let i18n = app.i18n.clone();
    let mut changed = false;
    let mut new_lang: Option<String> = None;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            crate::ui::settings_layout::centered_settings_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(i18n.t(game, "ui.setting.title"))
                            .size(18.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(i18n.t(game, "ui.button.close")).clicked() {
                            app.show_settings = false;
                        }
                    });
                });

                // ---------------- fetching ----------------
                section_title(ui, &i18n.t(game, "ui.setting.fetchSection"));
                Grid::new("settings-fetch")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(i18n.t(game, "ui.setting.logType")).size(12.0));
                        ui.vertical(|ui| {
                            let mut log_type = app.config.log_type(game);
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .radio_value(&mut log_type, 0, i18n.t(game, "ui.setting.auto"))
                                    .changed();
                                changed |= ui
                                    .radio_value(
                                        &mut log_type,
                                        1,
                                        i18n.t(game, "ui.setting.cnServer"),
                                    )
                                    .changed();
                                changed |= ui
                                    .radio_value(
                                        &mut log_type,
                                        2,
                                        i18n.t(game, "ui.setting.seaServer"),
                                    )
                                    .changed();
                                if game == Game::Genshin {
                                    changed |= ui
                                        .radio_value(&mut log_type, 3, "云原神".to_string())
                                        .changed();
                                }
                            });
                            hint(ui, &i18n.t(game, "ui.setting.logTypeHint"));
                            if log_type != app.config.log_type(game) {
                                app.config.set_log_type(game, log_type);
                                changed = true;
                            }
                        });
                        ui.end_row();

                        ui.label(
                            RichText::new(i18n.t(game, "ui.setting.fetchFullHistory")).size(12.0),
                        );
                        ui.vertical(|ui| {
                            let mut value = app.config.fetch_full_history;
                            if ui.checkbox(&mut value, "").changed() {
                                app.config.fetch_full_history = value;
                                changed = true;
                            }
                            hint(ui, &i18n.t(game, "ui.setting.fetchFullHistoryHint"));
                        });
                        ui.end_row();

                        ui.label(RichText::new(i18n.t(game, "ui.setting.hideNovice")).size(12.0));
                        let mut hide_novice = app.config.hide_novice(game);
                        if crate::ui::settings_layout::left_aligned_checkbox(ui, &mut hide_novice)
                            .changed()
                        {
                            app.config.set_hide_novice(game, hide_novice);
                            changed = true;
                        }
                        ui.end_row();

                        ui.label(RichText::new(i18n.t(game, "ui.setting.proxyMode")).size(12.0));
                        ui.vertical(|ui| {
                            let mut value = app.config.proxy_mode;
                            if ui.checkbox(&mut value, "").changed() {
                                app.config.proxy_mode = value;
                                changed = true;
                            }
                            hint(ui, &i18n.t(game, "ui.setting.proxyModeHint"));
                        });
                        ui.end_row();

                        ui.label(RichText::new(i18n.t(game, "ui.setting.closeProxy")).size(12.0));
                        ui.vertical(|ui| {
                            if ui.button(i18n.t(game, "ui.setting.closeProxy")).clicked() {
                                app.action_clear_system_proxy();
                            }
                            hint(ui, &i18n.t(game, "ui.setting.closeProxyHint"));
                        });
                        ui.end_row();
                    });

                // ---------------- display ----------------
                section_title(ui, &i18n.t(game, "ui.setting.displaySection"));
                Grid::new("settings-display")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(i18n.t(game, "ui.setting.pieInclude3")).size(12.0));
                        ui.vertical(|ui| {
                            let mut value = app.config.pie_include_3;
                            if ui.checkbox(&mut value, "").changed() {
                                app.config.pie_include_3 = value;
                                changed = true;
                            }
                            hint(ui, &i18n.t(game, "ui.setting.pieInclude3Hint"));
                        });
                        ui.end_row();
                    });

                // ---------------- data ----------------
                section_title(ui, &i18n.t(game, "ui.setting.dataSection"));
                Grid::new("settings-data")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(i18n.t(game, "ui.setting.UIGFLable")).size(12.0));
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let mut version = app.config.uigf_version.clone();
                                egui::ComboBox::from_id_salt("uigf-version")
                                    .selected_text(format!("UIGF v{version}"))
                                    .width(110.0)
                                    .show_ui(ui, |ui| {
                                        for candidate in app.uigf_versions() {
                                            ui.selectable_value(
                                                &mut version,
                                                candidate.to_string(),
                                                format!("UIGF v{candidate}"),
                                            );
                                        }
                                    });
                                if version != app.config.uigf_version {
                                    app.config.uigf_version = version;
                                    changed = true;
                                }
                                let mut all = app.config.uigf_all_accounts;
                                if ui
                                    .checkbox(&mut all, i18n.t(game, "ui.setting.UIGFAllAccounts"))
                                    .changed()
                                {
                                    app.config.uigf_all_accounts = all;
                                    changed = true;
                                }
                                let mut readable = app.config.readable_json;
                                if ui
                                    .checkbox(
                                        &mut readable,
                                        i18n.t(game, "ui.setting.UIGFReadable"),
                                    )
                                    .changed()
                                {
                                    app.config.readable_json = readable;
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button(i18n.t(game, "ui.setting.UIGFButton")).clicked() {
                                    app.action_export_uigf();
                                }
                                if ui
                                    .button(i18n.t(game, "ui.setting.UIGFImportButton"))
                                    .clicked()
                                {
                                    app.action_import_json();
                                }
                            });
                            hint(ui, &i18n.t(game, "ui.setting.UIGFHint"));
                        });
                        ui.end_row();

                        ui.label(RichText::new(i18n.t(game, "ui.common.dataManage")).size(12.0));
                        ui.vertical(|ui| {
                            if ui.button(i18n.t(game, "ui.common.dataManage")).clicked() {
                                app.dialog = crate::app::Dialog::DataManager;
                            }
                            hint(ui, &i18n.t(game, "ui.setting.dataManagerHint"));
                        });
                        ui.end_row();

                        ui.label(RichText::new(i18n.t(game, "ui.button.openDataDir")).size(12.0));
                        ui.horizontal(|ui| {
                            if ui.button(i18n.t(game, "ui.button.openDataDir")).clicked() {
                                app.action_open_data_dir();
                            }
                            ui.label(
                                RichText::new(app.store.dir.display().to_string())
                                    .size(11.0)
                                    .color(ui::TEXT_MUTED),
                            );
                        });
                        ui.end_row();
                    });

                // ---------------- application ----------------
                section_title(ui, &i18n.t(game, "ui.setting.appSection"));
                Grid::new("settings-app")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(i18n.t(game, "ui.setting.language")).size(12.0));
                        ui.vertical(|ui| {
                            let mut lang = app.config.lang.clone();
                            egui::ComboBox::from_id_salt("language")
                                .selected_text(
                                    crate::i18n::LANGS
                                        .iter()
                                        .find(|(code, _)| *code == lang)
                                        .map(|(_, name)| name.to_string())
                                        .unwrap_or_else(|| lang.clone()),
                                )
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for (code, name) in crate::i18n::LANGS {
                                        if ui
                                            .selectable_value(&mut lang, code.to_string(), name)
                                            .clicked()
                                        {
                                            new_lang = Some(code.to_string());
                                        }
                                    }
                                });
                            hint(ui, &i18n.t(game, "ui.setting.languageHint"));
                        });
                        ui.end_row();

                        ui.label(
                            RichText::new(i18n.t(game, "ui.setting.autoCheckUpdate")).size(12.0),
                        );
                        ui.vertical(|ui| {
                            let mut value = app.config.auto_check_update;
                            if ui.checkbox(&mut value, "").changed() {
                                app.config.auto_check_update = value;
                                changed = true;
                            }
                            ui.horizontal(|ui| {
                                if ui.button(i18n.t(game, "ui.button.checkUpdate")).clicked() {
                                    app.action_check_update();
                                }
                                if app.update.is_some()
                                    && ui.button(i18n.t(game, "ui.button.download")).clicked()
                                {
                                    app.action_open_download_page();
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("GitHub").size(11.0).color(ui::TEXT_MUTED));
                                let mut repo = app.config.update_repo.clone();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut repo)
                                            .desired_width(220.0)
                                            .hint_text("owner/repository"),
                                    )
                                    .changed()
                                {
                                    app.config.update_repo = repo;
                                    changed = true;
                                }
                            });
                            hint(ui, &i18n.t(game, "ui.setting.updateRepoHint"));
                        });
                        ui.end_row();
                    });

                // ---------------- about ----------------
                section_title(ui, &i18n.t(game, "ui.about.title"));
                ui.label(
                    RichText::new(i18n.t(game, "ui.common.appName"))
                        .size(13.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!(
                        "{}: v{}",
                        i18n.t_any("ui.common.version"),
                        env!("CARGO_PKG_VERSION")
                    ))
                    .size(11.0)
                    .color(ui::TEXT_MUTED),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(i18n.t_any("ui.about.desc")).size(11.0));
                ui.label(
                    RichText::new(i18n.t_any("ui.about.thanks"))
                        .size(11.0)
                        .color(ui::TEXT_MUTED),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(i18n.t(game, "ui.about.license")).size(11.0));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .link("genshin-wish-export")
                        .on_hover_text("https://github.com/biuuu/genshin-wish-export")
                        .clicked()
                    {
                        app.open_target("https://github.com/biuuu/genshin-wish-export");
                    }
                    if ui
                        .link("star-rail-warp-export")
                        .on_hover_text("https://github.com/biuuu/star-rail-warp-export")
                        .clicked()
                    {
                        app.open_target("https://github.com/biuuu/star-rail-warp-export");
                    }
                    if ui.link("UIGF").on_hover_text("https://uigf.org/").clicked() {
                        app.open_target("https://uigf.org/");
                    }
                });
                if let Some(release) = &app.update {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(i18n.t_fmt(
                            game,
                            "log.update.available",
                            &[("version", release.version.clone())],
                        ))
                        .size(12.0)
                        .color(ui::SUCCESS),
                    );
                    if !release.notes.is_empty() {
                        let excerpt: String = release.notes.chars().take(400).collect();
                        ui.label(RichText::new(excerpt).size(11.0).color(ui::TEXT_MUTED));
                    }
                }
                ui.add_space(20.0);
            });
        });

    if let Some(lang) = new_lang {
        app.set_language(&lang);
        changed = true;
    }
    if changed {
        app.save_config();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::store::{Config, Store};
    use std::sync::Arc;

    fn collect_visible_text(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        labels: &mut Vec<String>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) if shape.visual_bounding_rect().intersects(clip) => {
                labels.push(text.galley.text().to_string());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_visible_text(shape, clip, labels);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn settings_sections_are_visible_at_supported_widths() {
        let root = std::env::var_os("AURORA_TEST_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join(format!("aurora-settings-test-{}", std::process::id()));
        let mut app = App::new(
            Store::open(directory.clone()),
            Config::default(),
            Arc::new(GameData::load()),
        );
        for lang in ["zh-cn", "en-us"] {
            app.set_language(lang);
            for width in [840.0, 1024.0, 1600.0] {
                let ctx = egui::Context::default();
                crate::ui::apply_style(&ctx);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 680.0));
                let mut labels = Vec::new();
                for _ in 0..3 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| {
                            settings(&mut app, ui);
                        },
                    );
                    labels.clear();
                    for clipped in &output.shapes {
                        collect_visible_text(
                            &clipped.shape,
                            clipped.clip_rect.intersect(screen),
                            &mut labels,
                        );
                    }
                }
                for key in [
                    "ui.setting.title",
                    "ui.setting.fetchSection",
                    "ui.setting.logType",
                ] {
                    let expected = app.i18n.t(app.game, key);
                    assert!(
                        labels.contains(&expected),
                        "missing {expected:?} at {width}px in {lang}: {labels:?}"
                    );
                }
            }
        }
        std::fs::remove_dir(directory).unwrap();
    }
}
