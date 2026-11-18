use super::*;
mod path;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hide_viewmodel_until_pov_and_restore_the_export_setting() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let camera = Opening {
            steam_id: "main".into(),
            start_tick: 1000,
            end_tick: 1107.,
            tick_rate: 64.,
            path: vec![],
        };
        for enabled in [true, false] {
            let mut commands = vec![];
            schedule(
                &mut commands,
                &[0],
                &[Some(camera.clone())],
                directory.path(),
                enabled,
            )?;
            let hidden = commands
                .iter()
                .find(|s| s.cmd == "spec_mode 3; r_drawviewmodel 0")
                .unwrap();
            assert!(hidden.tick < f64::from(camera.start_tick));
            let pov = commands.iter().find(|s| s.cmd == "spec_mode 1").unwrap();
            assert!(pov.tick < camera.end_tick);
            let restored = commands
                .iter()
                .find(|s| s.cmd == format!("r_drawviewmodel {}", enabled as u8))
                .unwrap();
            assert_eq!(restored.tick, camera.end_tick);
            assert!(restored.tick < camera.end_tick + camera.tick_rate * 0.125);
        }
        Ok(())
    }

    #[test]
    fn prepend_once_keep_utility_order_and_skip_dead_or_round_boundary() {
        let demo: DemoInfo = serde_json::from_value(serde_json::json!({"path":"test.dem","mapName":"de_test","serverName":"test","tickRate":64,
            "players":[{"steamid":"main","name":"Main","teamNumber":3,"userId":0}]})).unwrap();
        let original: Highlight = serde_json::from_value(serde_json::json!({"id":"one","player":{"steamid":"main","name":"Main"},"round":1,
            "startTick":1000,"endTick":1600,"anchorTick":1200,"score":1,"tags":[],"title":"clip","kills":[],"breakdown":{}})).unwrap();
        let mut ally = original.clone();
        ally.player.steamid = "ally".into();
        let camera = utility::Camera::Follow(utility::Chase {
            steam_id: "ally".into(),
            start_tick: 1000,
            end_tick: 1100,
            grenade: None,
        });
        let round: demo_data::Event =
            serde_json::from_value(serde_json::json!({"tick":700,"event_name":"round_start"}))
                .unwrap();
        let (shots, utilities, openings) = plan(
            vec![original.clone(), ally.clone()],
            vec![None, Some(camera.clone())],
            &[original.clone()],
            &demo,
            &[round.clone()],
        );
        assert_eq!(shots.len(), 3);
        assert_eq!(shots[0].player.steamid, "main");
        assert_eq!((shots[0].start_tick, shots[0].end_tick), (885, 1000));
        assert_eq!(shots[1].player.steamid, "main");
        assert_eq!(shots[2].player.steamid, "ally");
        assert!(utilities[0].is_none() && utilities[1].is_none() && utilities[2].is_some());
        assert_eq!(openings.iter().flatten().count(), 1);
        let opening = openings[0].as_ref().unwrap();
        assert_eq!(opening.end_tick + demo.tick_rate * 0.125, 1000.0);
        for event in [
            serde_json::json!({"tick":900,"event_name":"round_start"}),
            serde_json::json!({"tick":950,"event_name":"player_death","user_steamid":"main"}),
        ] {
            let events = vec![round.clone(), serde_json::from_value(event).unwrap()];
            let (shots, _, openings) = plan(
                vec![original.clone()],
                vec![None],
                &[original.clone()],
                &demo,
                &events,
            );
            assert_eq!(shots.len(), 1);
            assert!(openings[0].is_none());
        }
        let (_, _, invalid_handoff) = plan(
            vec![ally],
            vec![Some(camera)],
            &[original.clone()],
            &demo,
            &[round],
        );
        assert!(
            invalid_handoff.iter().all(Option::is_none),
            "an intro must not hand off to a utility camera"
        );
        let options: RenderOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!options.opening_camera);
        let options: RenderOptions =
            serde_json::from_value(serde_json::json!({"openingCamera":true})).unwrap();
        assert!(options.opening_camera);
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Opening {
    steam_id: String,
    start_tick: i32,
    end_tick: f64,
    tick_rate: f64,
    path: Vec<path::Keyframe>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Prepared {
    windows: Vec<AliveWindow>,
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    highlight_id: String,
    tick: i32,
    path: Option<Vec<path::Keyframe>>,
}

#[derive(Serialize, Deserialize)]
struct AliveWindow {
    player: String,
    start: i32,
    end: i32,
}

pub(super) fn prepare(
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[demo_data::Event],
) -> Prepared {
    let mut rounds: Vec<_> = events
        .iter()
        .filter(|e| e.event_name == "round_start")
        .map(|e| e.tick)
        .collect();
    rounds.insert(0, seek::MIN_TICK);
    rounds.push(i32::MAX);
    let mut windows = vec![];
    let mains: std::collections::BTreeSet<_> = originals
        .iter()
        .map(|h| h.player.steamid.as_str())
        .collect();
    for player in mains.into_iter().filter(|id| {
        demo.players
            .iter()
            .any(|p| p.steamid == *id && p.user_id.is_some())
    }) {
        for round in rounds.windows(2) {
            let death = events
                .iter()
                .filter(|e| {
                    e.event_name == "player_death"
                        && e.user_steamid.as_deref() == Some(player)
                        && e.tick >= round[0]
                        && e.tick < round[1]
                })
                .map(|e| e.tick)
                .min()
                .unwrap_or(round[1]);
            windows.push(AliveWindow {
                player: player.into(),
                start: round[0],
                end: death,
            });
        }
    }
    let mut seen = std::collections::HashSet::new();
    let entries = originals
        .iter()
        .filter(|h| seen.insert(h.id.clone()))
        .map(|h| Entry {
            highlight_id: h.id.clone(),
            tick: h.start_tick,
            path: None,
        })
        .collect();
    Prepared { windows, entries }
}

impl Prepared {
    pub(super) fn plan_paths(
        &mut self,
        originals: &[Highlight],
        tracks: &demo_data::Tracks,
        rate: f64,
        ray: &impl Fn(
            crate::analysis::line_of_sight::Vec3,
            crate::analysis::line_of_sight::Vec3,
        ) -> crate::analysis::line_of_sight::Occlusion,
    ) {
        for entry in &mut self.entries {
            let Some(h) = originals.iter().find(|h| h.id == entry.highlight_id) else {
                continue;
            };
            let start = entry.tick - (1.8 * rate).round() as i32;
            entry.path = path::plan(
                tracks,
                &h.player.steamid,
                start,
                f64::from(entry.tick) - rate * 0.125,
                rate,
                ray,
            );
        }
    }
}

#[cfg(test)]
pub(super) fn plan(
    shots: Vec<Highlight>,
    cameras: Vec<Option<utility::Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[demo_data::Event],
) -> (
    Vec<Highlight>,
    Vec<Option<utility::Camera>>,
    Vec<Option<Opening>>,
) {
    let mut prepared = prepare(originals, demo, events);
    let tracks = (0..=2200)
        .step_by(8)
        .map(|tick| {
            (
                tick,
                originals
                    .iter()
                    .map(|h| demo_data::Player {
                        id: h.player.steamid.clone(),
                        team: 3,
                        alive: true,
                        position: [0., 0., 0.],
                        view: Some(demo_data::PlayerView {
                            yaw: 0.,
                            pitch: 0.,
                            duck: 0.,
                        }),
                    })
                    .collect(),
            )
        })
        .collect();
    prepared.plan_paths(originals, &tracks, demo.tick_rate, &|_, _| {
        crate::analysis::line_of_sight::Occlusion::Clear
    });
    compose(shots, cameras, originals, demo, &prepared)
}

pub(super) fn compose(
    shots: Vec<Highlight>,
    cameras: Vec<Option<utility::Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    prepared: &Prepared,
) -> (
    Vec<Highlight>,
    Vec<Option<utility::Camera>>,
    Vec<Option<Opening>>,
) {
    let mut seen = std::collections::HashSet::new();
    let (mut out, mut utilities, mut openings) = (vec![], vec![], vec![]);
    for (shot, camera) in shots.into_iter().zip(cameras) {
        if seen.insert(shot.id.clone()) {
            if let Some((original, entry)) = originals
                .iter()
                .find(|h| h.id == shot.id)
                .zip(prepared.entries.iter().find(|e| e.highlight_id == shot.id))
            {
                let end = entry.tick;
                let start = end - (1.8 * demo.tick_rate).round() as i32;
                if entry.path.is_some()
                    && shot.start_tick == end
                    && shot.player.steamid == original.player.steamid
                    && camera.is_none()
                    && start > seek::MIN_TICK + 8
                    && prepared.windows.iter().any(|w| {
                        w.player == original.player.steamid && start > w.start && end < w.end
                    })
                {
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
                        path: entry.path.clone().unwrap(),
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
    viewmodel: bool,
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
            cmd: "spec_mode 3; r_drawviewmodel 0".into(),
        });
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1) + 0.001,
            cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&file)),
        });
        schedule.push(actions::Scheduled {
            tick: camera.end_tick - camera.tick_rate * 0.15,
            cmd: "spec_mode 1".into(),
        });
        schedule.push(actions::Scheduled {
            tick: camera.end_tick,
            cmd: format!("r_drawviewmodel {}", viewmodel as u8),
        });
    }
    schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    Ok(())
}
