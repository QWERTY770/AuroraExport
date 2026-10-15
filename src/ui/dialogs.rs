//! Modal dialogs: manual URL entry, cache help, data manager and
//! messages.

use crate::app::{App, Dialog};
use crate::ui;
use egui::{Align2, Context, RichText, ScrollArea, Window};

fn modal<'a>(title: &str) -> egui::Window<'a> {
    Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

pub fn dialogs(app: &mut App, ctx: &Context) {
    let game = app.game;
    // Take the dialog out for this frame; arms put back an updated one when needed.
    let dialog = std::mem::replace(&mut app.dialog, Dialog::None);
    match dialog {
        Dialog::None => {}
        Dialog::File(request) => app.dialog = Dialog::File(request),
        Dialog::ExportError {
            purpose,
            path,
            text,
        } => {
            let i18n = app.i18n.clone();
            let mut retry = false;
            let mut close = false;
            modal(&i18n.t(game, "log.export.failed")).show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.label(RichText::new(path.display().to_string()).color(ui::TEXT_STRONG));
                ui.label(RichText::new(&text).color(ui::DANGER));
                ui.add_space(8.0);
                ui.label(i18n.t_any("log.export.locationHint"));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(i18n.t_any("ui.button.copyDetails")).clicked() {
                        ctx.copy_text(format!("{}\n{text}", path.display()));
                    }
                    retry = ui.button(i18n.t_any("ui.button.chooseLocation")).clicked();
                    close = ui.button(i18n.t(game, "ui.common.cancel")).clicked();
                });
            });
            if retry {
                app.retry_export(purpose, path);
                ctx.request_repaint();
            } else if !close {
                app.dialog = Dialog::ExportError {
                    purpose,
                    path,
                    text,
                };
            }
        }
        Dialog::ManualUrl(mut text) => {
            let i18n = app.i18n.clone();
            let mut close = false;
            let mut submit = false;
            modal(&i18n.t(game, "ui.urlDialog.title")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(
                    RichText::new(i18n.t(game, "ui.urlDialog.hint"))
                        .size(12.0)
                        .color(ui::TEXT_MUTED),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text(i18n.t(game, "ui.urlDialog.placeholder")),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(i18n.t(game, "ui.common.ok")).clicked() {
                        submit = true;
                    }
                    if ui.button(i18n.t(game, "ui.common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
            if submit {
                let trimmed = text.trim().to_string();
                if trimmed.is_empty() {
                    app.dialog = Dialog::ManualUrl(text);
                } else {
                    app.dialog = Dialog::None;
                    app.start_fetch_manual(trimmed);
                }
            } else if close {
                app.dialog = Dialog::None;
            } else {
                app.dialog = Dialog::ManualUrl(text);
            }
        }
        Dialog::CacheHint => {
            let i18n = app.i18n.clone();
            let mut close = false;
            modal(&i18n.t(game, "ui.button.solution")).show(ctx, |ui| {
                ui.set_width(520.0);
                if ui.button(i18n.t(game, "ui.button.cacheFolder")).clicked() {
                    app.action_open_cache_folder();
                }
                ui.add_space(6.0);
                ui.label(RichText::new(i18n.t(game, "ui.extra.cacheClean")).size(12.0));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(i18n.t(game, "ui.extra.findCacheFolder"))
                        .size(11.0)
                        .color(ui::TEXT_MUTED),
                );
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui.button(i18n.t(game, "ui.common.ok")).clicked() {
                        close = true;
                    }
                });
            });
            if close {
                app.dialog = Dialog::None;
            } else if matches!(app.dialog, Dialog::None) {
                app.dialog = Dialog::CacheHint;
            }
        }
        Dialog::Message { title, text } => {
            let i18n = app.i18n.clone();
            let mut close = false;
            modal(&title).show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(RichText::new(text.clone()).size(12.0));
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    if ui.button(i18n.t_any("ui.button.copyDetails")).clicked() {
                        ctx.copy_text(text.clone());
                    }
                    if ui.button(i18n.t(game, "ui.common.ok")).clicked() {
                        close = true;
                    }
                });
            });
            if close {
                app.dialog = Dialog::None;
            } else {
                app.dialog = Dialog::Message { title, text };
            }
        }
        Dialog::DataManager => {
            let i18n = app.i18n.clone();
            let mut close = false;
            let mut pending: Option<(String, bool)> = None;
            modal(&i18n.t(game, "ui.common.dataManage")).show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(
                    RichText::new(i18n.t(game, "ui.setting.dataManagerHint"))
                        .size(11.0)
                        .color(ui::TEXT_MUTED),
                );
                ui.add_space(6.0);
                let rows: Vec<(String, i64, bool)> = app
                    .store
                    .accounts
                    .iter()
                    .filter(|stored| stored.account.game == game)
                    .map(|stored| {
                        (
                            stored.account.uid.clone(),
                            stored.account.time,
                            stored.account.deleted,
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    ui.label(
                        RichText::new(i18n.t(game, "ui.hint.noData"))
                            .size(12.0)
                            .color(ui::TEXT_MUTED),
                    );
                }
                ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (uid, time, deleted) in &rows {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(uid).size(12.0));
                                ui.label(
                                    RichText::new(format_time(*time))
                                        .size(11.0)
                                        .color(ui::TEXT_MUTED),
                                );
                                if *deleted {
                                    ui.label(
                                        RichText::new(i18n.t(game, "ui.common.deleted"))
                                            .size(11.0)
                                            .color(ui::DANGER),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new(i18n.t(game, "ui.common.normal"))
                                            .size(11.0)
                                            .color(ui::SUCCESS),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let label = if *deleted {
                                            i18n.t(game, "ui.common.restore")
                                        } else {
                                            i18n.t(game, "ui.common.delete")
                                        };
                                        if ui.button(label).clicked() {
                                            pending = Some((uid.clone(), !*deleted));
                                        }
                                    },
                                );
                            });
                        }
                    });
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui.button(i18n.t(game, "ui.common.ok")).clicked() {
                        close = true;
                    }
                });
            });
            if let Some((uid, deleted)) = pending {
                app.action_toggle_deleted(&uid, deleted);
            }
            if close {
                app.dialog = Dialog::None;
            } else if matches!(app.dialog, Dialog::None) {
                app.dialog = Dialog::DataManager;
            }
        }
    }
}

fn format_time(millis: i64) -> String {
    if millis <= 0 {
        return "-".to_string();
    }
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_dialog::Purpose;
    use crate::gamedata::GameData;
    use crate::store::{Config, Store};
    use std::sync::Arc;

    #[test]
    fn dialogs_remain_visible_until_dismissed() {
        let root = std::env::var_os("AURORA_TEST_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join(format!("aurora-dialog-state-{}", std::process::id()));
        let mut app = App::new(
            Store::open(directory.clone()),
            Config::default(),
            Arc::new(GameData::load()),
        );
        for dialog in [
            Dialog::Message {
                title: "Failure".into(),
                text: "Details".into(),
            },
            Dialog::CacheHint,
            Dialog::DataManager,
            Dialog::ExportError {
                purpose: Purpose::ExportExcel,
                path: "records.xlsx".into(),
                text: "Access denied".into(),
            },
        ] {
            let kind = std::mem::discriminant(&dialog);
            app.dialog = dialog;
            let ctx = Context::default();
            for _ in 0..2 {
                let _ = ctx.run_ui(Default::default(), |ui| dialogs(&mut app, ui.ctx()));
                assert_eq!(std::mem::discriminant(&app.dialog), kind);
            }
        }
        std::fs::remove_dir(directory).unwrap();
    }
}
