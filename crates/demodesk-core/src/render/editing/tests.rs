use super::*;
use serde_json::json;

fn fixture() -> (DemoInfo, Highlight, Vec<demo_data::Event>) {
    let demo = serde_json::from_value(json!({"path":"missing.dem","mapName":"test","serverName":"test","tickRate":64,
        "players":[{"steamid":"main","name":"Main","userId":0,"teamNumber":2},{"steamid":"ally","name":"Ally","userId":1,"teamNumber":2},
        {"steamid":"a","name":"A","userId":2,"teamNumber":3},{"steamid":"b","name":"B","userId":3,"teamNumber":3},{"steamid":"c","name":"C","userId":4,"teamNumber":3}]})).unwrap();
    let h = serde_json::from_value(json!({"id":"one","player":{"steamid":"main","name":"Main"},"round":1,
        "startTick":800,"endTick":2000,"anchorTick":1100,"score":1,"tags":[],"title":"test","kills":[],"breakdown":{}})).unwrap();
    let mut events: Vec<demo_data::Event> = [
        json!({"tick":600,"event_name":"round_start"}),
        json!({"tick":900,"event_name":"weapon_fire","weapon":"flashbang","user_steamid":"ally"}),
        json!({"tick":1000,"event_name":"flashbang_detonate","user_steamid":"ally","entityid":500}),
        json!({"tick":1000,"event_name":"player_blind","user_steamid":"a","attacker_steamid":"ally","user_team_num":3,"attacker_team_num":2,"entityid":500,"blind_duration":3}),
        json!({"tick":1570,"event_name":"player_hurt","weapon":"m4a1","user_steamid":"main","attacker_steamid":"b","user_team_num":2,"attacker_team_num":3,"dmg_health":26}),
    ].into_iter().map(|e| serde_json::from_value(e).unwrap()).collect();
    for (tick, victim) in [(1100, "a"), (1600, "b"), (1750, "c")] {
        events.push(serde_json::from_value(json!({"tick":tick,"event_name":"player_death","weapon":"ak47","attacker_steamid":"main","user_steamid":victim,"attacker_team_num":2,"user_team_num":3})).unwrap());
    }
    events.sort_by_key(|e| e.tick);
    (demo, h, events)
}

fn opening_spatial() -> context::Spatial {
    use crate::analysis::{
        line_of_sight::{Triangle, World},
        visibility_assets,
    };
    let tracks = (0..=2400)
        .step_by(8)
        .map(|tick| {
            (
                tick,
                vec![demo_data::Player {
                    id: "main".into(),
                    team: 2,
                    alive: true,
                    position: [0., 0., 0.],
                    view: Some(demo_data::PlayerView {
                        yaw: 0.,
                        pitch: 0.,
                        duck: 0.,
                    }),
                }],
            )
        })
        .collect();
    let geometry = visibility_assets::Prepared {
        fingerprint: "test".into(),
        bytes: 1,
        world: World::new(vec![Triangle {
            vertices: [
                [-10000., -10000., 0.],
                [-10001., -10000., 0.],
                [-10000., -10001., 0.],
            ],
            opaque: true,
        }])
        .unwrap(),
    };
    context::Spatial {
        tracks,
        geometry,
        projectiles: Default::default(),
    }
}

#[test]
fn non_gun_kills_remain_selectable_without_being_recommended() -> Result<()> {
    for weapon in [
        "hegrenade",
        "molotov",
        "inferno",
        "flashbang",
        "decoy",
        "smokegrenade",
        "knife",
        "knife_t",
    ] {
        let (demo, h, mut events) = fixture();
        events
            .iter_mut()
            .find(|e| e.event_name == "player_death" && e.tick == 1750)
            .unwrap()
            .weapon = weapon.into();
        let clips = vec![h];
        let analysis = Analysis::analyze_context(
            "all-weapons".into(),
            context::Context {
                demo: &demo,
                originals: &clips,
                recording: &clips,
                events,
                cancel: &AtomicBool::new(false),
                spatial: None,
            },
            features::REGISTRY,
        )?;
        let replays: Vec<_> = analysis
            .candidates
            .iter()
            .filter(|c| c.kind == Kind::Combat)
            .collect();
        assert_eq!(
            replays.len(),
            3,
            "{weapon} must not hide a selectable victim replay"
        );
        let candidate = replays.iter().find(|c| c.tick == 1750).unwrap();
        assert!(
            !candidate.recommended,
            "{weapon} does not meet the gun-exchange recommendation rule"
        );
        let plan = analysis.select_with_links(std::slice::from_ref(&candidate.id), &[], &demo)?;
        let enemy = plan
            .recording
            .iter()
            .find(|h| h.player.steamid == "c")
            .unwrap();
        assert_eq!(
            (enemy.start_tick, enemy.end_tick),
            (1750 - 96, 1750 + pacing::event_tail(64))
        );
    }
    Ok(())
}

