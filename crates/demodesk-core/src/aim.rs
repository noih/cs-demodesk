//! Shot-based aim statistics. Visibility-dependent metrics are intentionally separate.
use crate::model::RoundInfo;
use crate::parser::Fields;
use parser::second_pass::game_events::GameEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AimStats {
    pub shots: u32,
    pub hits: u32,
    pub head_hits: u32,
    pub head_eligible_hits: u32,
    pub first_shots: u32,
    pub first_hits: u32,
    pub spray_shots: u32,
    pub spray_hits: u32,
}

fn weapon(name: &str) -> &str { name.strip_prefix("weapon_").unwrap_or(name) }
fn damage_weapon(name: &str) -> &str {
    match name { "usp_silencer" => "hkp2000", "m4a1_silencer" => "m4a1", _ => name }
}
fn group(name: &str) -> Option<&'static str> {
    match name {
        "ak47" | "m4a1" | "m4a1_silencer" | "famas" | "galilar" | "aug" | "sg556" => Some("rifles"),
        "awp" => Some("awp"),
        "glock" | "hkp2000" | "usp_silencer" | "p250" | "tec9" | "fiveseven" | "cz75a" | "elite" | "deagle" | "revolver" => Some("pistols"),
        "ssg08" | "scar20" | "g3sg1" | "mac10" | "mp9" | "mp7" | "mp5sd" | "ump45" | "bizon" | "p90" | "nova" | "xm1014" | "mag7" | "sawedoff" | "m249" | "negev" => Some("other"),
        _ => None,
    }
}

pub fn compute(events: &[GameEvent], rounds: &[RoundInfo], tick_rate: f64) -> BTreeMap<String, BTreeMap<String, AimStats>> {
    struct Shot { tick: i32, round: i32, weapon: String, hit: bool, head: bool }
    let round_of = |f: &Fields<'_>| rounds.iter().find(|r| !f.bool("is_freeze_period") && f.tick() >= r.start_tick && f.tick() <= r.officially_ended_tick);
    let mut shots: HashMap<String, Vec<Shot>> = HashMap::new();
    for event in events.iter().filter(|e| e.name == "weapon_fire") {
        let f = Fields(event);
        let Some(round) = round_of(&f) else { continue };
        let name = f.str("weapon");
        let name = weapon(&name);
        let id = f.str("user_steamid");
        if id.is_empty() || group(name).is_none() { continue; }
        shots.entry(id).or_default().push(Shot { tick: f.tick(), round: round.round, weapon: name.into(), hit: false, head: false });
    }
    for player in shots.values_mut() { player.sort_by_key(|s| s.tick); }
    for event in events.iter().filter(|e| e.name == "player_hurt") {
        let f = Fields(event);
        let Some(round) = round_of(&f) else { continue };
        let (attacker, victim) = (f.str("attacker_steamid"), f.str("user_steamid"));
        if attacker.is_empty() || victim.is_empty() || attacker == victim || f.int("dmg_health") <= 0 { continue; }
        let side = |field, id: &str| match f.int(field) { 2 => Some(crate::model::Team::T), 3 => Some(crate::model::Team::Ct), _ => round.roster.get(id).copied() };
        let (Some(a), Some(v)) = (side("attacker_team_num", &attacker), side("user_team_num", &victim)) else { continue };
        if a == v { continue; }
        let Some(player) = shots.get_mut(&attacker) else { continue };
        let index = player.partition_point(|s| s.tick <= f.tick());
        let Some(shot) = index.checked_sub(1).and_then(|i| player.get_mut(i)) else { continue };
        // Hits can precede their fire event in message order; use tick order and
        // deduplicate pellets / penetrations onto one fired shot, never count hits as bullets.
        if shot.round == round.round && f.tick() - shot.tick <= 1 && damage_weapon(&shot.weapon) == damage_weapon(weapon(&f.str("weapon"))) {
            shot.hit = true;
            shot.head |= f.str("hitgroup") == "head";
        }
    }
    let mut out = BTreeMap::new();
    for (id, player) in shots {
        let mut groups: BTreeMap<String, AimStats> = BTreeMap::new();
        let mut start = 0;
        while start < player.len() {
            let mut end = start + 1;
            // Local burst definition: same gun and round, gaps <= 300 ms.
            while end < player.len() && player[end].round == player[start].round && player[end].weapon == player[start].weapon && (player[end].tick-player[end-1].tick) as f64 <= tick_rate * 0.3 { end += 1; }
            let category = group(&player[start].weapon).expect("only firearms collected");
            for key in ["all", category] {
                let s = groups.entry(key.into()).or_default();
                for (i, shot) in player[start..end].iter().enumerate() {
                    s.shots += 1;
                    s.hits += u32::from(shot.hit);
                    if shot.weapon != "awp" {
                        s.head_eligible_hits += u32::from(shot.hit);
                        s.head_hits += u32::from(shot.head);
                    }
                    if i == 0 { s.first_shots += 1; s.first_hits += u32::from(shot.hit); }
                    if category == "rifles" && end-start >= 3 { s.spray_shots += 1; s.spray_hits += u32::from(shot.hit); }
                }
            }
            start = end;
        }
        out.insert(id, groups);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser::second_pass::game_events::EventField;
    use parser::second_pass::variants::Variant;
    #[test]
    fn pairs_aliases_and_deduplicates_hits_with_burst_and_round_boundaries() {
        let round: RoundInfo = serde_json::from_value(serde_json::json!({"round":1,"startTick":0,"freezeEndTick":0,"endTick":100,"officiallyEndedTick":100,"reason":"","roster":{}})).unwrap();
        let event = |name:&str, tick, gun:&str, head:bool, victim_team:i32| GameEvent {name:name.into(),tick,fields:vec![
            EventField{name:"weapon".into(),data:Some(Variant::String(gun.into()))},
            EventField{name:"user_steamid".into(),data:Some(Variant::String(if name=="weapon_fire"{"a"}else{"v"}.into()))},
            EventField{name:"attacker_steamid".into(),data:Some(Variant::String("a".into()))},
            EventField{name:"attacker_team_num".into(),data:Some(Variant::I32(3))},
            EventField{name:"user_team_num".into(),data:Some(Variant::I32(victim_team))},
            EventField{name:"dmg_health".into(),data:Some(Variant::I32(20))},
            EventField{name:"hitgroup".into(),data:Some(Variant::String(if head{"head"}else{"chest"}.into()))},
        ]};
        let events=vec![event("player_hurt",10,"m4a1",true,2),event("weapon_fire",10,"weapon_m4a1_silencer",false,3),
            event("player_hurt",10,"m4a1",false,2),event("weapon_fire",20,"weapon_m4a1_silencer",false,3),
            event("weapon_fire",30,"weapon_m4a1_silencer",false,3),event("player_hurt",30,"m4a1",false,3),
            event("weapon_fire",70,"weapon_awp",false,3),event("player_hurt",70,"awp",true,2),
            event("weapon_fire",101,"weapon_ak47",false,3)];
        let out=compute(&events,&[round],64.0);
        let s=&out["a"]["all"];
        assert_eq!((s.shots,s.hits,s.head_hits,s.head_eligible_hits),(4,2,1,1));
        assert_eq!((s.first_shots,s.first_hits,s.spray_shots,s.spray_hits),(2,2,3,1));
        assert_eq!(out["a"]["awp"].head_eligible_hits,0);
        assert_eq!(out["a"]["rifles"].shots,3);
    }
}

/// Chart coordinates, also used by the bundled angular calibration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoilPoint {
    pub x: f64,
    pub y: f64,
    pub samples: u32,
}

