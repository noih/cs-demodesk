use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepend_once_keep_utility_order_and_skip_dead_or_round_boundary() {
        let demo: DemoInfo = serde_json::from_value(serde_json::json!({"path":"test.dem","mapName":"de_test","serverName":"test","tickRate":64,
            "players":[{"steamid":"main","name":"Main","teamNumber":3,"userId":0}]})).unwrap();
        let original: Highlight = serde_json::from_value(serde_json::json!({"id":"one","player":{"steamid":"main","name":"Main"},"round":1,
            "startTick":1000,"endTick":1600,"anchorTick":1200,"score":1,"tags":[],"title":"clip","kills":[],"breakdown":{}})).unwrap();
        let mut ally = original.clone();
        ally.player.steamid = "ally".into();
        let camera = utility::Chase {
            steam_id: "ally".into(),
            start_tick: 1000,
            end_tick: 1100,
            grenade: None,
        };
        let round: utility::Event =
            serde_json::from_value(serde_json::json!({"tick":700,"event_name":"round_start"}))
                .unwrap();
        let (shots, utilities, openings) = plan(
            vec![ally.clone(), original.clone()],
            vec![Some(camera.clone()), None],
            &[original.clone()],
            &demo,
            &[round.clone()],
        );
        assert_eq!(shots.len(), 3);
        assert_eq!(shots[0].player.steamid, "main");
        assert_eq!((shots[0].start_tick, shots[0].end_tick), (885, 1000));
        assert_eq!(shots[1].player.steamid, "ally");
        assert!(utilities[0].is_none() && utilities[1].is_some());
        assert_eq!(openings.iter().flatten().count(), 1);
        let opening = openings[0].as_ref().unwrap();
        assert_eq!(opening.end_tick + demo.tick_rate * 0.125, 1000.0);
        for event in [
            serde_json::json!({"tick":900,"event_name":"round_start"}),
            serde_json::json!({"tick":950,"event_name":"player_death","user_steamid":"main"}),
        ] {
            let events = vec![round.clone(), serde_json::from_value(event).unwrap()];
            let (shots, _, openings) = plan(
                vec![ally.clone()],
                vec![Some(camera.clone())],
                &[original.clone()],
                &demo,
                &events,
            );
            assert_eq!(shots.len(), 1);
            assert!(openings[0].is_none());
        }
        let options: RenderOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!options.opening_camera);
        let options: RenderOptions =
            serde_json::from_value(serde_json::json!({"openingCamera":true})).unwrap();
        assert!(options.opening_camera);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Opening {
    steam_id: String,
    start_tick: i32,
    end_tick: f64,
    tick_rate: f64,
}

pub(super) fn plan(
    shots: Vec<Highlight>,
    cameras: Vec<Option<utility::Chase>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[utility::Event],
) -> (
    Vec<Highlight>,
    Vec<Option<utility::Chase>>,
    Vec<Option<Opening>>,
) {
    let mut seen = std::collections::HashSet::new();
    let (mut out, mut utilities, mut openings) = (vec![], vec![], vec![]);
    for (shot, camera) in shots.into_iter().zip(cameras) {
        if seen.insert(shot.id.clone()) {
            if let Some(original) = originals.iter().find(|h| h.id == shot.id) {
                let end = shot.start_tick;
                let start = end - (1.8 * demo.tick_rate).round() as i32;
                let round_start = events
                    .iter()
                    .filter(|e| e.event_name == "round_start" && e.tick <= end)
                    .map(|e| e.tick)
                    .max()
                    .unwrap_or(seek::MIN_TICK);
                let alive = !events.iter().any(|e| {
                    e.event_name == "player_death"
                        && e.user_steamid.as_deref() == Some(original.player.steamid.as_str())
                        && e.tick >= round_start
                        && e.tick <= end
                });
                let slot = demo
                    .players
                    .iter()
                    .any(|p| p.steamid == original.player.steamid && p.user_id.is_some());
                if start > round_start && start > seek::MIN_TICK + 8 && alive && slot {
                    let mut intro = original.clone();
                    intro.start_tick = start;
                    intro.end_tick = end;
                    intro.anchor_tick = start;
                    intro.kills.clear();
                    intro.key_moments.clear();
                    intro.round_result = None;
                    openings.push(Some(Opening {
                        steam_id: intro.player.steamid.clone(),
                        start_tick: start,
                        end_tick: f64::from(end) - demo.tick_rate * 0.125,
                        tick_rate: demo.tick_rate,
                    }));
                    utilities.push(None);
                    out.push(intro);
                }
            }
        }
        out.push(shot);
        utilities.push(camera);
        openings.push(None);
    }
    (out, utilities, openings)
}

pub(super) fn schedule(
    schedule: &mut Vec<actions::Scheduled>,
    indices: &[usize],
    cameras: &[Option<Opening>],
    directory: &Path,
) -> Result<()> {
    for (local, &global) in indices.iter().enumerate() {
        let Some(camera) = &cameras[global] else {
            continue;
        };
        std::fs::create_dir_all(directory)?;
        let file = directory.join(format!("opening-{local}.mjs"));
        std::fs::write(
            &file,
            format!(
                "{}\nstartOpening({});\n",
                include_str!("opening-camera.mjs"),
                serde_json::to_string(camera)?
            ),
        )?;
        let setup = seek::SeekWindow::new(
            camera.start_tick,
            camera.end_tick.ceil() as i32,
            None,
            camera.tick_rate.round() as i32,
        )
        .setup_tick;
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1),
            cmd: "spec_mode 3".into(),
        });
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1) + 0.001,
            cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&file)),
        });
        schedule.push(actions::Scheduled {
            tick: camera.end_tick,
            cmd: "spec_mode 1".into(),
        });
    }
    schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    Ok(())
}