#[test]
fn manual_replay_links_control_order_without_repeating_main_action() -> Result<()> {
    let (demo, h, events) = fixture();
    let clips = vec![h.clone()];
    let analysis = Analysis::analyze_context(
        "linked".into(),
        context::Context {
            demo: &demo,
            originals: &clips,
            recording: &clips,
            events,
            cancel: &AtomicBool::new(false),
            spatial: None,
        },
        features::REGISTRY,
    )?;
    let replays: Vec<_> = analysis
        .candidates
        .iter()
        .filter(|c| c.kind == Kind::Combat)
        .collect();
    assert_eq!(replays.len(), 3);
    let ids: Vec<_> = replays.iter().map(|c| c.id.clone()).collect();
    let links: Vec<_> = ids
        .windows(2)
        .map(|pair| ReplayLink {
            from: pair[0].clone(),
            to: pair[1].clone(),
        })
        .collect();
    for (chosen, expected) in [
        (vec![], vec!["main", "a", "main", "b", "main", "c", "main"]),
        (links.clone(), vec!["main", "a", "b", "c"]),
        (
            vec![links[0].clone()],
            vec!["main", "a", "b", "main", "c", "main"],
        ),
    ] {
        let plan = analysis.select_with_links(&ids, &chosen, &demo)?;
        assert_eq!(
            plan.recording
                .iter()
                .map(|h| h.player.steamid.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        let main: Vec<_> = plan
            .recording
            .iter()
            .filter(|h| h.player.steamid == "main")
            .collect();
        assert_eq!(main[0].start_tick, h.start_tick);
        assert_eq!(main.last().unwrap().end_tick, h.end_tick);
        assert!(main
            .windows(2)
            .all(|pair| pair[0].end_tick == pair[1].start_tick));
        assert!(plan
            .recording
            .iter()
            .all(|h| h.end_tick - h.start_tick >= pacing::minimum(64)));
        for enemy in plan.recording.iter().filter(|h| h.player.steamid != "main") {
            let candidate = replays
                .iter()
                .find(|c| c.target_id == enemy.player.steamid)
                .unwrap();
            assert_eq!(enemy.end_tick - candidate.tick, pacing::event_tail(64));
        }
        assert_eq!(plan.preview(&clips).len(), plan.recording.len());
    }
    for invalid in [
        vec![ReplayLink {
            from: ids[0].clone(),
            to: ids[2].clone(),
        }],
        vec![ReplayLink {
            from: ids[1].clone(),
            to: ids[0].clone(),
        }],
        vec![links[0].clone(), links[0].clone()],
        vec![ReplayLink {
            from: "unknown".into(),
            to: ids[1].clone(),
        }],
    ] {
        assert!(analysis.select_with_links(&ids, &invalid, &demo).is_err());
    }
    assert!(analysis
        .select_with_links(&ids[1..], &links, &demo)
        .is_err());
    let roundtrip: Analysis = serde_json::from_slice(&serde_json::to_vec(&analysis)?)?;
    assert_eq!(
        serde_json::to_value(analysis.select_with_links(&ids, &links, &demo)?)?,
        serde_json::to_value(roundtrip.select_with_links(&ids, &links, &demo)?)?
    );
    Ok(())
}

#[test]
fn prepared_features_roundtrip_and_manual_selection_need_no_demo() -> Result<()> {
    let (demo, h, events) = fixture();
    let clips = vec![h];
    let analysis = Analysis::analyze_context(
        "test".into(),
        context::Context {
            demo: &demo,
            originals: &clips,
            recording: &clips,
            events,
            cancel: &AtomicBool::new(false),
            spatial: Some(opening_spatial()),
        },
        features::REGISTRY,
    )?;
    assert_eq!(analysis.candidates.len(), 5);
    assert!(analysis.warnings.is_empty());
    let teamwork = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Teamwork)
        .unwrap();
    let flash_victim = analysis
        .candidates
        .iter()
        .find(|c| c.target_id == "a")
        .unwrap();
    assert!(teamwork.conflicts.contains(&flash_victim.id));
    assert!(!flash_victim.recommended);
    let manual = analysis
        .candidates
        .iter()
        .find(|c| c.target_id == "c")
        .unwrap();
    assert!(!manual.recommended);
    let all_combat: Vec<_> = analysis
        .candidates
        .iter()
        .filter(|c| c.kind == Kind::Combat)
        .map(|c| c.id.clone())
        .collect();
    let plan = analysis.select(&all_combat, &demo)?;
    assert_eq!(
        plan.recording
            .iter()
            .filter(|h| h.player.steamid != "main")
            .count(),
        3
    );
    let saved = serde_json::to_value(&analysis)?;
    assert!(saved.get("events").is_none());
    let restored: Analysis = serde_json::from_value(saved)?;
    for mask in 0..1 << analysis.candidates.len() {
        let ids: Vec<_> = analysis
            .candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| c.id.clone())
            .collect();
        let before = analysis
            .select(&ids, &demo)
            .map(|p| serde_json::to_value(p).unwrap());
        let after = restored
            .select(&ids, &demo)
            .map(|p| serde_json::to_value(p).unwrap());
        assert_eq!(before.is_ok(), after.is_ok());
        if let (Ok(before), Ok(after)) = (before, after) {
            assert_eq!(before, after);
        }
    }
    assert!(restored
        .select(&[manual.id.clone(), manual.id.clone()], &demo)
        .is_err());
    assert!(restored.select(&["missing".into()], &demo).is_err());
    let mut outdated = restored;
    outdated.version -= 1;
    assert!(outdated.select(&[], &demo).is_err());
    Ok(())
}

