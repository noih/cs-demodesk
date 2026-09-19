//! Tick-resolution context for spray inspection; pawn positions are not hitboxes.
use super::{damage_weapon, weapon, RecoilBurst};
use crate::parser::{DemoParser, Fields, Row};
use anyhow::Result;
use parser::second_pass::game_events::GameEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub position: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub tick: i32,
    pub eye: [f64; 3],
    pub view: [f64; 2],
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub tick: i32,
    pub target_id: String,
    pub kill: bool,
}

fn position(row: Row<'_>, height: f64) -> Option<[f64; 3]> {
    let duck = row.num("duck_amount")?;
    let p = [
        row.num("X")?,
        row.num("Y")?,
        row.num("Z")? + height - 18.0 * duck,
    ];
    (duck.is_finite() && (0.0..=1.0).contains(&duck) && p.iter().all(|n| n.is_finite()))
        .then_some(p)
}

pub(crate) fn attach(
    parser: &DemoParser,
    bytes: &[u8],
    bursts: &mut BTreeMap<String, BTreeMap<String, Vec<RecoilBurst>>>,
    events: &[GameEvent],
    rate: f64,
) -> Result<()> {
    let warmup = (rate * 0.25).ceil() as i32;
    let wanted: BTreeSet<i32> = bursts
        .values()
        .flat_map(|weapons| weapons.values().flatten())
        .flat_map(|b| (b.start_tick - warmup).max(0)..=b.shots.last().expect("nonempty burst").tick)
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let props = [
        "X",
        "Y",
        "Z",
        "pitch",
        "yaw",
        "health",
        "life_state",
        "team_num",
        "duck_amount",
    ]
    .map(String::from);
    let rows = parser.ticks(bytes, &props, wanted.into_iter().collect())?;
    let mut frames: BTreeMap<i32, Vec<Row<'_>>> = BTreeMap::new();
    for row in rows.iter() {
        if let Some(tick) = row.tick() {
            frames.entry(tick).or_default().push(row);
        }
    }
    let mut contacts: BTreeMap<String, Vec<(String, Contact)>> = BTreeMap::new();
    let mut deaths = BTreeSet::new();
    for event in events {
        if !matches!(event.name.as_str(), "player_hurt" | "player_death") {
            continue;
        }
        let f = Fields(event);
        let target_id = f.str("user_steamid");
        let kill = event.name == "player_death";
        if !kill && f.int("dmg_health") <= 0 {
            continue;
        }
        if kill {
            deaths.insert((f.tick(), target_id.clone()));
        }
        if !matches!(
            (f.int("attacker_team_num"), f.int("user_team_num")),
            (2, 3) | (3, 2)
        ) {
            continue;
        }
        contacts
            .entry(f.str("attacker_steamid"))
            .or_default()
            .push((
                damage_weapon(weapon(&f.str("weapon"))).to_string(),
                Contact {
                    tick: f.tick(),
                    target_id,
                    kill,
                },
            ));
    }
    for (id, weapons) in bursts {
        for (gun, bursts) in weapons {
            for burst in bursts {
                let end = burst.shots.last().expect("nonempty burst").tick;
                burst.contacts = contacts
                    .get(id)
                    .into_iter()
                    .flatten()
                    .filter(|(name, c)| {
                        name == damage_weapon(gun)
                            && burst
                                .shots
                                .iter()
                                .any(|s| (0..=1).contains(&(c.tick - s.tick)))
                    })
                    .map(|(_, c)| c.clone())
                    .collect();
                for (&tick, players) in frames.range((burst.start_tick - warmup).max(0)..=end) {
                    let Some(shooter) = players.iter().find(|r| r.steamid().as_ref() == Some(id))
                    else {
                        continue;
                    };
                    let (Some(team), Some(mut eye), Some(pitch), Some(yaw)) = (
                        shooter.num("team_num"),
                        position(*shooter, 64.0),
                        shooter.num("pitch"),
                        shooter.num("yaw"),
                    ) else {
                        continue;
                    };
                    if !matches!(team, 2.0 | 3.0)
                        || !pitch.is_finite()
                        || pitch.abs() > 90.0
                        || !yaw.is_finite()
                    {
                        continue;
                    }
                    // Use the measured firing origin at shot ticks. Between shots eye height is estimated.
                    if let Some(shot) = burst.shots.iter().find(|s| s.tick == tick) {
                        eye = shot.origin;
                    }
                    let targets = players
                        .iter()
                        .filter_map(|r| {
                            let target_id = r.steamid()?;
                            if r.num("team_num")? != 5.0 - team
                                || (!(r.num("life_state") == Some(0.0) && r.num("health")? > 0.0)
                                    && !deaths.contains(&(tick, target_id.clone())))
                            {
                                return None;
                            }
                            // ponytail: estimated torso centre; use measured body attachments when available here.
                            Some(Target {
                                id: target_id,
                                position: position(*r, 48.0)?,
                            })
                        })
                        .collect();
                    burst.tracking.push(Frame {
                        tick,
                        eye,
                        view: [pitch, yaw],
                        targets,
                    });
                }
            }
        }
    }
    Ok(())
}
