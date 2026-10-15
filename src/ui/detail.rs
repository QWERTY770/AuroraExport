//! Records page: a virtualised table of every pull of the selected banner.

use crate::app::App;
use crate::model::Record;
use crate::ui;
use egui::{CornerRadius, RichText, ScrollArea, Ui};

const ROW_HEIGHT: f32 = 22.0;

fn column_widths(available: f32) -> [f32; 6] {
    let ratios = [0.08, 0.24, 0.29, 0.16, 0.10, 0.13];
    let mut widths = [0.0f32; 6];
    for (index, ratio) in ratios.iter().enumerate() {
        widths[index] = available * ratio;
    }
    widths
}

fn header(ui: &mut Ui, widths: [f32; 6], labels: [String; 6]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, label) in labels.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(widths[index], ROW_HEIGHT),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(4),
                egui::Color32::from_rgb(0xE8, 0xEA, 0xEF),
            );
            ui.painter().text(
                rect.left_center() + egui::vec2(6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0),
                ui::TEXT_STRONG,
            );
        }
    });
}

fn cell(ui: &mut Ui, width: f32, text: &str, color: egui::Color32) {
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::hover());
    ui.painter().text(
        rect.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        color,
    );
}

pub fn detail(app: &mut App, ui: &mut Ui) {
    let game = app.game;
    let Some(uid) = app.current_uid() else {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                RichText::new(app.i18n.t(game, "ui.hint.noData"))
                    .size(13.0)
                    .color(ui::TEXT_MUTED),
            );
        });
        return;
    };

    // Banner selector (needs a mutable borrow, so it happens first).
    let options: Vec<(String, String, usize)> = app
        .store
        .get(game, &uid)
        .map(|account| {
            account
                .buckets
                .iter()
                .filter(|bucket| !bucket.records.is_empty())
                .map(|bucket| (bucket.key.clone(), bucket.name.clone(), bucket.records.len()))
                .collect()
        })
        .unwrap_or_default();

    if options.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                RichText::new(app.i18n.t(game, "ui.hint.noData"))
                    .size(13.0)
                    .color(ui::TEXT_MUTED),
            );
        });
        return;
    }

    let current_key = app
        .detail_key
        .clone()
        .filter(|key| options.iter().any(|(option, _, _)| option == key))
        .unwrap_or_else(|| options[0].0.clone());
    app.detail_key = Some(current_key.clone());

    let mut selected = current_key.clone();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("detail-banner")
            .selected_text(
                options
                    .iter()
                    .find(|(key, _, _)| key == &selected)
                    .map(|(_, name, _)| name.clone())
                    .unwrap_or_default(),
            )
            .width(220.0)
            .show_ui(ui, |ui| {
                for (key, name, count) in &options {
                    ui.selectable_value(&mut selected, key.clone(), format!("{name} ({count})"));
                }
            });
        if selected != current_key {
            app.detail_key = Some(selected.clone());
        }
        let total = options
            .iter()
            .find(|(key, _, _)| key == &selected)
            .map(|(_, _, count)| *count)
            .unwrap_or(0);
        ui.label(
            RichText::new(app.i18n.t_fmt(
                game,
                "ui.detail.count",
                &[("n", total.to_string())],
            ))
            .size(12.0)
            .color(ui::TEXT_MUTED),
        );
    });
    ui.add_space(6.0);

    // Immutable view of the selected bucket.
    let Some(account) = app.store.get(game, &uid) else {
        return;
    };
    let Some(bucket) = account.bucket(&selected) else {
        return;
    };
    let records: &[Record] = &bucket.records;
    if records.is_empty() {
        ui.label(
            RichText::new(app.i18n.t(game, "ui.hint.detailEmpty"))
                .size(12.0)
                .color(ui::TEXT_MUTED),
        );
        return;
    }

    // Pity of every pull, computed once per frame (cheap: one linear pass).
    let mut pity = vec![0usize; records.len()];
    let mut counter = 0usize;
    for (index, record) in records.iter().enumerate() {
        counter += 1;
        pity[index] = counter;
        if record.rank == 5 {
            counter = 0;
        }
    }

    let labels = [
        app.i18n.t(game, "excel.header.total"),
        app.i18n.t(game, "excel.header.time"),
        app.i18n.t(game, "excel.header.name"),
        app.i18n.t(game, "excel.header.type"),
        app.i18n.t(game, "excel.header.rank"),
        app.i18n.t(game, "excel.header.pity"),
    ];
    let available = ui.available_width();
    let widths = column_widths(available);
    header(ui, widths, labels);
    ui.add_space(2.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, records.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                // Newest first.
                let index = records.len() - 1 - row;
                let record = &records[index];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if row % 2 == 1 {
                        let rect = ui.available_rect_before_wrap();
                        let strip = egui::Rect::from_min_size(
                            rect.min,
                            egui::vec2(available, ROW_HEIGHT),
                        );
                        ui.painter().rect_filled(
                            strip,
                            CornerRadius::ZERO,
                            egui::Color32::from_rgb(0xF1, 0xF3, 0xF7),
                        );
                    }
                    let color = ui::rank_color(record.rank);
                    cell(ui, widths[0], &(index + 1).to_string(), ui::TEXT_MUTED);
                    cell(ui, widths[1], &record.time, ui::TEXT_MUTED);
                    cell(ui, widths[2], &record.name, color);
                    cell(ui, widths[3], &record.item_type, ui::TEXT_MUTED);
                    cell(ui, widths[4], &record.rank.to_string(), color);
                    cell(ui, widths[5], &pity[index].to_string(), ui::TEXT_MUTED);
                });
            }
        });
}