#[test]
fn recommended_linked_replays_can_span_trimmed_main_windows() -> Result<()> {
    let (demo, h, mut events) = fixture();
    events.push(serde_json::from_value(json!({"tick":1720,"event_name":"player_hurt","weapon":"m4a1","user_steamid":"main","attacker_steamid":"c","user_team_num":2,"attacker_team_num":3,"dmg_health":26}))?);
    events.sort_by_key(|e| e.tick);
    let originals = vec![h.clone()];
    let mut first = h.clone();
    first.end_tick = 1300;
    let mut second = h.clone();
    second.start_tick = 1450;
    let windows = vec![first, second];
    let analysis = Analysis::analyze_context(
        "trimmed".into(),
        context::Context {
            demo: &demo,
            originals: &originals,
            recording: &windows,
            events,
            cancel: &AtomicBool::new(false),
            spatial: None,
        },
        features::REGISTRY,
    )?;
    let replays: Vec<_> = analysis
        .candidates
        .iter()
        .filter(|c| c.kind == Kind::Combat)
        .collect();
    assert_eq!(replays.len(), 3);
    assert!(replays[1..].iter().all(|c| c.recommended));
    let ids: Vec<_> = replays.iter().map(|c| c.id.clone()).collect();
    let defaults = features::combat::recommended_links(&analysis.candidates, &ids);
    assert_eq!(
        defaults,
        vec![ReplayLink {
            from: ids[1].clone(),
            to: ids[2].clone()
        }]
    );
    let links: Vec<_> = ids
        .windows(2)
        .map(|pair| ReplayLink {
            from: pair[0].clone(),
            to: pair[1].clone(),
        })
        .collect();
    let plan = analysis.select_with_links(&ids, &links, &demo)?;
    assert_eq!(
        plan.recording
            .iter()
            .map(|h| h.player.steamid.as_str())
            .collect::<Vec<_>>(),
        ["main", "main", "a", "b", "c"]
    );
    assert_eq!(
        (plan.recording[0].start_tick, plan.recording[0].end_tick),
        (800, 1300)
    );
    assert_eq!(
        (plan.recording[1].start_tick, plan.recording[1].end_tick),
        (1450, 2000)
    );
    Ok(())
}

