//! Match-wide shot ledger statistics. No ratings or player verdicts.
use super::{Check, Definition, Finding, Measurement, State};
use crate::model::RoundInfo;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn text<'a>(event: &'a Value, field: &str) -> &'a str {
    event[field].as_str().unwrap_or("")
}
fn gun(name: &str) -> &str {
    match name.strip_prefix("weapon_").unwrap_or(name) {
        "usp_silencer" => "hkp2000",
        "m4a1_silencer" => "m4a1",
        other => other,
    }
}
fn metric(name: &str, value: f64, unit: &str) -> Measurement {
    Measurement {
        name: name.into(),
        value,
        unit: unit.into(),
        threshold: None,
    }
}
fn check(id: &str, category: &str) -> Check {
    Check {
        definition: Definition {
            id: id.into(),
            version: "2-counts-weapon-samples".into(),
            name: id.into(),
            description: "Observed match behavior; not a cheating verdict.".into(),
            category: category.into(),
            parameters: match id {
                "shot-hit-rate" => json!({"minimumWeaponShots":10,"highRatePercent":85}),
                "first-shot-hit-rate" => {
                    json!({"minimumFirstShots":10,"highRatePercent":85,"burstGapSeconds":0.3,"perWeapon":true})
                }
                "unbroken-hit-sequence" => json!({"minimumHitStreak":10}),
                "rapid-multikill" => json!({"twoKillsSeconds":1.5,"threeOrMoreKillsSeconds":3}),
                "smoke-hit-rate" | "penetration-hit-rate" => {
                    json!({"source":"player_bullet_hit","maximumShotDelayTicks":1,"requiresUniqueShot":true,"denominatorAvailable":false,"unclassifiedShots":"shotsWithoutConfirmedContextHit"})
                }
                _ => json!({"recordedBlindWindow":true}),
            },
        },
        state: State::Unavailable,
        reason: "No eligible firearm shots.".into(),
        reason_code: "noEligibleShots".into(),
        evaluated_samples: 0,
        findings: vec![],
        observations: vec![],
        occurrences: vec![],
        summary: vec![],
        diagnostics: Value::Null,
    }
}
fn finding(
    id: String,
    group: &str,
    round: i32,
    start: i32,
    end: i32,
    target: String,
    measurements: Vec<Measurement>,
) -> Finding {
    Finding {
        id,
        group: group.into(),
        round,
        start_tick: start,
        end_tick: end,
        target_id: target,
        reason: "Recorded behavior met the displayed criteria.".into(),
        measurements,
    }
}
#[derive(Clone)]
struct Shot {
    tick: i32,
    round: i32,
    weapon: String,
    hit: bool,
    target: String,
    blind: Option<f64>,
    smoke_target: Option<String>,
    penetration_target: Option<String>,
}