/// Eye angles describe aiming input, while firing angles also contain weapon recoil.
/// Fire-event origins include movement and eye height during duck transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoilShot {
    pub tick: i32,
    pub origin: [f64; 3],
    pub view_pitch: f64,
    pub view_yaw: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoilBurst {
    pub round: i32,
    pub start_tick: i32,
    pub shots: Vec<RecoilShot>,
}

pub fn recoil(events: &[GameEvent], rounds: &[RoundInfo], tick_rate: f64) -> BTreeMap<String, BTreeMap<String, Vec<RecoilBurst>>> {
    struct Shot { tick: i32, round: i32, weapon: String, ray: Option<RecoilShot>, index: Option<f64> }
    let mut players: BTreeMap<String, Vec<Shot>> = BTreeMap::new();
    for event in events.iter().filter(|e| e.name == "fire_bullets") {
        let f = Fields(event);
        if f.bool("is_freeze_period") { continue; }
        let Some(round) = rounds.iter().find(|r| f.tick() >= r.freeze_end_tick && f.tick() <= r.officially_ended_tick) else { continue };
        let id = f.str("user_steamid");
        if id.is_empty() { continue; }
        let gun = match f.int("item_def_index") { 7 => "ak47", 16 => "m4a1", 60 => "m4a1_silencer", _ => "" };
        let ray = (|| {
            let origin = [f.opt_num("origin_x")?, f.opt_num("origin_y")?, f.opt_num("origin_z")?];
            let (pitch, yaw) = (f.opt_num("user_pitch")?, f.opt_num("user_yaw")?);
            if !origin.iter().all(|n| n.is_finite()) || !pitch.is_finite() || pitch.abs() > 90.0 || !yaw.is_finite() || yaw.abs() > 360.0 { return None; }
            Some(RecoilShot { tick: f.tick(), origin, view_pitch: pitch, view_yaw: yaw })
        })();
        // Retain invalid samples and other guns as boundaries; never bridge them.
        players.entry(id).or_default().push(Shot { tick: f.tick(), round: round.round, weapon: gun.into(), ray,
            index: f.opt_num("recoil_index").filter(|n| n.is_finite() && *n >= 0.0) });
    }
    let mut out = BTreeMap::new();
    for (id, mut shots) in players {
        shots.sort_by_key(|s| s.tick);
        let mut weapons: BTreeMap<String, Vec<RecoilBurst>> = BTreeMap::new();
        let mut start = 0;
        while start < shots.len() {
            let first = &shots[start];
            if first.weapon.is_empty() || first.ray.is_none() || !first.index.is_some_and(|n| n < 0.01) {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < shots.len() {
                let (prev, next) = (&shots[end - 1], &shots[end]);
                if next.round != first.round || next.weapon != first.weapon || next.ray.is_none()
                    || next.tick <= prev.tick || (next.tick - prev.tick) as f64 > tick_rate * 0.3
                    || !next.index.zip(prev.index).is_some_and(|(n, p)| (n - p - 1.0).abs() < 0.05) { break; }
                end += 1;
            }
            if end - start >= 3 {
                weapons.entry(first.weapon.clone()).or_default().push(RecoilBurst {
                    round: first.round, start_tick: first.tick,
                    shots: shots[start..end].iter().map(|s| s.ray.clone().expect("validated burst sample")).collect(),
                });
            }
            start = end;
        }
        if !weapons.is_empty() { out.insert(id, weapons); }
    }
    out
}

#[cfg(test)]
mod recoil_tests {
    use super::*;
    use parser::second_pass::{game_events::EventField, variants::Variant};

    #[test]
    fn preserves_individual_rays_and_breaks_on_missing_samples_weapons_rounds_and_recovery() {
        let round: RoundInfo = serde_json::from_value(serde_json::json!({"round":1,"startTick":0,"freezeEndTick":10,"endTick":400,"officiallyEndedTick":410,"reason":"","roster":{}})).unwrap();
        let shot = |tick, index: f32, yaw: Option<f32>, height: f32, gun| GameEvent { name: "fire_bullets".into(), tick, fields: vec![
            EventField { name:"item_def_index".into(), data:Some(Variant::U32(gun)) },
            EventField { name:"user_steamid".into(), data:Some(Variant::String("p".into())) },
            EventField { name:"angles_y".into(), data:Some(Variant::F32(77.)) },
            EventField { name:"user_yaw".into(), data:yaw.map(Variant::F32) },
            EventField { name:"user_pitch".into(), data:Some(Variant::F32(index)) },
            EventField { name:"angles_x".into(), data:Some(Variant::F32(-index)) },
            EventField { name:"origin_x".into(), data:Some(Variant::F32(index * 10.)) },
            EventField { name:"origin_y".into(), data:Some(Variant::F32(index * 20.)) },
            EventField { name:"origin_z".into(), data:Some(Variant::F32(height)) },
            EventField { name:"recoil_index".into(), data:Some(Variant::F32(index)) },
        ] };
        let mut events = vec![
            shot(10,0.,Some(179.),64.,7), shot(16,1.,Some(-179.),54.,7), shot(22,2.,Some(-177.),46.,7), shot(28,3.,Some(-175.),46.,7),
            shot(60,0.,Some(0.),64.,7), shot(66,1.,Some(4.),64.,7), shot(72,2.,Some(8.),64.,7),
            shot(100,0.,Some(0.),64.,7), shot(106,1.,None,64.,7), shot(112,2.,Some(4.),64.,7), shot(118,3.,Some(6.),64.,7),
            shot(140,0.5,Some(0.),64.,7), shot(146,1.5,Some(1.),64.,7), shot(152,2.5,Some(2.),64.,7),
            shot(180,0.,Some(0.),64.,16), shot(186,1.,Some(2.),64.,60), shot(192,2.,Some(4.),64.,60),
            shot(220,0.,Some(0.),64.,60), shot(226,1.,Some(1.),64.,60), shot(232,2.,Some(2.),64.,60),
            shot(270,0.,Some(0.),64.,7), shot(290,1.,Some(1.),64.,7), shot(296,2.,Some(2.),64.,7),
            shot(401,0.,Some(0.),64.,7), shot(407,1.,Some(1.),64.,7), shot(413,2.,Some(2.),64.,7),
            shot(310,0.,Some(0.),64.,7), shot(316,1.,Some(1.),f32::NAN,7), shot(322,2.,Some(2.),64.,7),
            shot(340,0.,Some(0.),64.,7), shot(346,2.,Some(1.),64.,7), shot(352,3.,Some(2.),64.,7),
        ];
        events.reverse();
        let result = recoil(&events, &[round], 64.0);
        let bursts = &result["p"]["ak47"];
        assert_eq!(bursts.iter().map(|b| (b.round,b.start_tick,b.shots.len())).collect::<Vec<_>>(), vec![(1,10,4),(1,60,3)]);
        assert_eq!(bursts[0].shots[2].origin, [20.,40.,46.]);
        assert_eq!((bursts[0].shots[2].view_pitch,bursts[0].shots[2].view_yaw),(2.,-177.));
        assert_eq!(result["p"].len(),2);
        assert_eq!(result["p"]["m4a1_silencer"][0].shots.len(),3);
    }
}
