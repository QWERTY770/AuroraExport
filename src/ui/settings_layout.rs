use egui::{Frame, Margin, Ui};

pub fn left_aligned_checkbox(ui: &mut Ui, value: &mut bool) -> egui::Response {
    ui.vertical(|ui| ui.checkbox(value, "")).inner
}

pub fn centered_settings_card(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    let width = ui.available_width().min(1000.0);
    let offset = ((ui.available_width() - width) * 0.5).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(offset);
        Frame::new()
            .fill(egui::Color32::WHITE)
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width((width - 40.0).max(280.0));
                    contents(ui);
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkbox_icons(shape: &egui::epaint::Shape, positions: &mut Vec<f32>) {
        match shape {
            egui::epaint::Shape::Rect(rect)
                if (rect.rect.width() - rect.rect.height()).abs() < 0.1
                    && (10.0..=24.0).contains(&rect.rect.width()) =>
            {
                positions.push(rect.rect.center().x);
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    checkbox_icons(shape, positions);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn standalone_checkbox_aligns_with_checkboxes_above_hints() {
        for width in [840.0, 1024.0, 1600.0] {
            let ctx = egui::Context::default();
            ctx.set_theme(egui::ThemePreference::Light);
            ctx.all_styles_mut(|style| {
                style.spacing.item_spacing = egui::vec2(8.0, 6.0);
                style.spacing.button_padding = egui::vec2(10.0, 5.0);
                style.spacing.interact_size.y = 28.0;
            });
            let mut positions = Vec::new();
            for _ in 0..3 {
                positions.clear();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 680.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        centered_settings_card(ui, |ui| {
                            egui::Grid::new("aligned-settings")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Fetch full history");
                                    ui.vertical(|ui| {
                                        let _ = ui.checkbox(&mut false, "");
                                        ui.label("A descriptive hint makes this column wider");
                                    });
                                    ui.end_row();
                                    ui.label("Hide novice");
                                    let _ = left_aligned_checkbox(ui, &mut true);
                                    ui.end_row();
                                    ui.label("Proxy mode");
                                    ui.vertical(|ui| {
                                        let _ = ui.checkbox(&mut false, "");
                                        ui.label("Another hint");
                                    });
                                    ui.end_row();
                                });
                        });
                    },
                );
                for clipped in &output.shapes {
                    checkbox_icons(&clipped.shape, &mut positions);
                }
            }
            assert_eq!(positions.len(), 3);
            for position in &positions[1..] {
                assert!(
                    (position - positions[0]).abs() < 0.5,
                    "misaligned checkboxes at {width}px: {positions:?}"
                );
            }
        }
    }

    fn visible_text(shape: &egui::epaint::Shape, clip: egui::Rect, labels: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) if shape.visual_bounding_rect().intersects(clip) => {
                labels.push(text.galley.text().to_string());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    visible_text(shape, clip, labels);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn card_renders_content_below_the_header() {
        for width in [840.0, 1024.0, 1600.0] {
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 680.0));
            let mut labels = Vec::new();
            for _ in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ui| {
                        centered_settings_card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.heading("Settings");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let _ = ui.button("Close");
                                    },
                                );
                            });
                            ui.heading("Fetching");
                            ui.label("Log type");
                        });
                    },
                );
                labels.clear();
                for clipped in output.shapes {
                    visible_text(
                        &clipped.shape,
                        clipped.clip_rect.intersect(screen),
                        &mut labels,
                    );
                }
            }
            for expected in ["Settings", "Fetching", "Log type"] {
                assert!(
                    labels.iter().any(|label| label == expected),
                    "{expected} is off-screen at {width}px: {labels:?}"
                );
            }
        }
    }
}
