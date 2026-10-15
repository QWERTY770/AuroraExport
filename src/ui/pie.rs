//! A small pie chart drawn with the egui painter (replaces the ECharts chart).

use egui::{Align2, Color32, FontId, Response, Sense, Shape, Stroke, Ui, Vec2};

pub struct Slice {
    pub label: String,
    pub count: usize,
    pub color: Color32,
}

pub fn pie(ui: &mut Ui, slices: &[Slice], height: f32) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let total: usize = slices.iter().map(|slice| slice.count).sum();
    if slices.is_empty() || total == 0 {
        return response;
    }
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = (rect.height().min(width) * 0.5 - 6.0).max(10.0);
    let start_angle = 70f32.to_radians();
    let tau = std::f32::consts::TAU;

    let hovered = response.hover_pos().and_then(|pos| {
        let offset = pos - center;
        if offset.length() > radius {
            return None;
        }
        let mut angle = offset.y.atan2(offset.x);
        while angle < start_angle {
            angle += tau;
        }
        let mut cursor = start_angle;
        for (index, slice) in slices.iter().enumerate() {
            let sweep = tau * (slice.count as f32 / total as f32);
            if angle >= cursor && angle < cursor + sweep {
                return Some(index);
            }
            cursor += sweep;
        }
        None
    });

    let mut cursor = start_angle;
    for (index, slice) in slices.iter().enumerate() {
        let sweep = tau * (slice.count as f32 / total as f32);
        if sweep >= tau - 1e-4 {
            // A single slice is a full circle; a wedge would be degenerate.
            painter.circle_filled(center, radius, slice.color);
        } else {
            let steps = ((sweep / 0.08).ceil() as usize).max(2);
            let mut points = Vec::with_capacity(steps + 2);
            points.push(center);
            for step in 0..=steps {
                let angle = cursor + sweep * (step as f32 / steps as f32);
                points.push(center + Vec2::new(angle.cos(), angle.sin()) * radius);
            }
            painter.add(Shape::convex_polygon(points, slice.color, Stroke::NONE));
            if hovered == Some(index) {
                // Outline the wedge: centre -> arc -> back to the centre, so the
                // highlight follows the slice instead of cutting across it.
                let mut outline = Vec::with_capacity(steps + 2);
                outline.push(center);
                for step in 0..=steps {
                    let angle = cursor + sweep * (step as f32 / steps as f32);
                    outline.push(center + Vec2::new(angle.cos(), angle.sin()) * radius);
                }
                outline.push(center);
                painter.add(Shape::line(
                    outline,
                    Stroke::new(2.0, Color32::from_black_alpha(120)),
                ));
            }
        }
        // Label large slices in place.
        let share = slice.count as f32 / total as f32;
        if share >= 0.08 {
            let mid = cursor + sweep * 0.5;
            let label_pos = center + Vec2::new(mid.cos(), mid.sin()) * (radius * 0.62);
            painter.text(
                label_pos,
                Align2::CENTER_CENTER,
                format!("{:.0}%", share * 100.0),
                FontId::proportional(11.0),
                Color32::WHITE,
            );
        }
        cursor += sweep;
    }

    if let Some(index) = hovered {
        let slice = &slices[index];
        let share = slice.count as f32 / total as f32 * 100.0;
        return response.on_hover_text(format!("{}: {} ({:.2}%)", slice.label, slice.count, share));
    }
    response
}
