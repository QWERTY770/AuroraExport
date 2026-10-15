//! Excel export, reproducing the sheet layout of both upstream tools.

use crate::i18n::I18n;
use crate::model::{Account, Game};
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use std::path::Path;

const HEADER_FILL: u32 = 0xDBD7D3;
const BODY_FILL: u32 = 0xEBEBEB;
const BORDER: u32 = 0xC4C2BF;

fn rank_color(rank: i64) -> u32 {
    match rank {
        5 => 0xBD6932,
        4 => 0xA256E1,
        _ => 0x8E8E8E,
    }
}

fn sanitize_sheet_name(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '*' | '?' | ':' | '/' | '\\' | '[' | ']' => ' ',
            other => other,
        })
        .collect();
    cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        cleaned = "Sheet".to_string();
    }
    if cleaned.chars().count() > 31 {
        cleaned = cleaned.chars().take(31).collect();
    }
    cleaned
}

/// Default file name (without extension) for an export.
pub fn default_file_name(i18n: &I18n, game: Game) -> String {
    format!(
        "{}_{}",
        i18n.t(game, "excel.filePrefix"),
        crate::store::timestamp_compact()
    )
}

pub fn export(
    account: &Account,
    i18n: &I18n,
    path: &Path,
    with_raw_sheet: bool,
) -> Result<(), String> {
    let game = account.game;
    let font = i18n.t(game, "excel.customFont");
    let header_format = Format::new()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(BORDER))
        .set_background_color(Color::RGB(HEADER_FILL))
        .set_font_name(font.as_str())
        .set_font_color(Color::RGB(0x757575))
        .set_bold();

    let headers = [
        i18n.t(game, "excel.header.time"),
        i18n.t(game, "excel.header.name"),
        i18n.t(game, "excel.header.type"),
        i18n.t(game, "excel.header.rank"),
        i18n.t(game, "excel.header.total"),
        i18n.t(game, "excel.header.pity"),
        i18n.t(game, "excel.header.remark"),
    ];
    let wide = account.lang.starts_with("zh-");
    let widths: [f64; 7] = if wide {
        [24.0, 14.0, 8.0, 8.0, 8.0, 8.0, 8.0]
    } else {
        [24.0, 32.0, 16.0, 12.0, 12.0, 12.0, 8.0]
    };
    let wish2 = i18n.t(game, "excel.wish2");

    let mut workbook = Workbook::new();
    let mut used_names: Vec<String> = Vec::new();

    for bucket in &account.buckets {
        if bucket.records.is_empty() {
            continue;
        }
        let mut sheet_name = sanitize_sheet_name(&bucket.name);
        if used_names.contains(&sheet_name) {
            sheet_name = sanitize_sheet_name(&format!("{}-{}", bucket.name, bucket.key));
        }
        if used_names.contains(&sheet_name) {
            continue;
        }
        used_names.push(sheet_name.clone());

        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name(&sheet_name)
            .map_err(|err| err.to_string())?;
        worksheet
            .set_freeze_panes(1, 0)
            .map_err(|err| err.to_string())?;
        for (index, width) in widths.iter().enumerate() {
            worksheet
                .set_column_width(index as u16, *width)
                .map_err(|err| err.to_string())?;
        }
        for (index, header) in headers.iter().enumerate() {
            worksheet
                .write_string_with_format(0, index as u16, header, &header_format)
                .map_err(|err| err.to_string())?;
        }

        let mut total = 0usize;
        let mut pity = 0usize;
        for (row_index, record) in bucket.records.iter().enumerate() {
            let row = row_index as u32 + 1;
            total += 1;
            pity += 1;
            let color = rank_color(record.rank);
            let base_format = Format::new()
                .set_border(FormatBorder::Thin)
                .set_border_color(Color::RGB(BORDER))
                .set_background_color(Color::RGB(BODY_FILL))
                .set_font_name(font.as_str())
                .set_font_color(Color::RGB(color));
            let format = if record.rank != 3 {
                base_format.set_bold()
            } else {
                base_format
            };
            worksheet
                .write_string_with_format(row, 0, &record.time, &format)
                .map_err(|err| err.to_string())?;
            worksheet
                .write_string_with_format(row, 1, &record.name, &format)
                .map_err(|err| err.to_string())?;
            worksheet
                .write_string_with_format(row, 2, &record.item_type, &format)
                .map_err(|err| err.to_string())?;
            worksheet
                .write_number_with_format(row, 3, record.rank as f64, &format)
                .map_err(|err| err.to_string())?;
            worksheet
                .write_number_with_format(row, 4, total as f64, &format)
                .map_err(|err| err.to_string())?;
            worksheet
                .write_number_with_format(row, 5, pity as f64, &format)
                .map_err(|err| err.to_string())?;
            // Upstream marks the second character banner of Genshin.
            if game == Game::Genshin && bucket.key == "301" && record.gacha_type == "400" {
                worksheet
                    .write_string_with_format(row, 6, &wish2, &format)
                    .map_err(|err| err.to_string())?;
            }
            if record.rank == 5 {
                pity = 0;
            }
        }
    }

    if with_raw_sheet && !account.buckets.is_empty() {
        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name("rawData")
            .map_err(|err| err.to_string())?;
        worksheet
            .set_freeze_panes(1, 0)
            .map_err(|err| err.to_string())?;
        let columns = [
            "gacha_id",
            "gacha_type",
            "id",
            "item_id",
            "item_type",
            "lang",
            "name",
            "rank_type",
            "time",
            "uid",
        ];
        for (index, column) in columns.iter().enumerate() {
            worksheet
                .write_string(0, index as u16, *column)
                .map_err(|err| err.to_string())?;
        }
        let mut row = 1u32;
        for bucket in &account.buckets {
            for record in &bucket.records {
                let values = [
                    record.gacha_id.clone(),
                    record.gacha_type.clone(),
                    record.id.clone(),
                    record.item_id.clone(),
                    record.item_type.clone(),
                    account.lang.clone(),
                    record.name.clone(),
                    record.rank.to_string(),
                    record.time.clone(),
                    account.uid.clone(),
                ];
                for (index, value) in values.iter().enumerate() {
                    worksheet
                        .write_string(row, index as u16, value)
                        .map_err(|err| err.to_string())?;
                }
                row += 1;
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    workbook.save(path).map_err(|err| err.to_string())?;
    Ok(())
}
