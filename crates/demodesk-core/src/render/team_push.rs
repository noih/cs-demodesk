//! Conservative nearby attacking-team pushes. Independent cameras never replace main combat.
use super::*;
use crate::analysis::line_of_sight::{Occlusion, Vec3};
use camera_search::{Candidate, Sample};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Player {
    id: String,
    team: i32,
    alive: bool,
    position: Vec3,
}
type Tracks = BTreeMap<i32, Vec<Player>>;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Shot {
    start_tick: i32,
    end_tick: i32,
    camera: Candidate,
    samples: Vec<Sample>,
    transition_safe: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Sequence {
    id: String,
    steam_id: String,
    tick_rate: f64,
    shots: Vec<Shot>,
}
impl Sequence {
    pub(super) fn id(&self) -> &str {
        &self.id
    }
}
pub(super) fn applies(
    plan: &Sequence,
    recording: &[Highlight],
    utilities: &[Option<utility::Chase>],
) -> bool {
    recording.iter().enumerate().any(|(index, h)| {
        eligible(index, recording, utilities, plan)
            && plan.shots.iter().any(|s| {
                s.end_tick.min(h.end_tick) - (s.start_tick.max(h.start_tick))
                    >= (plan.tick_rate / 2.) as i32
            })
    })
}
fn eligible(
    index: usize,
    recording: &[Highlight],
    utilities: &[Option<utility::Chase>],
    plan: &Sequence,
) -> bool {
    let h = &recording[index];
    h.id == plan.id
        && h.player.steamid == plan.steam_id
        && utilities[index].is_none()
        && !(0..index).any(|i| {
            recording[i].id == h.id
                && (recording[i].player.steamid != plan.steam_id || utilities[i].is_some())
        })
}
fn distance(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
fn member<'a>(players: &'a [Player], id: &str) -> Option<&'a Player> {
    players.iter().find(|p| p.id == id)
}
fn active(e: &utility::Event, id: &str) -> bool {
    matches!(e.event_name.as_str(), "player_hurt" | "player_death")
        && (e.user_steamid.as_deref() == Some(id) || e.attacker_steamid.as_deref() == Some(id))
}
fn main_owns(events: &[utility::Event], main: &str, start: i32, end: i32, rate: i32) -> bool {
    events.iter().any(|e| {
        e.tick >= start - rate / 4
            && e.tick < end + rate / 4
            && (active(e, main)
                || (e.event_name == "weapon_fire"
                    && e.user_steamid.as_deref() == Some(main)
                    && utility::gun(&e.weapon)))
    })
}
fn load(path: &Path, clips: &[Highlight], rate: i32, cancel: &AtomicBool) -> Result<Tracks> {
    let mut ticks = BTreeSet::new();
    let step = (rate / 8).max(1) as usize;
    for h in clips {
        ticks.extend(((h.start_tick - 4 * rate).max(seek::MIN_TICK)..=h.end_tick).step_by(step));
    }
    if ticks.is_empty() {
        return Ok(Tracks::new());
    }
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let bytes = std::fs::read(path)?;
    let rows = crate::parser::DemoParser::new().ticks(
        &bytes,
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

fn samples(
    tracks: &Tracks,
    ids: &[String],
    start: i32,
    end: i32,
    rate: i32,
) -> Option<Vec<Sample>> {
    let mut out: Vec<Sample> = vec![];
    for (&tick, players) in tracks.range(start..=end) {
        let subjects = ids
            .iter()
            .map(|id| member(players, id).map(|p| p.position))
            .collect::<Option<Vec<_>>>()?;
        if out.last().is_some_and(|p| tick - p.tick > rate / 4) {
            return None;
        }
        out.push(Sample { tick, subjects });
    }
    (out.len() >= 2 && out[0].tick - start <= rate / 4 && end - out.last()?.tick <= rate / 4)
        .then_some(out)
}

// Require sustained progress by at least two nearby T players, not a stationary cluster.
fn advancement(
    tracks: &Tracks,
    main: &str,
    start: i32,
    end: i32,
    rate: i32,
) -> Option<(i32, Vec<String>)> {
    if end <= start {
        return None;
    }
    for (&tick, players) in tracks.range(start..end) {
        let a = member(players, main).filter(|p| p.alive && p.team == 2)?;
        let (&later, after) = tracks.range(tick + rate * 3 / 2..).next()?;
        if later > end || later - tick > rate * 7 / 4 {
            continue;
        }
        let Some(b) = member(after, main).filter(|p| p.alive && p.team == 2) else {
            continue;
        };
        let delta = [b.position[0] - a.position[0], b.position[1] - a.position[1]];
        let length = delta[0].hypot(delta[1]);
        if length < 150. {
            continue;
        }
        let mut group = vec![main.to_string()];
        let mut allies: Vec<_> = players
            .iter()
            .filter(|p| {
                p.id != main && p.alive && p.team == 2 && distance(p.position, a.position) <= 450.
            })
            .collect();
        allies.sort_by(|x, y| {
            distance(x.position, a.position).total_cmp(&distance(y.position, a.position))
        });
        for p in allies {
            let Some(q) = member(after, &p.id)
                .filter(|q| q.alive && q.team == 2 && distance(q.position, b.position) <= 500.)
            else {
                continue;
            };
            let movement = [q.position[0] - p.position[0], q.position[1] - p.position[1]];
            let travel = movement[0].hypot(movement[1]);
            if travel >= 100.
                && (movement[0] * delta[0] + movement[1] * delta[1]) / (travel * length) >= 0.5
            {
                // Missing, dead or separated members invalidate the whole movement interval.
                if tracks.range(tick..=later).all(|(_, ps)| {
                    member(ps, main)
                        .zip(member(ps, &p.id))
                        .is_some_and(|(m, t)| {
                            m.alive
                                && t.alive
                                && m.team == 2
                                && t.team == 2
                                && distance(m.position, t.position) <= 500.
                        })
                }) {
                    group.push(p.id.clone());
                }
            }
            if group.len() == 3 {
                break;
            }
        }
        if group.len() >= 2 {
            return Some((tick, group));
        }
    }
    None
}

fn camera_shot(
    tracks: &Tracks,
    ids: &[String],
    start: i32,
    end: i32,
    required: i32,
    rate: i32,
    ray: &impl Fn(Vec3, Vec3) -> Occlusion,
) -> Option<Shot> {
    let data = samples(tracks, ids, start, end, rate)?;
    if required < data[0].tick || required > data.last()?.tick {
        return None;
    }
    let (cut, search) = camera_search::search_window(&data, required, ray).ok()?;
    let camera = search.candidates.into_iter().next()?;
    Some(Shot {
        start_tick: cut,
        end_tick: end,
        camera,
        samples: data.into_iter().filter(|s| s.tick >= cut).collect(),
        transition_safe: false,
    })
}

fn plan_one(
    h: &Highlight,
    tracks: &Tracks,
    events: &[utility::Event],
    rate: i32,
    ray: &impl Fn(Vec3, Vec3) -> Occlusion,
) -> Option<Sequence> {
    let main = h.player.steamid.as_str();
    let anchor = events
        .iter()
        .find(|e| {
            e.event_name == "player_death"
                && e.attacker_steamid.as_deref() == Some(main)
                && e.tick >= h.start_tick
                && e.tick < h.end_tick
                && utility::enemies(e)
        })?
        .tick;
    let round = events
        .iter()
        .filter(|e| e.event_name == "round_start" && e.tick <= anchor)
        .map(|e| e.tick)
        .max()
        .unwrap_or(seek::MIN_TICK);
    let earliest = (h.start_tick - 4 * rate)
        .max(anchor - 7 * rate)
        .max(round + rate);
    let (onset, group) = advancement(tracks, main, earliest, anchor - rate / 2, rate)?;
    let start = (onset - 2 * rate).max(earliest);
    let main_action = events
        .iter()
        .filter(|e| e.tick >= start && e.tick <= anchor && active(e, main))
        .map(|e| e.tick)
        .min()
        .unwrap_or(anchor);
    let end = (main_action - rate / 2).min(
        h.round_result
            .as_ref()
            .map(|r| r.from_tick)
            .unwrap_or(h.end_tick),
    );
    if end - start < rate {
        return None;
    }
    let mut shots = vec![];
    // Nearby ally death followed by protagonist killing the same enemy: an entry/trade story.
    let encounter = events
        .iter()
        .filter(|e| {
            e.event_name == "player_death"
                && e.tick > start
                && e.tick + rate / 2 < end
                && e.user_team_num == Some(2)
                && e.attacker_team_num == Some(3)
                && utility::gun(&e.weapon)
        })
        .find_map(|death| {
            let ally = death.user_steamid.as_deref()?;
            let enemy = death.attacker_steamid.as_deref()?;
            if ally == main
                || !events.iter().any(|e| {
                    e.event_name == "player_death"
                        && e.attacker_steamid.as_deref() == Some(main)
                        && e.user_steamid.as_deref() == Some(enemy)
                        && e.tick > death.tick
                        && e.tick <= death.tick + 4 * rate
                })
            {
                return None;
            }
            let (_, ps) = tracks.range(..death.tick).next_back()?;
            if distance(member(ps, ally)?.position, member(ps, main)?.position) > 500. {
                return None;
            }
            let first = events
                .iter()
                .filter(|e| {
                    e.event_name == "player_hurt"
                        && e.tick >= death.tick - 2 * rate
                        && e.tick <= death.tick
                        && [Some(ally), Some(enemy)].contains(&e.user_steamid.as_deref())
                        && [Some(ally), Some(enemy)].contains(&e.attacker_steamid.as_deref())
                })
                .map(|e| e.tick)
                .min()
                .unwrap_or(death.tick);
            let from = (first - rate).max(start + rate / 2);
            let required = (first - rate / 4).max(from);
            camera_shot(
                tracks,
                &[ally.into(), enemy.into()],
                from,
                death.tick + rate / 2,
                required,
                rate,
                ray,
            )
        })
        .filter(|s| !main_owns(events, main, s.start_tick, s.end_tick, rate));
    let group_end = encounter
        .as_ref()
        .map(|s| s.start_tick)
        .unwrap_or((onset + rate * 3 / 2).min(end));
    // Establish the cohort while it can still share a frame. The entry player can
    // leave the room before the rest advance, so do not demand one view of both phases.
    'establish: for duration in [2 * rate, rate * 3 / 2, rate, rate * 3 / 4, rate / 2] {
        for from in (start..=onset.min(group_end - duration)).step_by((rate / 4).max(1) as usize) {
            if let Some(shot) = camera_shot(
                tracks,
                &group,
                from,
                from + duration,
                from + rate / 8,
                rate,
                ray,
            ) {
                if main_owns(events, main, shot.start_tick, shot.end_tick, rate) {
                    continue;
                }
                shots.push(shot);
                break 'establish;
            }
        }
    }
    // A duel by itself is not a team-push establishing sequence.
    if shots.is_empty() {
        return None;
    }
    if let Some(mut shot) = encounter {
        let previous = &shots[0];
        let a = previous.camera.position;
        let b = shot.camera.position;
        shot.transition_safe = previous.end_tick == shot.start_tick
            && ray(a, b) == Occlusion::Clear
            && (0..=8).all(|step| {
                let p: Vec3 = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f64::from(step) / 8.);
                (0..3).all(|axis| {
                    [-12., 12.].iter().all(|offset| {
                        let mut q = p;
                        q[axis] += offset;
                        ray(p, q) == Occlusion::Clear
                    })
                })
            });
        shots.push(shot);
    }
    Some(Sequence {
        id: h.id.clone(),
        steam_id: main.into(),
        tick_rate: f64::from(rate),
        shots,
    })
}

