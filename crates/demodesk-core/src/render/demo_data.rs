//! Shared demo facts and acquisition, independent of editing features.
use super::*;
use crate::analysis::line_of_sight::Vec3;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct Event {
    pub(super) tick: i32,
    pub(super) event_name: String,
    pub(super) weapon: String,
    pub(super) user_steamid: Option<String>,
    pub(super) attacker_steamid: Option<String>,
    pub(super) user_team_num: Option<i32>,
    pub(super) attacker_team_num: Option<i32>,
    pub(super) entityid: Option<i32>,
    pub(super) blind_duration: Option<f64>,
    pub(super) dmg_health: Option<i32>,
}

#[cfg(test)]
pub(super) fn load(path: &Path, cancel: &AtomicBool) -> Result<Vec<Event>> {
    read_events(&std::fs::read(path)?, cancel)
}

pub(super) fn read_events(bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<Event>> {
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let names = [
        "weapon_fire",
        "player_blind",
        "player_hurt",
        "player_death",
        "flashbang_detonate",
        "hegrenade_detonate",
        "round_start",
        "inferno_startburn",
        "decoy_detonate",
        "smokegrenade_detonate",
    ];
    let data = crate::parser::DemoParser::new().events(
        bytes,
        &names.map(String::from),
        &["team_num".into()],
        &[],
    )?;
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let mut events: Vec<Event> = data
        .game_events
        .iter()
        .map(|e| {
            let value: Value = serde_json::to_value(e)?;
            serde_json::from_value(value)
        })
        .collect::<std::result::Result<_, _>>()?;
    events.sort_by_key(|e| e.tick);
    Ok(events)
}

pub(super) fn gun(weapon: &str) -> bool {
    matches!(
        weapon.trim_start_matches("weapon_"),
        "ak47"
            | "m4a1"
            | "m4a1_silencer"
            | "awp"
            | "ssg08"
            | "scar20"
            | "g3sg1"
            | "aug"
            | "sg556"
            | "famas"
            | "galilar"
            | "glock"
            | "hkp2000"
            | "usp_silencer"
            | "p250"
            | "deagle"
            | "revolver"
            | "tec9"
            | "fiveseven"
            | "cz75a"
            | "elite"
            | "mac10"
            | "mp9"
            | "mp7"
            | "mp5sd"
            | "ump45"
            | "p90"
            | "bizon"
            | "nova"
            | "xm1014"
            | "mag7"
            | "sawedoff"
            | "m249"
            | "negev"
    )
}

pub(super) fn enemies(event: &Event) -> bool {
    matches!(
        (event.attacker_team_num, event.user_team_num),
        (Some(2), Some(3)) | (Some(3), Some(2))
    )
}

#[derive(Clone, Debug)]
pub(super) struct Player {
    pub(super) id: String,
    pub(super) team: i32,
    pub(super) alive: bool,
    pub(super) position: Vec3,
}
pub(super) type Tracks = BTreeMap<i32, Vec<Player>>;
#[cfg(test)]
pub(super) fn load_tracks(
    path: &Path,
    clips: &[Highlight],
    rate: i32,
    cancel: &AtomicBool,
) -> Result<Tracks> {
    read_tracks(&std::fs::read(path)?, clips, rate, cancel)
}

pub(super) fn read_tracks(
    bytes: &[u8],
    clips: &[Highlight],
    rate: i32,
    cancel: &AtomicBool,
) -> Result<Tracks> {
    let mut ticks = BTreeSet::new();
    let step = (rate / 8).max(1) as usize;
    for h in clips {
        ticks.extend(((h.start_tick - 4 * rate).max(seek::MIN_TICK)..=h.end_tick).step_by(step));
    }
    if ticks.is_empty() {
        return Ok(Tracks::new());
    }
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let rows = crate::parser::DemoParser::new().ticks(
        bytes,
        &["X", "Y", "Z", "team_num", "health"].map(String::from),
        ticks.into_iter().collect(),
    )?;
    let mut out = Tracks::new();
    for row in rows.iter() {
        let (Some(tick), Some(id), Some(x), Some(y), Some(z), Some(team), Some(health)) = (
            row.tick(),
            row.steamid(),
            row.num("X"),
            row.num("Y"),
            row.num("Z"),
            row.num("team_num"),
            row.num("health"),
        ) else {
            continue;
        };
        if id == "0" || ![x, y, z].iter().all(|v| v.is_finite()) {
            continue;
        }
        out.entry(tick).or_default().push(Player {
            id,
            team: team as i32,
            alive: health > 0.,
            position: [x, y, z],
        });
    }
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    Ok(out)
}
