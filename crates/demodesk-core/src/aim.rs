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

/// Mean view-angle movement, aligned to fresh rifle bursts within one demo.
/// This includes target tracking; it is not isolated mouse input or bullet impact.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoilPoint {
    pub x: f64,
    pub y: f64,
    pub samples: u32,
}

pub fn recoil(events: &[GameEvent], rounds: &[RoundInfo], tick_rate: f64) -> BTreeMap<String, BTreeMap<String, Vec<RecoilPoint>>> {
    struct Shot { tick: i32, round: i32, weapon: String, angles: Option<(f64, f64)>, index: Option<f64> }
    let mut players: BTreeMap<String, Vec<Shot>> = BTreeMap::new();
    for event in events.iter().filter(|e| e.name == "weapon_fire") {
        let f = Fields(event);
        if f.bool("is_freeze_period") { continue; }
        let Some(round) = rounds.iter().find(|r| f.tick() >= r.freeze_end_tick && f.tick() <= r.officially_ended_tick) else { continue };
        let id = f.str("user_steamid");
        if id.is_empty() { continue; }
        let gun = f.str("weapon");
        let angles = f.opt_num("user_yaw").zip(f.opt_num("user_pitch"))
            .filter(|(yaw, pitch)| yaw.is_finite() && yaw.abs() <= 360.0 && pitch.is_finite() && pitch.abs() <= 90.0);
        // Retain missing samples and other guns as boundaries; never bridge them.
        players.entry(id).or_default().push(Shot { tick: f.tick(), round: round.round, weapon: weapon(&gun).into(), angles,
            index: f.opt_num("user_fl_recoil_idx").filter(|n| n.is_finite() && *n >= 0.0) });
    }
    let mut out = BTreeMap::new();
    for (id, mut shots) in players {
        shots.sort_by_key(|s| s.tick);
        let mut weapons: BTreeMap<String, Vec<RecoilPoint>> = BTreeMap::new();
        let mut start = 0;
        while start < shots.len() {
            let first = &shots[start];
            if !matches!(first.weapon.as_str(), "ak47" | "m4a1" | "m4a1_silencer") || first.angles.is_none() || !first.index.is_some_and(|n| n < 0.01) {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < shots.len() {
                let (prev, next) = (&shots[end - 1], &shots[end]);
                if next.round != first.round || next.weapon != first.weapon || next.angles.is_none()
                    || next.tick <= prev.tick || (next.tick - prev.tick) as f64 > tick_rate * 0.3
                    || !next.index.zip(prev.index).is_some_and(|(n, p)| (n - p - 1.0).abs() < 0.05) { break; }
                end += 1;
            }
            if end - start >= 3 {
                let points = weapons.entry(first.weapon.clone()).or_default();
                let (mut previous_yaw, first_pitch) = first.angles.expect("validated first shot");
                let mut yaw_delta = 0.0;
                for (i, shot) in shots[start..end].iter().enumerate() {
                    let (yaw, pitch) = shot.angles.expect("validated burst sample");
                    yaw_delta += (yaw - previous_yaw + 180.0).rem_euclid(360.0) - 180.0;
                    previous_yaw = yaw;
                    if points.len() <= i { points.push(RecoilPoint::default()); }
                    let point = &mut points[i];
                    point.samples += 1;
                    // Positive X = right, positive Y = up; CS pitch increases downwards.
                    point.x += (-yaw_delta - point.x) / f64::from(point.samples);
                    point.y += (first_pitch - pitch - point.y) / f64::from(point.samples);
                }
            }
            start = end;
        }
        if !weapons.is_empty() { out.insert(id, weapons); }
    }
    out
}

/// Empirical compensation reference from this demo, not a version-independent ideal.
/// FireBullets supplies the firing angle before random spread. Its difference from
/// the same-tick eye angle estimates recoil; subtick aim changes remain measurement noise.
pub fn recoil_reference(events: &[GameEvent], rounds: &[RoundInfo], tick_rate: f64) -> BTreeMap<String, Vec<RecoilPoint>> {
    use parser::second_pass::variants::Variant;
    let mut bullets = HashMap::new();
    for event in events.iter().filter(|e| e.name == "fire_bullets") {
        let key = (event.tick, Fields(event).str("user_steamid"));
        // Ambiguous pairs must not silently pick whichever event came last.
        bullets.entry(key).and_modify(|v| *v = None).or_insert(Some(event));
    }
    let samples: Vec<_> = events.iter().filter(|e| e.name == "weapon_fire").map(|event| {
        let f = Fields(event);
        let compensation = (|| {
            let b = Fields(*bullets.get(&(event.tick, f.str("user_steamid")))?.as_ref()?);
            let expected = match weapon(&f.str("weapon")) { "ak47" => 7, "m4a1" => 16, "m4a1_silencer" => 60, _ => return None };
            if b.int("item_def_index") != expected || (b.opt_num("recoil_index")? - f.opt_num("user_fl_recoil_idx")?).abs() > 0.01 { return None; }
            let pitch = b.opt_num("angles_x")? - f.opt_num("user_pitch")?;
            let yaw = (b.opt_num("angles_y")? - f.opt_num("user_yaw")? + 180.0).rem_euclid(360.0) - 180.0;
            if !pitch.is_finite() || !yaw.is_finite() || pitch.abs() > 90.0 { return None; }
            Some((pitch, yaw))
        })();
        let mut sample = event.clone();
        // recoil() converts eye angles to screen axes and aligns each fresh burst.
        for field in &mut sample.fields {
            if field.name == "user_pitch" { field.data = compensation.map(|(p, _)| Variant::F32(-p as f32)); }
            if field.name == "user_yaw" { field.data = compensation.map(|(_, y)| Variant::F32(-y as f32)); }
        }
        sample
    }).collect();
    let mut reference: BTreeMap<String, Vec<RecoilPoint>> = BTreeMap::new();
    for weapons in recoil(&samples, rounds, tick_rate).into_values() {
        for (gun, points) in weapons {
            let pooled = reference.entry(gun).or_default();
            for (i, p) in points.into_iter().enumerate() {
                if pooled.len() <= i { pooled.push(RecoilPoint::default()); }
                let mean = &mut pooled[i];
                mean.samples += p.samples;
                let weight = f64::from(p.samples) / f64::from(mean.samples);
                mean.x += (p.x - mean.x) * weight;
                mean.y += (p.y - mean.y) * weight;
            }
        }
    }
    reference
}

#[cfg(test)]
mod recoil_tests {
    use super::*;
    use parser::second_pass::{game_events::EventField, variants::Variant};

    #[test]
    fn aligns_fresh_bursts_unwraps_yaw_and_keeps_late_shot_sample_counts() {
        let round: RoundInfo = serde_json::from_value(serde_json::json!({"round":1,"startTick":0,"freezeEndTick":10,"endTick":400,"officiallyEndedTick":410,"reason":"","roster":{}})).unwrap();
        let shot = |tick, index: f32, yaw: Option<f32>, pitch: f32, gun: &str| GameEvent { name: "weapon_fire".into(), tick, fields: vec![
            EventField { name:"weapon".into(), data:Some(Variant::String(gun.into())) },
            EventField { name:"user_steamid".into(), data:Some(Variant::String("p".into())) },
            EventField { name:"user_yaw".into(), data:yaw.map(Variant::F32) },
            EventField { name:"user_pitch".into(), data:Some(Variant::F32(pitch)) },
            EventField { name:"user_fl_recoil_idx".into(), data:Some(Variant::F32(index)) },
        ] };
        let mut events = vec![
            shot(10,0.,Some(179.),1.,"weapon_ak47"), shot(16,1.,Some(-179.),3.,"weapon_ak47"), shot(22,2.,Some(-177.),5.,"weapon_ak47"), shot(28,3.,Some(-175.),7.,"weapon_ak47"),
            shot(60,0.,Some(0.),10.,"ak47"), shot(66,1.,Some(4.),14.,"ak47"), shot(72,2.,Some(8.),18.,"ak47"),
            // Missing intermediate angle must not be bridged into a three-shot burst.
            shot(100,0.,Some(0.),0.,"ak47"), shot(106,1.,None,0.,"ak47"), shot(112,2.,Some(4.),4.,"ak47"), shot(118,3.,Some(6.),6.,"ak47"),
            // Unrecovered and interrupted bursts are excluded, as are other weapons.
            shot(140,0.5,Some(0.),0.,"ak47"), shot(146,1.5,Some(1.),1.,"ak47"), shot(152,2.5,Some(2.),2.,"ak47"),
            shot(180,0.,Some(0.),0.,"m4a1"), shot(186,1.,Some(2.),1.,"m4a1_silencer"), shot(192,2.,Some(4.),2.,"m4a1_silencer"),
            shot(220,0.,Some(0.),0.,"m4a1_silencer"), shot(226,1.,Some(1.),2.,"m4a1_silencer"), shot(232,2.,Some(2.),4.,"m4a1_silencer"),
            shot(270,0.,Some(0.),0.,"ak47"), shot(290,1.,Some(1.),1.,"ak47"), shot(296,2.,Some(2.),2.,"ak47"),
            shot(401,0.,Some(0.),0.,"ak47"), shot(407,1.,Some(1.),1.,"ak47"), shot(413,2.,Some(2.),2.,"ak47"),
        ];
        let mut with_bullets = events.clone();
        for e in &events {
            let f = Fields(e);
            let Some(yaw) = f.opt_num("user_yaw") else { continue };
            let index = f.num("user_fl_recoil_idx");
            let item = match weapon(&f.str("weapon")) { "ak47" => 7, "m4a1" => 16, _ => 60 };
            let mut b = e.clone();
            b.name = "fire_bullets".into();
            for (name, value) in [("angles_x", f.num("user_pitch") - index * 1.5), ("angles_y", yaw + index * 0.5), ("item_def_index", item as f64), ("recoil_index", index)] {
                b.fields.push(EventField { name: name.into(), data: Some(Variant::F32(value as f32)) });
            }
            with_bullets.push(b);
        }
        with_bullets.reverse();
        let reference = recoil_reference(&with_bullets, std::slice::from_ref(&round), 64.0);
        let expected = &reference["ak47"];
        assert_eq!(expected.iter().map(|p| p.samples).collect::<Vec<_>>(), vec![2,2,2,1]);
        assert_eq!(expected.iter().map(|p| (p.x,p.y)).collect::<Vec<_>>(), vec![(0.,0.),(0.5,-1.5),(1.,-3.),(1.5,-4.5)]);
        // Pool by burst counts, not equal weights for each player's mean.
        let mut pooled_events = with_bullets.clone();
        for event in with_bullets.iter().filter(|e| [10,16,22].contains(&e.tick)) {
            let index = Fields(event).num("user_fl_recoil_idx") as f32;
            let mut other = event.clone();
            for field in &mut other.fields {
                if field.name == "user_steamid" { field.data = Some(Variant::String("q".into())); }
                if let Some(Variant::F32(value)) = &mut field.data {
                    if field.name == "angles_x" { *value -= index * 1.5; }
                    if field.name == "angles_y" { *value += index * 0.5; }
                }
            }
            pooled_events.push(other);
        }
        let pooled = recoil_reference(&pooled_events, std::slice::from_ref(&round), 64.0);
        assert_eq!(pooled["ak47"][1].samples, 3);
        assert!((pooled["ak47"][1].x - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(pooled["ak47"][1].y, -2.0);
        // Missing or duplicate pairs cannot manufacture a reference or bridge gaps.
        with_bullets.retain(|e| !(e.name == "fire_bullets" && e.tick == 66));
        let duplicate = with_bullets.iter().find(|e| e.name == "fire_bullets" && e.tick == 16).unwrap().clone();
        with_bullets.push(duplicate);
        assert!(!recoil_reference(&with_bullets, std::slice::from_ref(&round), 64.0).contains_key("ak47"));
        assert!(recoil_reference(&events, std::slice::from_ref(&round), 64.0).is_empty());
        events.reverse(); // Message order is not assumed.
        let result = recoil(&events, &[round], 64.0);
        let points = &result["p"]["ak47"];
        assert_eq!(points.iter().map(|p|p.samples).collect::<Vec<_>>(), vec![2,2,2,1]);
        assert_eq!(points.iter().map(|p|(p.x,p.y)).collect::<Vec<_>>(), vec![(0.,0.),(-3.,-3.),(-6.,-6.),(-6.,-6.)]);
        assert_eq!(result["p"].len(),2);
        assert_eq!(result["p"]["m4a1_silencer"][2].y,-4.);
    }
}
