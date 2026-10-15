//! Per banner statistics, mirroring the upstream `gachaDetail.js`.

use crate::model::{Account, Game, Record};

/// Pie chart colours, matching the upstream ECharts palette.
pub const PIE_COLORS: [(u8, u8, u8); 5] = [
    (250, 200, 88),  // 5 star character
    (238, 102, 102), // 5 star weapon
    (84, 112, 198),  // 4 star character
    (145, 204, 117), // 4 star weapon
    (115, 192, 222), // 3 star weapon
];

const NAME_COLORS: [(u8, u8, u8); 14] = [
    (84, 112, 198),
    (250, 200, 88),
    (238, 102, 102),
    (115, 192, 222),
    (59, 162, 114),
    (252, 132, 82),
    (154, 96, 180),
    (234, 124, 204),
    (42, 183, 202),
    (0, 91, 150),
    (255, 139, 148),
    (114, 160, 7),
    (182, 13, 27),
    (22, 87, 13),
];

const GENSHIN_WEAPONS: [&str; 10] = [
    "武器", "Weapon", "무기", "Arma", "Arme", "Оружие", "อาวุธ", "Vũ Khí", "Waffe", "Senjata",
];
const GENSHIN_CHARACTERS: [&str; 12] = [
    "角色",
    "Character",
    "캐릭터",
    "キャラクター",
    "Personaje",
    "Personnage",
    "Персонажи",
    "ตัวละคร",
    "Nhân Vật",
    "Figur",
    "Karakter",
    "Personagem",
];
const STARRAIL_WEAPONS: [&str; 13] = [
    "光锥",
    "光錐",
    "Lichtkegel",
    "Light Cone",
    "Conos de luz",
    "cônes de lumière",
    "光円錐",
    "광추",
    "무기",
    "Cones de Luz",
    "Световые конусы",
    "Nón Ánh Sáng",
    "Cône de lumière",
];
const STARRAIL_CHARACTERS: [&str; 14] = [
    "角色",
    "角色",
    "Figur",
    "Character",
    "Personajes",
    "Personnages",
    "Karakter",
    "キャラクター",
    "キャラ",
    "캐릭터",
    "Personagens",
    "Персонажи",
    "ตัวละคร",
    "Nhân Vật",
];

pub fn is_weapon(game: Game, item_type: &str) -> bool {
    let table: &[&str] = match game {
        Game::Genshin => &GENSHIN_WEAPONS,
        Game::StarRail => &STARRAIL_WEAPONS,
    };
    table.contains(&item_type)
}

pub fn is_character(game: Game, item_type: &str) -> bool {
    let table: &[&str] = match game {
        Game::Genshin => &GENSHIN_CHARACTERS,
        Game::StarRail => &STARRAIL_CHARACTERS,
    };
    table.contains(&item_type)
}

/// Colour for an item name, stable across runs.
pub fn name_color(name: &str) -> (u8, u8, u8) {
    let mut hash: i32 = 0;
    for ch in name.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(ch as i32);
    }
    let index = (hash.unsigned_abs() as usize) % NAME_COLORS.len();
    NAME_COLORS[index]
}

#[derive(Clone, Debug)]
pub struct SsrEntry {
    pub name: String,
    /// Number of pulls it took, including this one.
    pub pity: usize,
    pub time: String,
    pub item_type: String,
    pub color: (u8, u8, u8),
}

#[derive(Clone, Debug, Default)]
pub struct BannerStats {
    pub key: String,
    pub name: String,
    pub total: usize,
    pub count3: usize,
    pub count4: usize,
    pub count5: usize,
    pub count3_weapon: usize,
    pub count4_weapon: usize,
    pub count5_weapon: usize,
    pub count4_character: usize,
    pub count5_character: usize,
    /// Pulls since the last 5 star, i.e. current pity.
    pub since_last_5: usize,
    pub first_time: String,
    pub last_time: String,
    pub ssr: Vec<SsrEntry>,
}

impl BannerStats {
    pub fn average_5(&self) -> f64 {
        if self.ssr.is_empty() {
            return 0.0;
        }
        let sum: usize = self.ssr.iter().map(|entry| entry.pity).sum();
        ((sum as f64 / self.ssr.len() as f64) * 100.0).round() / 100.0
    }