#[test]
fn registry_subset_keeps_unrelated_features_out_of_the_plan() -> Result<()> {
    let (demo, h, events) = fixture();
    let clips = vec![h];
    let registry: Vec<_> = features::REGISTRY
        .iter()
        .filter(|f| f.kind == Kind::Combat)
        .copied()
        .collect();
    assert!(!registry.iter().any(|f| f.spatial));
    let analysis = Analysis::analyze_context(
        "test".into(),
        context::Context {
            demo: &demo,
            originals: &clips,
            recording: &clips,
            events,
            cancel: &AtomicBool::new(false),
            spatial: Some(opening_spatial()),
        },
        &registry,
    )?;
    assert!(analysis.warnings.is_empty());
    assert!(analysis.candidates.iter().all(|c| c.kind == Kind::Combat));
    let ids = analysis
        .candidates
        .iter()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    let plan = analysis.select(&ids, &demo)?;
    assert!(plan.openings.iter().all(Option::is_none));
    assert!(plan.utilities.iter().all(Option::is_none));
    assert!(plan.teams.is_empty());
    Ok(())
}

#[test]
fn push_owns_intro_but_preserves_selected_combat() -> Result<()> {
    use crate::analysis::{
        line_of_sight::{Triangle, World},
        visibility_assets,
    };
    let (mut demo, mut h, _) = fixture();
    h.start_tick = 400;
    h.end_tick = 800;
    h.anchor_tick = 640;
    demo.players[2].steamid = "enemy".into();
    let tracks = (0..=800)
        .step_by(8)
        .map(|tick| {
            (
                tick,
                vec![
                    demo_data::Player {
                        id: "main".into(),
                        team: 2,
                        alive: true,
                        position: [f64::from(tick) * 2., 0., 0.],
                        view: Some(demo_data::PlayerView {
                            yaw: 0.,
                            pitch: 0.,
                            duck: 0.,
                        }),
                    },
                    demo_data::Player {
                        id: "ally".into(),
                        team: 2,
                        alive: tick < 480,
                        position: [f64::from(tick.min(480)) * 2. + 80., 30., 0.],
                        view: Some(demo_data::PlayerView {
                            yaw: 0.,
                            pitch: 0.,
                            duck: 0.,
                        }),
                    },
                    demo_data::Player {
                        id: "enemy".into(),
                        team: 3,
                        alive: tick < 640,
                        position: [1200., 150., 0.],
                        view: Some(demo_data::PlayerView {
                            yaw: 0.,
                            pitch: 0.,
                            duck: 0.,
                        }),
                    },
                ],
            )
        })
        .collect();
    let events = [json!({"tick":0,"event_name":"round_start"}),
        json!({"tick":420,"event_name":"player_hurt","user_steamid":"ally","attacker_steamid":"enemy","user_team_num":2,"attacker_team_num":3,"weapon":"m4a1"}),
        json!({"tick":480,"event_name":"player_death","user_steamid":"ally","attacker_steamid":"enemy","user_team_num":2,"attacker_team_num":3,"weapon":"m4a1"}),
        json!({"tick":640,"event_name":"player_death","user_steamid":"enemy","attacker_steamid":"main","user_team_num":3,"attacker_team_num":2,"weapon":"ak47"}),
    ].into_iter().map(|e| serde_json::from_value(e).unwrap()).collect();
    let geometry = visibility_assets::Prepared {
        world: World::new(vec![Triangle {
            vertices: [
                [-10000., -10000., -10000.],
                [-9999., -10000., -10000.],
                [-10000., -9999., -10000.],
            ],
            opaque: true,
        }])?,
        fingerprint: String::new(),
        bytes: 0,
    };
    let clips = vec![h];
    let analysis = Analysis::analyze_context(
        "push".into(),
        context::Context {
            demo: &demo,
            originals: &clips,
            recording: &clips,
            events,
            cancel: &AtomicBool::new(false),
            spatial: Some(context::Spatial {
                tracks,
                geometry,
                projectiles: Default::default(),
            }),
        },
        features::REGISTRY,
    )?;
    let push = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Push)
        .expect("moving attackers");
    let intro = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Opening)
        .unwrap();
    assert!(push.recommended && !intro.recommended);
    assert!(push.conflicts.contains(&intro.id));
    assert!(analysis
        .select(&[push.id.clone(), intro.id.clone()], &demo)
        .is_err());
    let combat = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Combat)
        .unwrap();
    let plan = analysis.select(&[push.id.clone(), combat.id.clone()], &demo)?;
    assert_eq!(plan.teams.len(), 1);
    assert!(plan.openings.iter().all(Option::is_none));
    assert!(plan.recording.iter().any(|h| h.player.steamid == "enemy"));
    Ok(())
}

