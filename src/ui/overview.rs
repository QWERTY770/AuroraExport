//! Overview page: one card per banner with a pie chart and the statistics.

use crate::app::App;
use crate::stats::{BannerStats, PIE_COLORS};
use crate::ui::{self, pie};
use egui::{Align, CornerRadius, Frame, Layout, RichText, ScrollArea, Ui};

fn date_only(time: &str) -> String {
    if time.len() >= 10 {
        time[..10].to_string()
    } else {
        time.to_string()
    }
}

fn legend_row(ui: &mut Ui, slices: &[pie::Slice], total: usize) {
    for slice in slices {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(2), slice.color);
            ui.label(RichText::new(&slice.label).size(11.0).color(ui::TEXT_STRONG));
            let percent = if total == 0 {
                0.0
            } else {
                slice.count as f64 / total as f64 * 100.0
            };
            ui.label(
                RichText::new(format!("{} ({percent:.2}%)", slice.count))
                    .size(11.0)
                    .color(ui::TEXT_MUTED),
            );
        });
    }
}

fn card(app: &App, ui: &mut Ui, stat: &BannerStats, width: f32) {
    let text = |key: &str| app.i18n.t(app.game, key);
    let colon = app.i18n.t(app.game, "symbol.colon");

    Frame::group(ui.style())
        .fill(ui::CARD_BG)
        .inner_margin(ui::card_margin())
        .corner_radius(CornerRadius::same(10))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(&stat.name).size(14.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "{} - {}",
                            date_only(&stat.first_time),
                            date_only(&stat.last_time)
                        ))
                        .size(11.0)
                        .color(ui::TEXT_MUTED),
                    );
                });

                // Pie chart of the item composition.
                let slices: Vec<pie::Slice> = stat
                    .pie_slices(app.config.pie_include_3)
                    .into_iter()
                    .map(|(label_key, count, color_index)| pie::Slice {
                        label: app.i18n.t(app.game, &label_key),
                        count,
                        color: ui::rgb(PIE_COLORS[color_index.min(PIE_COLORS.len() - 1)]),
                    })
                    .collect();
                let pie_total: usize = slices.iter().map(|slice| slice.count).sum();
                if slices.is_empty() {
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("—").size(20.0).color(ui::TEXT_MUTED));
                    });
                    ui.add_space(6.0);
                } else {
                    pie::pie(ui, &slices, 132.0);
                    legend_row(ui, &slices, pie_total);
                }

                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{}{}{}{}",
                        text("ui.data.total"),
                        colon,
                        stat.total,
                        text("ui.data.times")
                    ))
                    .size(12.0)
                    .color(ui::TEXT_STRONG),
                );
                if stat.since_last_5 > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{}{}{}",
                            text("ui.data.sum"),
                            stat.since_last_5,
                            text("ui.data.no5star")
                        ))
                        .size(12.0)
                        .color(ui::SUCCESS),
                    );
                }

                for (label_, count, color) in [
                    ("ui.data.star5", stat.count5, ui::STAR5),
                    ("ui.data.star4", stat.count4, ui::STAR4),
                    ("ui.data.star3", stat.count3, ui::STAR3),
                ] {
                    ui.label(
                        RichText::new(format!(
                            "{}{}{} [{:.2}%]",
                            text(label_),
                            colon,
                            count,
                            stat.percent(count)
                        ))
                        .size(12.0)
                        .color(color),
                    );
                }

                if !stat.ssr.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{}{}", text("ui.data.history"), colon))
                            .size(12.0)
                            .color(ui::TEXT_STRONG),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for entry in &stat.ssr {
                            let mut rich = RichText::new(format!("{}[{}]", entry.name, entry.pity))
                                .size(12.0)
                                .color(ui::rgb(entry.color));
                            if entry.item_type.is_empty() {
                                rich = rich.italics();
                            }
                            ui.label(rich).on_hover_text(format!(
                                "{}\n{}",
                                entry.time,
                                entry.item_type
                            ));
                        }
                    });
                    ui.label(
                        RichText::new(format!(
                            "{}{}{}",
                            text("ui.data.average"),
                            colon,
                            stat.average_5()
                        ))
                        .size(12.0)
                        .color(ui::SUCCESS),
                    );
                }
            });
        });
}

pub fn overview(app: &App, ui: &mut Ui, stats: &[BannerStats]) {
    let hide_novice = app.config.hide_novice(app.game);
    let novice_key = app.game.novice_key();
    let visible: Vec<&BannerStats> = stats
        .iter()
        .filter(|stat| stat.total > 0 && !(hide_novice && stat.key == novice_key))
        .collect();

    if visible.is_empty() {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space(80.0);
            ui.label(
                RichText::new(app.i18n.t(app.game, "ui.hint.noData"))
                    .size(13.0)
                    .color(ui::TEXT_MUTED),
            );
        });
        return;
    }

    let spacing = 12.0;
    let available = ui.available_width();
    let columns = (((available + spacing) / 330.0).floor() as usize).clamp(1, 4);
    let card_width = (available - spacing * (columns.saturating_sub(1) as f32)) / columns as f32;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(spacing, spacing);
            for row in visible.chunks(columns) {
                ui.horizontal_top(|ui| {
                    for stat in row {
                        card(app, ui, stat, card_width - 26.0);
                    }
                });
            }
            ui.add_space(8.0);
        });
}