    pub fn percent(&self, count: usize) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        ((count as f64 / self.total as f64) * 10000.0).round() / 100.0
    }

    /// Pie slices: `(label key, count, color index)`.
    pub fn pie_slices(&self, include_3: bool) -> Vec<(String, usize, usize)> {
        let mut slices = Vec::new();
        if self.count5_character > 0 {
            slices.push(("ui.data.chara5".to_string(), self.count5_character, 0));
        }
        if self.count5_weapon > 0 {
            slices.push(("ui.data.weapon5".to_string(), self.count5_weapon, 1));
        }
        if self.count4_character > 0 {
            slices.push(("ui.data.chara4".to_string(), self.count4_character, 2));
        }
        if self.count4_weapon > 0 {
            slices.push(("ui.data.weapon4".to_string(), self.count4_weapon, 3));
        }
        if include_3 && self.count3_weapon > 0 {
            slices.push(("ui.data.weapon3".to_string(), self.count3_weapon, 4));
        }
        if slices.is_empty() && self.count5 > 0 {
            slices.push(("ui.data.star5".to_string(), self.count5, 0));
        }
        slices
    }
}

pub fn compute(game: Game, bucket_key: &str, bucket_name: &str, records: &[Record]) -> BannerStats {
    let mut stats = BannerStats {
        key: bucket_key.to_string(),
        name: bucket_name.to_string(),
        total: records.len(),
        first_time: records.first().map(|r| r.time.clone()).unwrap_or_default(),
        last_time: records.last().map(|r| r.time.clone()).unwrap_or_default(),
        ..Default::default()
    };
    let mut last_ssr_index = 0usize;
    for (index, record) in records.iter().enumerate() {
        match record.rank {
            3 => {
                stats.count3 += 1;
                stats.since_last_5 += 1;
                if is_weapon(game, &record.item_type) {
                    stats.count3_weapon += 1;
                }
            }
            4 => {
                stats.count4 += 1;
                stats.since_last_5 += 1;
                if is_weapon(game, &record.item_type) {
                    stats.count4_weapon += 1;
                } else if is_character(game, &record.item_type) {
                    stats.count4_character += 1;
                }
            }
            5 => {
                stats.ssr.push(SsrEntry {
                    name: record.name.clone(),
                    pity: index + 1 - last_ssr_index,
                    time: record.time.clone(),
                    item_type: record.item_type.clone(),
                    color: name_color(&record.name),
                });
                last_ssr_index = index + 1;
                stats.count5 += 1;
                stats.since_last_5 = 0;
                if is_weapon(game, &record.item_type) {
                    stats.count5_weapon += 1;
                } else if is_character(game, &record.item_type) {
                    stats.count5_character += 1;
                }
            }
            _ => {}
        }
    }
    // Time range: prefer the earliest/latest timestamp, falling back to order.
    let parse = |text: &str| {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| dt.and_utc().timestamp())
    };
    let mut min: Option<(i64, String)> = None;
    let mut max: Option<(i64, String)> = None;
    for record in records {
        let Some(ts) = parse(&record.time) else { continue };
        if min.as_ref().is_none_or(|(value, _)| ts < *value) {
            min = Some((ts, record.time.clone()));
        }
        if max.as_ref().is_none_or(|(value, _)| ts > *value) {
            max = Some((ts, record.time.clone()));
        }
    }
    if let Some((_, time)) = min {
        stats.first_time = time;
    }
    if let Some((_, time)) = max {
        stats.last_time = time;
    }
    stats
}

/// Statistics for every banner of an account, in stored order.
pub fn compute_all(account: &Account) -> Vec<BannerStats> {
    account
        .buckets
        .iter()
        .map(|bucket| compute(account.game, &bucket.key, &bucket.name, &bucket.records))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(rank: i64, name: &str) -> Record {
        Record {
            rank,
            name: name.to_string(),
            item_type: "角色".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn counts_pity_like_upstream() {
        let records = vec![
            rec(3, "a"),
            rec(3, "b"),
            rec(4, "c"),
            rec(5, "d"),
            rec(3, "e"),
            rec(5, "f"),
        ];
        let stats = compute(Game::Genshin, "301", "角色活动祈愿", &records);
        assert_eq!(stats.total, 6);
        assert_eq!(stats.count3, 3);
        assert_eq!(stats.count4, 1);
        assert_eq!(stats.count5, 2);
        assert_eq!(stats.ssr[0].pity, 4);
        assert_eq!(stats.ssr[1].pity, 2);
        assert_eq!(stats.since_last_5, 0);
        assert_eq!(stats.average_5(), 3.0);
        assert_eq!(stats.count5_character, 2);
    }
}