pub(super) struct MapTools<'a> {
    pub game: &'a Path,
    pub vrf: Option<&'a Path>,
    pub root: &'a Path,
}

pub(super) fn prepare(
    clips: &mut [Highlight],
    demo: &DemoInfo,
    path: &Path,
    events: &[utility::Event],
    map: MapTools<'_>,
    cancel: &AtomicBool,
    log: &mut dyn FnMut(String),
) -> Result<Vec<Sequence>> {
    let Some(vrf) = map.vrf else {
        log("team push: map tools unavailable; keeping original POV".into());
        return Ok(vec![]);
    };
    let rate = demo.tick_rate.round() as i32;
    anyhow::ensure!(rate >= 8, "invalid team-push tick rate");
    let tracks = load(path, clips, rate, cancel)?;
    let world =
        crate::analysis::visibility_assets::prepare(map.root, map.game, vrf, &demo.map_name)?;
    let mut plans = vec![];
    for h in clips {
        Control::new(cancel, &mut |_| {}).check_cancelled()?;
        if plans.iter().any(|p: &Sequence| p.id == h.id) {
            continue;
        }
        if !demo
            .players
            .iter()
            .any(|p| p.steamid == h.player.steamid && p.user_id.is_some())
        {
            continue;
        }
        if let Some(plan) = plan_one(h, &tracks, events, rate, &|a, b| world.world.ray(a, b)) {
            h.start_tick = h.start_tick.min(plan.shots[0].start_tick);
            log(format!(
                "team push: {} cameras in {}",
                plan.shots.len(),
                h.id
            ));
            plans.push(plan);
        }
    }
    Ok(plans)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Highlight, Tracks, Vec<utility::Event>) {
        let h=serde_json::from_value(serde_json::json!({"id":"push","player":{"steamid":"main","name":"Main"},"round":1,"startTick":400,"endTick":800,"anchorTick":640,"score":1,"tags":[],"title":"test","kills":[],"breakdown":{}})).unwrap();
        let tracks = (0..=800)
            .step_by(8)
            .map(|tick| {
                (
                    tick,
                    vec![
                        Player {
                            id: "main".into(),
                            team: 2,
                            alive: true,
                            position: [f64::from(tick) * 2., 0., 0.],
                        },
                        Player {
                            id: "ally".into(),
                            team: 2,
                            alive: tick < 480,
                            position: [f64::from(tick.min(480)) * 2. + 80., 30., 0.],
                        },
                        Player {
                            id: "enemy".into(),
                            team: 3,
                            alive: tick < 640,
                            position: [1200., 150., 0.],
                        },
                    ],
                )
            })
            .collect();
        let events=[serde_json::json!({"tick":0,"event_name":"round_start"}),serde_json::json!({"tick":420,"event_name":"player_hurt","user_steamid":"ally","attacker_steamid":"enemy","user_team_num":2,"attacker_team_num":3,"weapon":"m4a1"}),serde_json::json!({"tick":480,"event_name":"player_death","user_steamid":"ally","attacker_steamid":"enemy","user_team_num":2,"attacker_team_num":3,"weapon":"m4a1"}),serde_json::json!({"tick":640,"event_name":"player_death","user_steamid":"enemy","attacker_steamid":"main","user_team_num":3,"attacker_team_num":2,"weapon":"ak47"})].into_iter().map(|e|serde_json::from_value(e).unwrap()).collect();
        (h, tracks, events)
    }
    #[test]
    fn selects_advancing_attackers_and_entry_trade_before_main_action() {
        let (h, tracks, events) = fixture();
        let plan = plan_one(&h, &tracks, &events, 64, &|_, _| Occlusion::Clear).unwrap();
        assert_eq!(plan.shots.len(), 2);
        assert!(plan.shots[0].end_tick <= plan.shots[1].start_tick);
        assert_eq!(plan.shots[1].end_tick, 512);
        assert!(plan.shots.last().unwrap().end_tick < 640 - 32);
    }
    #[test]
    fn stationary_defending_diverging_or_missing_teams_do_not_qualify() {
        for mode in 0..4 {
            let (_, mut tracks, _) = fixture();
            for ps in tracks.values_mut() {
                match mode {
                    0 => {
                        ps[0].position = [0., 0., 0.];
                        ps[1].position = [80., 0., 0.];
                    }
                    1 => {
                        ps[0].team = 3;
                        ps[1].team = 3;
                    }
                    2 => ps[1].position[0] = -ps[0].position[0],
                    _ => {
                        ps.remove(1);
                    }
                }
            }
            assert!(
                advancement(&tracks, "main", 192, 600, 64).is_none(),
                "mode {mode}"
            );
        }
    }
    #[test]
    fn main_fire_cannot_be_covered_by_an_independent_encounter() {
        let (h, tracks, mut events) = fixture();
        events.push(serde_json::from_value(serde_json::json!({"tick":430,"event_name":"weapon_fire","user_steamid":"main","weapon":"ak47"})).unwrap());
        events.sort_by_key(|e| e.tick);
        let plan = plan_one(&h, &tracks, &events, 64, &|_, _| Occlusion::Clear).unwrap();
        assert!(plan
            .shots
            .iter()
            .all(|s| s.end_tick <= 414 || s.start_tick > 446));
    }
    #[test]
    fn scheduling_preserves_alternate_views_and_contiguous_camera_handoff() {
        let (mut h, tracks, events) = fixture();
        let mut plan = plan_one(&h, &tracks, &events, 64, &|_, _| Occlusion::Clear).unwrap();
        h.start_tick = plan.shots[0].start_tick;
        let boundary = plan.shots[1].start_tick;
        plan.shots[0].end_tick = boundary;
        let dir = tempfile::tempdir().unwrap();
        let mut commands = vec![];
        schedule(
            &mut commands,
            &[0],
            &[h.clone()],
            &[None],
            &[None],
            &[plan.clone()],
            dir.path(),
        )
        .unwrap();
        assert!(!commands
            .iter()
            .any(|c| c.tick == f64::from(boundary) && c.cmd == "spec_mode 1"));
        assert!(commands
            .iter()
            .any(|c| c.cmd.starts_with("mirv_script_load")));
        let mut alternate = h.clone();
        alternate.player.steamid = "enemy".into();
        assert!(!applies(&plan, &[alternate, h.clone()], &[None, None]));
        let chase = utility::Chase {
            steam_id: "main".into(),
            start_tick: h.start_tick,
            end_tick: h.end_tick,
            grenade: None,
        };
        assert!(!applies(&plan, &[h.clone()], &[Some(chase)]));
        let original_end = h.end_tick;
        assert!(plan.shots.iter().all(|s| s.end_tick < original_end));
    }
    #[test]
    fn missing_geometry_and_round_boundary_keep_original_story() {
        let (h, tracks, mut events) = fixture();
        assert!(plan_one(&h, &tracks, &events, 64, &|_, _| Occlusion::Unknown).is_none());
        events.push(
            serde_json::from_value(serde_json::json!({"tick":600,"event_name":"round_start"}))
                .unwrap(),
        );
        events.sort_by_key(|e| e.tick);
        assert!(plan_one(&h, &tracks, &events, 64, &|_, _| Occlusion::Clear).is_none());
        let options: RenderOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!options.team_push);
        let options: RenderOptions =
            serde_json::from_value(serde_json::json!({"teamPush":true})).unwrap();
        assert!(options.team_push);
    }
}