#[test]
fn opening_adds_time_before_original_pov_even_when_utility_throw_is_earlier() -> Result<()> {
    let (demo, original, events) = fixture();
    let originals = vec![original.clone()];
    // The original highlight is trimmed to 900; the utility chase starts at 852.
    let mut clip = original;
    clip.start_tick = 900;
    let recording = vec![clip];
    let analysis = Analysis::analyze_context(
        "intro-regression".into(),
        context::Context {
            demo: &demo,
            originals: &originals,
            recording: &recording,
            events,
            spatial: Some(opening_spatial()),
            cancel: &AtomicBool::new(false),
        },
        features::REGISTRY,
    )?;
    let opening = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Opening)
        .unwrap();
    let teamwork = analysis
        .candidates
        .iter()
        .find(|c| c.kind == Kind::Teamwork)
        .unwrap();
    let story = analysis.select(&[teamwork.id.clone()], &demo)?;
    let combined = analysis.select(&[opening.id.clone(), teamwork.id.clone()], &demo)?;
    assert_eq!(
        (
            combined.recording[0].start_tick,
            combined.recording[0].end_tick
        ),
        (785, 900)
    );
    assert!(combined.openings[0].is_some());
    assert_eq!(combined.recording[1].player.steamid, "main");
    assert_eq!(
        (
            combined.recording[1].start_tick,
            combined.recording[1].end_tick
        ),
        (900, 996)
    );
    assert!(combined.utilities[1].is_none());
    assert!(combined.openings[1].is_none());
    assert!(combined.utilities[2].is_some());
    assert_eq!(
        combined.recording[2].start_tick, 852,
        "the throw can rewind after the original POV"
    );
    assert_eq!(
        serde_json::to_value(&combined.recording[1..])?,
        serde_json::to_value(&story.recording)?
    );
    assert_eq!(
        combined
            .recording
            .iter()
            .map(|h| h.end_tick - h.start_tick)
            .sum::<i32>()
            - story
                .recording
                .iter()
                .map(|h| h.end_tick - h.start_tick)
                .sum::<i32>(),
        115
    );
    Ok(())
}

#[test]
fn missing_geometry_does_not_offer_an_unchecked_opening() -> Result<()> {
    let (demo, h, events) = fixture();
    let clips = vec![h];
    let analysis = Analysis::analyze_context(
        "test".into(),
        context::Context {
            demo: &demo,
            originals: &clips,
            recording: &clips,
            events,
            cancel: &AtomicBool::new(false),
            spatial: None,
        },
        features::REGISTRY,
    )?;
    assert!(!analysis.candidates.iter().any(|c| c.kind == Kind::Opening));
    assert!(analysis.warnings.iter().any(|w| w == "openingUnavailable"));
    assert!(analysis.candidates.iter().any(|c| c.kind == Kind::Combat));
    Ok(())
}

#[test]
fn final_pacing_guard_rejects_short_fragments_from_combined_edits() -> Result<()> {
    let (_, mut h, _) = fixture();
    h.end_tick = h.start_tick + 3;
    let plan = Plan::plain(vec![h]);
    assert!(pacing::validate(&plan, &["one"], 64).is_err());
    assert!(
        pacing::validate(&plan, &[], 64).is_ok(),
        "ordinary export is unchanged"
    );
    Ok(())
}