pub fn evaluate_match(
    events: &[Value],
    players: &[String],
    rounds: &[RoundInfo],
    rate: f64,
) -> BTreeMap<String, Vec<Check>> {
    if !rate.is_finite() || rate <= 0. {
        return players
            .iter()
            .map(|player| {
                (
                    player.clone(),
                    vec![
                        check("shot-hit-rate", "accuracy"),
                        check("first-shot-hit-rate", "accuracy"),
                        check("unbroken-hit-sequence", "accuracy"),
                        check("rapid-multikill", "combat"),
                        check("flashed-hit-rate", "context"),
                        check("smoke-hit-rate", "context"),
                        check("penetration-hit-rate", "context"),
                    ],
                )
            })
            .collect();
    }
    let round_of = |tick: i32| {
        rounds
            .iter()
            .find(|r| tick >= r.freeze_end_tick && tick <= r.end_tick)
    };
    let mut shots: BTreeMap<String, BTreeMap<(i32, String), Shot>> = players
        .iter()
        .map(|p| (p.clone(), BTreeMap::new()))
        .collect();
    let mut hurts = vec![];
    let mut deaths = vec![];
    let mut blinds = vec![];
    let mut bullet_hits = vec![];
    for event in events {
        let Some(tick) = event["tick"].as_i64().and_then(|n| i32::try_from(n).ok()) else {
            continue;
        };
        let Some(round) = round_of(tick) else {
            continue;
        };
        match text(event, "event_name") {
            "weapon_fire" => {
                let player = text(event, "user_steamid");
                let weapon = text(event, "weapon")
                    .strip_prefix("weapon_")
                    .unwrap_or(text(event, "weapon"));
                if crate::aim::group(weapon).is_none() || !round.roster.contains_key(player) {
                    continue;
                }
                if let Some(ledger) = shots.get_mut(player) {
                    ledger.entry((tick, gun(weapon).into())).or_insert(Shot {
                        tick,
                        round: round.round,
                        weapon: weapon.into(),
                        hit: false,
                        target: String::new(),
                        blind: None,
                        smoke_target: None,
                        penetration_target: None,
                    });
                }
            }
            "player_hurt" => hurts.push((tick, round, event)),
            "player_bullet_hit" => bullet_hits.push((tick, round, event)),
            "player_death" => deaths.push((tick, round, event)),
            "player_blind" => blinds.push((tick, round, event)),
            _ => {}
        }
    }
    for (tick, round, event) in hurts {
        let attacker = text(event, "attacker_steamid");
        let victim = text(event, "user_steamid");
        if attacker == victim || event["dmg_health"].as_f64().unwrap_or(0.) <= 0. {
            continue;
        }
        let (Some(a), Some(v)) = (round.roster.get(attacker), round.roster.get(victim)) else {
            continue;
        };
        if a == v {
            continue;
        }
        let Some(ledger) = shots.get_mut(attacker) else {
            continue;
        };
        for shot_tick in [tick, tick - 1] {
            if let Some(shot) = ledger.get_mut(&(shot_tick, gun(text(event, "weapon")).into())) {
                if shot.round != round.round {
                    continue;
                }
                shot.hit = true;
                if shot.target.is_empty() {
                    shot.target = victim.into();
                }
                break;
            }
        }
    }
    for (tick, round, event) in bullet_hits {
        let attacker = text(event, "attacker_steamid");
        let victim = text(event, "user_steamid");
        let (Some(a), Some(v)) = (round.roster.get(attacker), round.roster.get(victim)) else {
            continue;
        };
        if attacker == victim || a == v {
            continue;
        }
        let smoke = event["through_smoke"].as_bool() == Some(true);
        let penetration = event["penetration_count"].as_i64().is_some_and(|n| n > 0);
        if !smoke && !penetration {
            continue;
        }
        let Some(ledger) = shots.get_mut(attacker) else {
            continue;
        };
        for shot_tick in [tick, tick - 1] {
            let mut candidates = ledger
                .range((shot_tick, String::new())..)
                .take_while(|((t, _), _)| *t == shot_tick)
                .filter(|(_, shot)| shot.round == round.round);
            let Some((key, _)) = candidates.next() else {
                continue;
            };
            let key = key.clone();
            // This event has no weapon. Multiple weapons at one tick cannot be
            // disambiguated, and falling back would attach it to another shot.
            if candidates.next().is_some() {
                break;
            }
            let shot = ledger.get_mut(&key).unwrap();
            if smoke && shot.smoke_target.is_none() {
                shot.smoke_target = Some(victim.into());
            }
            if penetration && shot.penetration_target.is_none() {
                shot.penetration_target = Some(victim.into());
            }
            break;
        }
    }
    let mut blind_windows = BTreeMap::<(String, i32), Vec<(i32, f64)>>::new();
    if rate.is_finite() && rate > 0. {
        for (tick, round, event) in blinds {
            let duration = event["blind_duration"].as_f64().unwrap_or(0.);
            if !duration.is_finite() || duration <= 0. {
                continue;
            }
            let player = text(event, "user_steamid");
            let death = deaths
                .iter()
                .filter(|(t, r, e)| {
                    *t >= tick && r.round == round.round && text(e, "user_steamid") == player
                })
                .map(|(t, _, _)| f64::from(*t))
                .min_by(f64::total_cmp)
                .unwrap_or(f64::from(round.end_tick));
            blind_windows
                .entry((player.into(), round.round))
                .or_default()
                .push((tick, (f64::from(tick) + duration * rate).min(death)));
        }
    }
    let mut kills = BTreeMap::<String, Vec<(i32, i32, String)>>::new();
    for (tick, round, event) in deaths {
        let attacker = text(event, "attacker_steamid");
        let victim = text(event, "user_steamid");
        let (Some(a), Some(v)) = (round.roster.get(attacker), round.roster.get(victim)) else {
            continue;
        };
        if attacker == victim
            || a == v
            || crate::aim::group(
                text(event, "weapon")
                    .strip_prefix("weapon_")
                    .unwrap_or(text(event, "weapon")),
            )
            .is_none()
        {
            continue;
        }
        kills
            .entry(attacker.into())
            .or_default()
            .push((tick, round.round, victim.into()));
    }
    shots.into_iter().map(|(player,ledger)| {
        let mut rows:Vec<_>=ledger.into_values().collect();
        for shot in &mut rows {
            shot.blind=blind_windows.get(&(player.clone(),shot.round)).into_iter().flatten()
                .filter(|(start,end)|shot.tick>=*start && f64::from(shot.tick)<*end)
                .map(|(_,end)|(end-f64::from(shot.tick))/rate).max_by(f64::total_cmp);
        }
        let mut checks=vec![check("shot-hit-rate","accuracy"),check("first-shot-hit-rate","accuracy"),
            check("unbroken-hit-sequence","accuracy"),check("rapid-multikill","combat"),check("flashed-hit-rate","context"),check("smoke-hit-rate","context"),check("penetration-hit-rate","context")];
        let hits=rows.iter().filter(|s|s.hit).count();
        let summary=vec![metric("shots",rows.len() as f64,"shots"),metric("hits",hits as f64,"shots")];
        for c in &mut checks {c.evaluated_samples=rows.len();c.summary=summary.clone();}
        if !rows.is_empty() {
            for c in &mut checks {
                c.state=State::Passed;c.reason_code="measuredBehavior".into();c.reason="Observed counts, without a player verdict.".into();
                c.summary.push(metric("hitRate",100.*hits as f64/rows.len() as f64,"%"));
            }
        }
        let mut weapons=BTreeMap::<&str,(usize,usize)>::new();
        for s in &rows {let n=weapons.entry(&s.weapon).or_default();n.0+=1;n.1+=usize::from(s.hit);}
        let first:Vec<_>=rows.iter().enumerate().filter(|(i,s)|*i==0 || rows[*i-1].round!=s.round
            || rows[*i-1].weapon!=s.weapon || f64::from(s.tick-rows[*i-1].tick)>rate*0.3).map(|(_,s)|s).collect();
        checks[1].evaluated_samples=first.len();
        let first_hits=first.iter().filter(|s|s.hit).count();
        checks[1].summary=vec![metric("firstShots",first.len() as f64,"shots"),metric("firstHits",first_hits as f64,"shots")];
        if !first.is_empty() {checks[1].summary.push(metric("firstHitRate",100.*first_hits as f64/first.len() as f64,"%"));}
        for s in &rows {
            let (n,h)=weapons[s.weapon.as_str()];
            if s.hit && n>=10 && h as f64/n as f64>=0.85 {
                checks[0].findings.push(finding(format!("shot-{}",s.tick),"accuracy",s.round,s.tick,s.tick,s.target.clone(),
                    vec![metric("shots",n as f64,"shots"),metric("hits",h as f64,"shots"),metric("hitRate",100.*h as f64/n as f64,"%")]));
            }
        }
        let mut first_weapons=BTreeMap::<&str,(usize,usize)>::new();
        for s in &first {let counts=first_weapons.entry(&s.weapon).or_default();counts.0+=1;counts.1+=usize::from(s.hit);}
        for s in first.iter().filter(|s|s.hit) {
            let (n,h)=first_weapons[s.weapon.as_str()];
            if n>=10 && h as f64/n as f64>=0.85 {
                checks[1].findings.push(finding(format!("first-{}-{}",s.tick,s.weapon),"accuracy",s.round,s.tick,s.tick,s.target.clone(),
                    vec![metric("firstShots",n as f64,"shots"),metric("firstHits",h as f64,"shots"),metric("firstHitRate",100.*h as f64/n as f64,"%")]));
            }
        }
        let mut start=0;
        while start<rows.len() {
            if !rows[start].hit {start+=1;continue}
            let mut end=start+1;
            while end<rows.len() && rows[end].hit {end+=1}
            if end-start>=10 {
                checks[2].findings.push(finding(format!("streak-{}",rows[start].tick),"accuracy",rows[start].round,
                    rows[start].tick,rows[end-1].tick,String::new(),vec![metric("streakLength",(end-start) as f64,"shots"),
                        metric("lastRound",f64::from(rows[end-1].round),"round")]));
            }
            start=end;
        }
        let mut ks=kills.remove(&player).unwrap_or_default();ks.sort();ks.dedup();
        checks[3].evaluated_samples=ks.len();
        checks[3].summary=vec![metric("killCount",ks.len() as f64,"kills")];
        let mut i=0;
        while i<ks.len() {
            let mut end=i+1;
            while end<ks.len() && ks[end].1==ks[i].1 && f64::from(ks[end].0-ks[i].0)<=rate*3. {end+=1}
            if end-i>=3 || end-i==2 && f64::from(ks[end-1].0-ks[i].0)<=rate*1.5 {
                checks[3].findings.push(finding(format!("kills-{}",ks[i].0),"combat",ks[i].1,ks[i].0,ks[end-1].0,String::new(),
                    vec![metric("killCount",(end-i) as f64,"kills"),metric("intervalSeconds",f64::from(ks[end-1].0-ks[i].0)/rate,"s")]));
                i=end;
            } else {i+=1}
        }
        let blind:Vec<_>=rows.iter().filter(|s|s.blind.is_some()).collect();
        let blind_hits=blind.iter().filter(|s|s.hit).count();
        checks[4].evaluated_samples=blind.len();
        checks[4].summary=vec![metric("blindShots",blind.len() as f64,"shots"),metric("blindHits",blind_hits as f64,"shots")];
        if blind.is_empty() {checks[4].state=State::Unavailable;checks[4].reason_code="noEligibleShots".into();}
        else {
            checks[4].summary.push(metric("blindHitRate",100.*blind_hits as f64/blind.len() as f64,"%"));
            for s in blind.into_iter().filter(|s|s.hit) {
                checks[4].findings.push(finding(format!("blind-{}",s.tick),"context",s.round,s.tick,s.tick,s.target.clone(),
                    vec![metric("blindRemainingSeconds",s.blind.unwrap_or(0.),"s")]));
            }
        }
        for (index, is_smoke, metric_name) in [(5,true,"smokeHits"),(6,false,"penetrationHits")] {
            let c=&mut checks[index];
            c.findings=rows.iter().filter_map(|shot| {
                let target=if is_smoke {shot.smoke_target.as_ref()} else {shot.penetration_target.as_ref()}?;
                Some(finding(format!("{}-{}-{}",c.definition.id,shot.tick,shot.weapon),
                    &c.definition.id,shot.round,shot.tick,shot.tick,target.clone(),
                    vec![metric(metric_name,1.,"shots")]))
            }).collect();
            let confirmed=c.findings.len();
            c.evaluated_samples=confirmed;
            c.summary=vec![metric(metric_name,confirmed as f64,"shots"),
                metric("unclassifiedShots",(rows.len()-confirmed) as f64,"shots")];
            c.state=if confirmed>0 {State::Findings} else {State::Unavailable};
            c.reason_code="shotPathsMissing".into();
            c.reason="Recorded enemy-hit shots are counted; missing missed-shot paths leave the context hit-rate denominator incomplete.".into();
        }
        for c in &mut checks {if !c.findings.is_empty(){c.state=State::Findings;}}
        (player,checks)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Team;
    #[test]
    fn context_hits_deduplicate_shots_without_inventing_miss_denominators() {
        let round = RoundInfo {
            round: 1,
            start_tick: 0,
            freeze_end_tick: 10,
            end_tick: 400,
            officially_ended_tick: 420,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::from([
                ("a".into(), Team::Ct),
                ("b".into(), Team::T),
                ("c".into(), Team::T),
                ("friend".into(), Team::Ct),
            ]),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        };
        let mut events = vec![];
        for (tick, weapon) in [
            (20, "ak47"),
            (30, "ak47"),
            (40, "ak47"),
            (40, "deagle"),
            (50, "ak47"),
        ] {
            events.push(
                json!({"event_name":"weapon_fire","tick":tick,"user_steamid":"a","weapon":weapon}),
            );
        }
        for (tick, victim, smoke, penetration) in [
            (20, "b", true, 1),
            (20, "c", true, 2),
            (31, "b", false, 1),
            (40, "b", true, 1),
            (50, "friend", true, 1),
            (50, "a", true, 1),
        ] {
            events.push(
                json!({"event_name":"player_bullet_hit","tick":tick,"attacker_steamid":"a",
                "user_steamid":victim,"through_smoke":smoke,"penetration_count":penetration}),
            );
            events.push(
                json!({"event_name":"player_hurt","tick":tick,"attacker_steamid":"a",
                "user_steamid":victim,"weapon":"ak47","dmg_health":10}),
            );
        }
        // Duplicate impact/damage events and multiple victims must remain one shot.
        events.push(events[5].clone());
        let mut checks = evaluate_match(&events, &["a".into()], &[round], 64.)
            .remove("a")
            .unwrap();
        super::super::statistics::summarize(&mut checks);
        assert_eq!(checks[5].occurrences.len(), 1);
        assert_eq!(checks[6].occurrences.len(), 2);
        assert_eq!(
            checks[5]
                .summary
                .iter()
                .find(|m| m.name == "smokeHits")
                .unwrap()
                .value,
            1.
        );
        assert_eq!(
            checks[5]
                .summary
                .iter()
                .find(|m| m.name == "unclassifiedShots")
                .unwrap()
                .value,
            4.
        );
        assert_eq!(
            checks[6]
                .summary
                .iter()
                .find(|m| m.name == "unclassifiedShots")
                .unwrap()
                .value,
            3.
        );
        for c in &checks[5..] {
            assert_eq!(c.reason_code, "shotPathsMissing");
            assert!(!c
                .summary
                .iter()
                .any(|m| m.unit == "%" || m.name.to_lowercase().contains("rate")));
        }
    }
    #[test]
    fn counts_unique_shots_misses_and_flash_hits_without_scores() {
        let round = RoundInfo {
            round: 1,
            start_tick: 0,
            freeze_end_tick: 10,
            end_tick: 400,
            officially_ended_tick: 420,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::from([
                ("a".into(), Team::Ct),
                ("b".into(), Team::T),
                ("c".into(), Team::T),
                ("d".into(), Team::T),
            ]),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        };
        let mut events = vec![
            json!({"event_name":"player_blind","tick":20,"user_steamid":"a","blind_duration":1.0}),
        ];
        for i in 0..11 {
            let tick = 20 + 32 * i;
            events.push(json!({"event_name":"weapon_fire","tick":tick,"user_steamid":"a","weapon":"weapon_ak47"}));
            if i < 10 {
                let hurt = json!({"event_name":"player_hurt","tick":tick,"attacker_steamid":"a","user_steamid":"b","weapon":"ak47","dmg_health":10});
                events.extend([hurt.clone(), hurt]);
            }
        }
        events.push(events[1].clone());
        events.push(json!({"event_name":"player_hurt","tick":340,"attacker_steamid":"a","user_steamid":"a","weapon":"ak47","dmg_health":5}));
        for (tick, victim) in [(84, "b"), (100, "c"), (116, "d")] {
            events.push(json!({"event_name":"player_death","tick":tick,"attacker_steamid":"a","user_steamid":victim,"weapon":"ak47"}));
        }
        // A large denominator is real evidence; tiny or mixed-weapon samples cannot borrow it.
        for count in [1, 100] {
            let mut sample_round=round.clone();sample_round.end_tick=4000;
            let sample_events:Vec<_>=(0..count).flat_map(|i| {
                let tick=20+i*32;
                [json!({"event_name":"weapon_fire","tick":tick,"user_steamid":"a","weapon":"ak47"}),
                 json!({"event_name":"player_hurt","tick":tick,"attacker_steamid":"a","user_steamid":"b","weapon":"ak47","dmg_health":10})]
            }).collect();
            let sample=evaluate_match(&sample_events,&["a".into()],&[sample_round],64.);
            assert_eq!(sample["a"][0].evaluated_samples,count as usize);
            assert_eq!(sample["a"][0].findings.len(),if count==100 {100}else{0});
            assert_eq!(sample["a"][1].findings.len(),if count==100 {100}else{0});
        }
        let mixed:Vec<_>=(0..10).flat_map(|i| {
            let tick=20+i*32;let weapon=if i==9 {"awp"} else {"ak47"};
            [json!({"event_name":"weapon_fire","tick":tick,"user_steamid":"a","weapon":weapon}),
             json!({"event_name":"player_hurt","tick":tick,"attacker_steamid":"a","user_steamid":"b","weapon":weapon,"dmg_health":10})]
        }).collect();
        assert!(evaluate_match(&mixed,&["a".into()],&[round.clone()],64.)["a"][1].findings.is_empty());
        let mut results = evaluate_match(&events, &["a".into(), "b".into()], &[round], 64.);
        let mut checks = results.remove("a").unwrap();
        super::super::statistics::summarize(&mut checks);
        assert_eq!(
            checks
                .iter()
                .map(|c| c.occurrences.len())
                .collect::<Vec<_>>(),
            vec![10, 10, 1, 1, 2, 0, 0]
        );
        assert_eq!(
            checks[0]
                .summary
                .iter()
                .find(|m| m.name == "shots")
                .unwrap()
                .value,
            11.
        );
        assert_eq!(
            checks[0]
                .summary
                .iter()
                .find(|m| m.name == "hits")
                .unwrap()
                .value,
            10.
        );
        assert_eq!(
            checks[4]
                .summary
                .iter()
                .find(|m| m.name == "blindShots")
                .unwrap()
                .value,
            2.
        );
        let encoded = serde_json::to_string(&checks).unwrap();
        assert!(!encoded.contains("proposedPoints") && !encoded.contains("ruleCap"));
        assert!(results["b"].iter().all(|c| c.state == State::Unavailable));
    }
}