pub(super) fn schedule(
    schedule: &mut Vec<actions::Scheduled>,
    indices: &[usize],
    recording: &[Highlight],
    utilities: &[Option<utility::Chase>],
    openings: &[Option<opening::Opening>],
    plans: &[Sequence],
    directory: &Path,
) -> Result<()> {
    for (local, &global) in indices.iter().enumerate() {
        let h = &recording[global];
        if utilities[global].is_some() || openings[global].is_some() {
            continue;
        }
        let Some(plan) = plans
            .iter()
            .find(|p| eligible(global, recording, utilities, p))
        else {
            continue;
        };
        let mut plan = plan.clone();
        let guard = (plan.tick_rate * 0.125).ceil() as i32;
        plan.shots.retain_mut(|s| {
            s.start_tick = s.start_tick.max(h.start_tick);
            s.end_tick = s.end_tick.min(h.end_tick - guard).min(
                h.round_result
                    .as_ref()
                    .map(|r| r.from_tick - guard)
                    .unwrap_or(h.end_tick),
            );
            s.end_tick - s.start_tick >= (plan.tick_rate / 2.) as i32
        });
        if plan.shots.is_empty() {
            continue;
        }
        std::fs::create_dir_all(directory)?;
        std::fs::write(
            directory.join("camera-transition.mjs"),
            include_str!("camera-transition.mjs"),
        )?;
        let file = directory.join(format!("team-push-{local}.mjs"));
        std::fs::write(
            &file,
            format!(
                "{}\nstartTeamPushSequence({});\n",
                include_str!("team-push-camera.mjs"),
                serde_json::to_string(&plan)?
            ),
        )?;
        let setup = seek::SeekWindow::new(
            h.start_tick,
            h.end_tick,
            None,
            plan.tick_rate.round() as i32,
        )
        .setup_tick;
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1) + 0.002,
            cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&file)),
        });
        for shot in &plan.shots {
            schedule.push(actions::Scheduled {
                tick: f64::from(shot.start_tick) - 0.01,
                cmd: "spec_mode 3".into(),
            });
            if !plan
                .shots
                .iter()
                .any(|next| next.start_tick == shot.end_tick)
            {
                schedule.push(actions::Scheduled {
                    tick: f64::from(shot.end_tick),
                    cmd: "spec_mode 1".into(),
                });
            }
        }
    }
    schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    Ok(())
}
