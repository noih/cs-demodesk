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
            spatial: None,
        },
        features::REGISTRY,
    )?;
    assert_eq!(analysis.candidates.len(), 5);
    assert_eq!(analysis.warnings, ["pushUnavailable"]);
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
            spatial: None,
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
                    },
                    demo_data::Player {
                        id: "ally".into(),
                        team: 2,
                        alive: tick < 480,
                        position: [f64::from(tick.min(480)) * 2. + 80., 30., 0.],
                    },
                    demo_data::Player {
                        id: "enemy".into(),
                        team: 3,
                        alive: tick < 640,
                        position: [1200., 150., 0.],
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
            spatial: Some(context::Spatial { tracks, geometry }),
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
