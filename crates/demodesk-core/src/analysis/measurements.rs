//! Unrounded demo measurements shared by analysis consumers.
//! Player origins and recorded flash state are NOT body points or visibility verdicts.
use crate::parser::{DemoParser, Fields, Row};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

const PROPS: &[&str] = &[
    "X",
    "Y",
    "Z",
    "pitch",
    "yaw",
    "health",
    "life_state",
    "team_num",
    "duck_amount",
    "flash_duration",
    "flash_max_alpha",
];
const OPTIONAL: &[&str] = &[
    "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_nHitboxSet",
    "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_flRootBoneOffset_x",
    "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_flRootBoneOffset_y",
    "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_flRootBoneOffset_z",
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flDuckViewOffset",
    "CCSPlayerPawn.CCSPlayer_CameraServices.m_vecCsViewPunchAngle",
];
const EVENTS: &[&str] = &[
    "player_blind",
    "player_death",
    "round_start",
    "smokegrenade_detonate",
    "smokegrenade_expired",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSample {
    pub tick: i32,
    pub player_id: String,
    pub origin: Option<[f64; 3]>,
    pub view: Option<[f64; 2]>,
    pub team: Option<f64>,
    pub alive: Option<bool>,
    pub duck_amount: Option<f64>,
    pub flash_duration: Option<f64>,
    pub flash_max_alpha: Option<f64>,
    /// Remaining event duration, not remaining full-white screen time.
    pub blind_event_remaining_seconds: Option<f64>,
    pub hitbox_set: Option<f64>,
    pub root_bone_offset: Option<[f64; 3]>,
    pub duck_view_offset: Option<f64>,
    pub view_punch: Option<[f64; 3]>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    pub tick: i32,
    pub kind: String,
    pub player_id: Option<String>,
    pub duration_seconds: Option<f64>,
    pub position: Option<[f64; 3]>,
    pub entity_id: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurements {
    pub schema_version: u32,
    pub demo_fingerprint: String,
    pub map_name: Option<String>,
    pub tick_rate: f64,
    pub tick_rate_source: &'static str,
    pub first_tick: i32,
    pub last_tick: i32,
    pub sample_step_ticks: u32,
    pub available_optional_properties: Vec<String>,
    pub decoded_counts: BTreeMap<&'static str, usize>,
    pub limitations: Vec<&'static str>,
    pub events: Vec<RecordedEvent>,
    pub samples: Vec<PlayerSample>,
}

/// Caller supplies a verified tick rate; the current parser does not expose server tick_interval.
pub fn extract(
    parser: &DemoParser,
    bytes: &[u8],
    first: i32,
    last: i32,
    tick_rate: f64,
) -> Result<Measurements> {
    ensure!(first >= 0 && last >= first, "invalid tick range");
    ensure!(
        tick_rate.is_finite() && tick_rate > 0.0,
        "invalid tick rate"
    );
    let names = parser.property_names(bytes)?;
    let optional: Vec<String> = OPTIONAL
        .iter()
        .filter(|name| names.binary_search_by(|n| n.as_str().cmp(name)).is_ok())
        .map(|n| (*n).into())
        .collect();
    let props: Vec<String> = PROPS
        .iter()
        .map(|p| (*p).into())
        .chain(optional.iter().cloned())
        .collect();
    let events_out = parser.events(
        bytes,
        &EVENTS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        &[],
        &[],
    )?;
    let mut events = Vec::new();
    for event in &events_out.game_events {
        if event.tick > last {
            continue;
        }
        let f = Fields(event);
        let duration = f
            .opt_num("blind_duration")
            .filter(|n| n.is_finite() && *n > 0.0);
        let position = ["x", "y", "z"].map(|name| f.opt_num(name).filter(|n| n.is_finite()));
        events.push(RecordedEvent {
            tick: event.tick,
            kind: event.name.clone(),
            player_id: f.opt_str("user_steamid").filter(|id| id != "0"),
            duration_seconds: duration,
            position: triple(position),
            entity_id: f.opt_num("entityid"),
        });
    }
    events.sort_by_key(|e| e.tick);
    let rows = parser.ticks(bytes, &props, (first..=last).collect())?;
    let mut samples = Vec::new();
    for row in rows.iter() {
        let (Some(tick), Some(player_id)) = (row.tick(), row.steamid()) else {
            continue;
        };
        if player_id == "0" {
            continue;
        }
        let view = match (number(row, "pitch"), number(row, "yaw")) {
            (Some(p), Some(y)) if p.abs() <= 90.0 => Some([p, y]),
            _ => None,
        };
        let alive = match number(row, "life_state") {
            Some(0.0) => Some(true),
            Some(1.0 | 2.0) => Some(false),
            _ => None,
        };
        samples.push(PlayerSample {
            tick,
            player_id,
            origin: triple(["X", "Y", "Z"].map(|p| number(row, p))),
            view,
            team: number(row, "team_num"),
            alive,
            duck_amount: number(row, "duck_amount"),
            flash_duration: number(row, "flash_duration"),
            flash_max_alpha: number(row, "flash_max_alpha"),
            blind_event_remaining_seconds: None,
            hitbox_set: number(row, OPTIONAL[0]),
            root_bone_offset: triple(
                [OPTIONAL[1], OPTIONAL[2], OPTIONAL[3]].map(|p| number(row, p)),
            ),
            duck_view_offset: number(row, OPTIONAL[4]),
            view_punch: row
                .vec3(OPTIONAL[5])
                .filter(|v| v.iter().all(|n| n.is_finite())),
        });
    }
    samples.sort_by(|a, b| (a.tick, &a.player_id).cmp(&(b.tick, &b.player_id)));
    annotate_blinds(&mut samples, &events, tick_rate);
    let decoded_counts = [
        (
            "origin",
            samples.iter().filter(|s| s.origin.is_some()).count(),
        ),
        ("view", samples.iter().filter(|s| s.view.is_some()).count()),
        (
            "alive",
            samples.iter().filter(|s| s.alive.is_some()).count(),
        ),
        (
            "flashMaxAlpha",
            samples
                .iter()
                .filter(|s| s.flash_max_alpha.is_some())
                .count(),
        ),
        (
            "blindEventActive",
            samples
                .iter()
                .filter(|s| s.blind_event_remaining_seconds.is_some())
                .count(),
        ),
        (
            "hitboxSet",
            samples.iter().filter(|s| s.hitbox_set.is_some()).count(),
        ),
        (
            "rootBoneOffset",
            samples
                .iter()
                .filter(|s| s.root_bone_offset.is_some())
                .count(),
        ),
        (
            "duckViewOffset",
            samples
                .iter()
                .filter(|s| s.duck_view_offset.is_some())
                .count(),
        ),
        (
            "viewPunch",
            samples.iter().filter(|s| s.view_punch.is_some()).count(),
        ),
    ]
    .into_iter()
    .collect();
    Ok(Measurements {
        schema_version: 1, demo_fingerprint: sha1_smol::Sha1::from(bytes).digest().to_string(),
        map_name: parser.header(bytes)?.remove("map_name"), tick_rate, tick_rate_source: "caller-supplied; verify against recording", first_tick: first, last_tick: last, sample_step_ticks: 1,
        available_optional_properties: optional, decoded_counts,
        limitations: vec!["Origins/root offsets are not eye positions or animated body points.", "Blind event duration and maximum alpha do not establish full-screen blindness.", "Smoke events are not the rendered smoke volume.", "No wall line-of-sight classification; map geometry and dynamic occluders are still required."],
        events, samples,
    })
}
fn number(row: Row<'_>, key: &str) -> Option<f64> {
    row.num(key).filter(|n| n.is_finite())
}
fn triple(v: [Option<f64>; 3]) -> Option<[f64; 3]> {
    Some([v[0]?, v[1]?, v[2]?])
}

fn annotate_blinds(samples: &mut [PlayerSample], events: &[RecordedEvent], rate: f64) {
    let mut expiry = HashMap::<&str, f64>::new();
    let mut next = 0;
    for sample in samples {
        while let Some(event) = events.get(next).filter(|e| e.tick <= sample.tick) {
            match event.kind.as_str() {
                "round_start" => expiry.clear(),
                "player_death" => {
                    if let Some(id) = event.player_id.as_deref() {
                        expiry.remove(id);
                    }
                }
                "player_blind" => {
                    if let (Some(id), Some(duration)) =
                        (event.player_id.as_deref(), event.duration_seconds)
                    {
                        // Each event describes its own interval; a later shorter flash cannot erase an earlier interval.
                        let end = f64::from(event.tick) + duration * rate;
                        expiry
                            .entry(id)
                            .and_modify(|until| *until = until.max(end))
                            .or_insert(end);
                    }
                }
                _ => {}
            }
            next += 1;
        }
        if sample.alive == Some(false) {
            expiry.remove(sample.player_id.as_str());
        }
        sample.blind_event_remaining_seconds = if sample.alive == Some(true) {
            expiry
                .get(sample.player_id.as_str())
                .map(|until| (until - f64::from(sample.tick)) / rate)
                .filter(|seconds| *seconds > 0.0)
        } else {
            None
        };
    }
}

/// Increment schema for incompatible data changes; implementation for algorithm changes.
pub fn contract() -> super::Contract {
    super::Contract {
        module: "player-measurements".into(),
        schema_version: 1,
        implementation_version: "0.2.0".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(tick: i32) -> PlayerSample {
        PlayerSample {
            tick,
            player_id: "subject".into(),
            origin: None,
            view: None,
            team: None,
            alive: Some(true),
            duck_amount: None,
            flash_duration: Some(5.0),
            flash_max_alpha: Some(255.0),
            blind_event_remaining_seconds: None,
            hitbox_set: None,
            root_bone_offset: None,
            duck_view_offset: None,
            view_punch: None,
        }
    }
    fn event(tick: i32, kind: &str, duration: Option<f64>) -> RecordedEvent {
        RecordedEvent {
            tick,
            kind: kind.into(),
            player_id: Some("subject".into()),
            duration_seconds: duration,
            position: None,
            entity_id: None,
        }
    }
    #[test]
    fn blind_intervals_use_events_expire_and_reset_at_death_or_round_start() {
        let events = vec![
            event(0, "player_blind", Some(2.0)),
            event(10, "player_blind", Some(0.1)),
            event(140, "player_blind", Some(10.0)),
            event(160, "player_death", None),
            event(180, "player_blind", Some(10.0)),
            event(200, "round_start", None),
        ];
        let mut samples: Vec<_> = [0, 20, 128, 150, 170, 190, 210].map(sample).into();
        annotate_blinds(&mut samples, &events, 64.0);
        assert_eq!(samples[0].blind_event_remaining_seconds, Some(2.0));
        assert_eq!(samples[1].blind_event_remaining_seconds, Some(1.6875));
        assert_eq!(samples[2].blind_event_remaining_seconds, None);
        assert!(samples[3].blind_event_remaining_seconds.is_some());
        assert_eq!(samples[4].blind_event_remaining_seconds, None);
        assert!(samples[5].blind_event_remaining_seconds.is_some());
        assert_eq!(samples[6].blind_event_remaining_seconds, None);
    }
    #[test]
    fn stale_flash_properties_and_missing_life_state_do_not_prove_blindness() {
        let mut samples = vec![sample(0), sample(1), sample(2)];
        annotate_blinds(&mut samples, &[], 64.0);
        assert!(samples
            .iter()
            .all(|s| s.blind_event_remaining_seconds.is_none()));
        samples[0].alive = None;
        samples[1].alive = Some(false);
        annotate_blinds(&mut samples, &[event(0, "player_blind", Some(10.0))], 64.0);
        assert!(samples
            .iter()
            .all(|s| s.blind_event_remaining_seconds.is_none()));
        assert_eq!(triple([Some(0.0), None, Some(0.0)]), None);
    }
}
