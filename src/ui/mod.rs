//! Look and feel shared by every screen.

pub mod detail;
pub mod dialogs;
pub mod overview;
pub mod pie;
pub mod settings;
mod settings_layout;

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, Margin};
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(0x3D, 0x4C, 0xB8);
pub const CARD_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const PANEL_BG: Color32 = Color32::from_rgb(0xF4, 0xF5, 0xF7);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x62, 0x6B, 0x7C);
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0x24, 0x28, 0x30);
pub const STAR3: Color32 = Color32::from_rgb(0x5B, 0x8D, 0xEF);
pub const STAR4: Color32 = Color32::from_rgb(0x9A, 0x53, 0xE0);
pub const STAR5: Color32 = Color32::from_rgb(0xD2, 0x8B, 0x2F);
pub const DANGER: Color32 = Color32::from_rgb(0xD1, 0x3B, 0x3B);
pub const SUCCESS: Color32 = Color32::from_rgb(0x2E, 0x9E, 0x6A);

pub fn rgb((r, g, b): (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn rank_color(rank: i64) -> Color32 {
    match rank {
        5 => STAR5,
        4 => STAR4,
        _ => STAR3,
    }
}

pub fn card_margin() -> Margin {
    Margin::same(12)
}

/// Candidate CJK fonts, in order of preference.
const FONT_CANDIDATES: [(&str, u32); 16] = [
    ("C:/Windows/Fonts/msyh.ttc", 0),
    ("C:/Windows/Fonts/msyh.ttf", 0),
    ("C:/Windows/Fonts/msyhbd.ttc", 0),
    ("C:/Windows/Fonts/Deng.ttf", 0),
    ("C:/Windows/Fonts/simhei.ttf", 0),
    ("C:/Windows/Fonts/simsun.ttc", 0),
    ("/System/Library/Fonts/PingFang.ttc", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/Library/Fonts/Arial Unicode.ttf", 0),
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        0,
    ),
    ("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
    ("/usr/share/fonts/truetype/arphic/uming.ttc", 0),
    (
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        0,
    ),
];

/// Loads a system font that covers CJK text. Returns the path that was used.
pub fn install_fonts(ctx: &Context) -> Option<String> {
    let mut fonts = FontDefinitions::default();
    let mut used = None;
    for (path, index) in FONT_CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        fonts.font_data.insert("cjk".to_string(), Arc::new(data));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_string());
        }
        used = Some(path.to_string());
        break;
    }
    ctx.set_fonts(fonts);
    used
}

pub fn apply_style(ctx: &Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill = PANEL_BG;
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_STRONG);
        style.visuals.extreme_bg_color = Color32::WHITE;
        style.visuals.window_corner_radius = egui::CornerRadius::same(10);
        style.visuals.menu_corner_radius = egui::CornerRadius::same(8);
        style.visuals.selection.bg_fill = ACCENT;
        // Selected tabs draw their label in `selection.stroke`, so it must be
        // clearly readable on the accent background.
        style.visuals.selection.stroke = egui::Stroke::new(1.5, Color32::WHITE);
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xEA, 0xED, 0xF5);
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.2, TEXT_STRONG);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xDF, 0xE5, 0xFA);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, ACCENT);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
    });
}

/// Saves an egui framebuffer copy as a PNG (used by `--screenshot`).
pub fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|err| err.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
    writer
        .write_image_data(&bytes)
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// A tiny application icon drawn procedurally (no asset needed).
pub fn app_icon() -> egui::IconData {
    const SIZE: usize = 64;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = (y * SIZE + x) * 4;
            let fx = (x as f32 + 0.5) / SIZE as f32;
            let fy = (y as f32 + 0.5) / SIZE as f32;
            let mut r = (0x53 as f32 * (1.0 - fy) + 0x9C as f32 * fy) as u8;
            let mut g = (0x64 as f32 * (1.0 - fy) + 0x4D as f32 * fy) as u8;
            let mut b = (0xC8 as f32 * (1.0 - fy) + 0xE0 as f32 * fy) as u8;
            // Four pointed star in the middle.
            let dx = (fx - 0.5).abs();
            let dy = (fy - 0.5).abs();
            let star = dx.powf(0.55) + dy.powf(0.55);
            if star < 0.36 {
                r = 0xFF;
                g = 0xF3;
                b = 0xC4;
            }
            // Rounded corners.
            let corner = (fx - 0.5).abs().max((fy - 0.5).abs());
            let alpha = if corner > 0.46 { 0 } else { 255 };
            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = alpha;
        }
    }
    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}
