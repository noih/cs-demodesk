//! Builds the command schedule HLAE executes during demo playback. HLAE's
//! `mirv_cmd` command system runs every scheduled command whose demo tick falls
//! in [previousTick, currentTick) as playback advances (ascending order), so no
//! server plugin, gameinfo.gi patch or engine callback is needed: we load the
//! schedule through the game's netcon before starting the demo.
//!
//! Timing rules:
//!  - nothing may be scheduled below tick 96 (early ticks get skipped)
//!  - `demo_gototick` lands one tick before the setup tick; the setup commands
//!    (`spec_mode 1` before `spec_player`) then run on the next frame
//!  - clips are played in tick order and jumps are forward-only: backward jumps
//!    execute nothing in mirv_cmd; the renderer splits overlaps into separate sessions
//!  - `quit` ~1 s after the last clip ends
//!  - `echo [demodesk] …` markers let the app follow progress on the netcon
use super::RenderOptions;
use crate::model::Highlight;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct RenderClip {
    pub highlight: Highlight,
    /// CS2 spec_player slot (user_id + 1)
    pub slot: Option<i32>,
    pub round_result_slot: Option<i32>,
    /// Steam account id, for `spec_lock_to_accountid`
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Camera {
    Slot,
    Lock,
}

/// What the schedule needs besides the user's [`RenderOptions`].
#[derive(Debug, Clone)]
pub struct ActionsOptions<'a> {
    pub render: &'a RenderOptions,
    pub tick_rate: f64,
    /// Folder that HLAE writes into; forward slashes, absolute
    pub output_dir: String,
    /// e.g. "-c:v libx264 -pix_fmt yuv420p -crf 23" — the output path is appended by us
    pub ffmpeg_preset: String,
}

/// One console command at a (fractional) demo tick.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scheduled {
    pub tick: f64,
    pub cmd: String,
}

const MIN_TICK: i32 = 96;
/// Marker prefix echoed to the console at every milestone.
pub const MARK: &str = "[demodesk]";

pub fn sequence_folder_name(index: usize) -> String {
    format!("{}-sequence", index + 1)
}

pub fn steamid_to_account_id(steamid64: &str) -> Option<String> {
    steamid64
        .parse::<u64>()
        .ok()
        .map(|v| (v - 76561197960265728).to_string())
}

/// Commands for all clips, sorted by tick. Clips are visited in start-tick
/// order; sequence folders are numbered in that order.
pub fn build_schedule(clips: &[RenderClip], o: &ActionsOptions) -> Vec<Scheduled> {
    let r = o.render;
    let rate = o.tick_rate.round() as i32;
    let mut order: Vec<usize> = (0..clips.len()).collect();
    order.sort_by_key(|&i| clips[i].highlight.start_tick);

    let mut out: Vec<Scheduled> = vec![];
    // `slot` keeps commands scheduled on the same tick in insertion order.
    let mut push = |tick: i32, slot: &mut u32, cmd: String| {
        out.push(Scheduled {
            tick: tick.max(MIN_TICK) as f64 + (*slot as f64) * 0.001,
            cmd,
        });
        *slot += 1;
    };

    let mut prev_end: Option<i32> = None;
    let n = order.len();
    let total_ticks: f64 = clips
        .iter()
        .map(|c| (c.highlight.end_tick - c.highlight.start_tick).max(1) as f64)
        .sum();
    let mut completed_ticks = 0.0;
    for (seq, &ci) in order.iter().enumerate() {
        let clip = &clips[ci];
        let h = &clip.highlight;
        // Setup one second before the clip, never before the previous clip ended.
        let mut setup_tick = (h.start_tick - rate).max(MIN_TICK + 4);
        if let Some(pe) = prev_end {
            setup_tick = setup_tick.max(pe + rate / 2);
        }
        let start_tick = h.start_tick.max(setup_tick + 2);
        let end_tick = h.end_tick.max(start_tick + 1);
        let folder = format!("{}/{}", o.output_dir, sequence_folder_name(seq));
        let preset = format!("demodesk{}", seq + 1);

        // 1. Jump: from tick 96 for the first clip, from just after the previous clip otherwise.
        let jump_from = match prev_end {
            None => MIN_TICK,
            Some(pe) => pe + rate / 4,
        };
        let mut slot = 0;
        push(
            jump_from,
            &mut slot,
            format!("echo {MARK} seq {} of {n} seek", seq + 1),
        );
        if setup_tick - 1 > jump_from + 1 {
            push(
                jump_from,
                &mut slot,
                format!("demo_gototick {}", setup_tick - 1),
            );
        }

        // 2. Baseline + recording configuration at the setup tick.
        let mut slot = 0;
        push(
            setup_tick,
            &mut slot,
            format!("echo {MARK} seq {} of {n} setup", seq + 1),
        );
        for cmd in [
            "sv_cheats 1",
            "demo_ui_mode 0",
            // keep rendering at full speed when the window is not in front
            "engine_no_focus_sleep 0",
            "fullscreen_min_on_focus_loss 0",
            "cl_hud_telemetry_frametime_show 0",
            "cl_hud_telemetry_net_misdelivery_show 0",
            "cl_hud_telemetry_ping_show 0",
            "cl_hud_telemetry_serverrecvmargin_graph_show 0",
            "cl_trueview_show_status 0",
            "r_show_build_info 0",
            "mirv_streams record screen enabled 1",
        ] {
            push(setup_tick, &mut slot, cmd.to_string());
        }
        push(setup_tick, &mut slot, "volume 1".into());
        push(
            setup_tick,
            &mut slot,
            format!("cl_demo_predict {}", r.true_view as u8),
        );
        // HUD: cl_drawhud must stay on for any element; without the main HUD we go through
        // cl_draw_only_deathnotices and force radar / kill feed on or off individually.
        push(
            setup_tick,
            &mut slot,
            format!("cl_drawhud {}", (r.hud || r.radar || r.kill_feed) as u8),
        );
        push(
            setup_tick,
            &mut slot,
            format!("cl_draw_only_deathnotices {}", (!r.hud) as u8),
        );
        push(
            setup_tick,
            &mut slot,
            format!("crosshair {}", r.crosshair as u8),
        );
        push(setup_tick, &mut slot, "cl_show_observer_crosshair 2".into());
        let force = |on: bool| if on { 1 } else { -1 };
        push(
            setup_tick,
            &mut slot,
            format!("cl_drawhud_force_radar {}", force(r.radar)),
        );
        push(
            setup_tick,
            &mut slot,
            format!("cl_drawhud_force_deathnotices {}", force(r.kill_feed)),
        );
        // teammate names / equipment over heads: never (clutter, and it is not the player's own view)
        push(
            setup_tick,
            &mut slot,
            "cl_drawhud_force_teamid_overhead -1".into(),
        );
        push(
            setup_tick,
            &mut slot,
            format!("cl_chatfilters {}", if r.chat { 63 } else { 0 }),
        );
        push(
            setup_tick,
            &mut slot,
            format!("r_drawviewmodel {}", r.viewmodel as u8),
        );
        push(
            setup_tick,
            &mut slot,
            format!("r_drawtracers_firstperson {}", r.tracers as u8),
        );
        push(
            setup_tick,
            &mut slot,
            format!("hud_scaling {:.2}", r.hud_scale.clamp(0.5, 0.95)),
        );
        push(
            setup_tick,
            &mut slot,
            format!("mirv_deathmsg lifetime {}", r.death_notice_seconds),
        );
        push(setup_tick, &mut slot, "mirv_deathmsg filter clear".into());
        push(setup_tick, &mut slot, "mirv_deathmsg clear".into());
        push(
            setup_tick,
            &mut slot,
            format!("tv_listen_voice_indices {}", if r.voice { -1 } else { 0 }),
        );
        push(
            setup_tick,
            &mut slot,
            format!("tv_listen_voice_indices_h {}", if r.voice { -1 } else { 0 }),
        );
        push(
            setup_tick,
            &mut slot,
            "mirv_streams record startMovieWav 1".into(),
        );
        push(
            setup_tick,
            &mut slot,
            format!("mirv_streams record name \"{folder}\""),
        );
        push(
            setup_tick,
            &mut slot,
            format!("spec_show_xray {}", r.xray as u8),
        );
        push(setup_tick, &mut slot, "mp_display_kill_assists 1".into());
        // HLAE unescapes the doubled backslash before the file name; {QUOTE} is its quote placeholder.
        let mut encoding = o.ffmpeg_preset.clone();
        if let Some(view) = h.round_result.as_ref().filter(|v| v.from_tick < end_tick) {
            let mut label: String = r
                .round_result_label
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .take(80)
                .collect();
            let from = (view.from_tick - start_tick).max(0) as f64 / o.tick_rate;
            let fonts = std::path::PathBuf::from(
                std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()),
            )
            .join("Fonts");
            let mut font = fonts.join(
                if label
                    .chars()
                    .any(|c| ('\u{ac00}'..='\u{d7af}').contains(&c))
                {
                    "malgun.ttf"
                } else {
                    "msjh.ttc"
                },
            );
            if !font.is_file() {
                font = fonts.join("arial.ttf");
                label = "Round result".into();
            }
            let font = font
                .to_string_lossy()
                .replace('\\', "/")
                .replace(':', "\\:");
            let filter = format!("drawtext=fontfile='{font}':text='{label}':fontcolor=white:fontsize=h/30:box=1:boxcolor=black@0.6:boxborderw=10:x=(w-tw)/2:y=h*0.22:enable='gte(t,{from:.6})'");
            encoding.push_str(&format!(
                " -vf {{QUOTE}}{}{{QUOTE}}",
                filter.replace('\\', "\\\\")
            ));
        }
        push(setup_tick, &mut slot, format!("mirv_streams settings add ffmpeg {preset} \"{} {{QUOTE}}{folder}\\\\video.{}{{QUOTE}}\"", encoding, r.container));
        push(
            setup_tick,
            &mut slot,
            format!("mirv_streams record screen settings {preset}"),
        );
        push(
            setup_tick,
            &mut slot,
            format!("mirv_streams record fps {}", r.fps),
        );

        // 3. Camera on the highlighted player (after the seek, before recording).
        push(setup_tick, &mut slot, "spec_mode 1".into());
        match (r.camera, &clip.account_id, clip.slot) {
            (Camera::Lock, Some(acc), _) => push(
                setup_tick,
                &mut slot,
                format!("spec_lock_to_accountid {acc}"),
            ),
            (_, _, Some(slot_no)) => push(setup_tick, &mut slot, format!("spec_player {slot_no}")),
            _ => {}
        }

        if let Some(view) = &h.round_result {
            if view.from_tick < end_tick {
                let switch_tick = if view.from_tick <= start_tick {
                    setup_tick + 1
                } else {
                    view.from_tick + 1
                };
                let mut camera_slot = 0;
                push(
                    switch_tick,
                    &mut camera_slot,
                    "spec_lock_to_accountid 0".into(),
                );
                if let Some(target) = clip.round_result_slot {
                    push(
                        switch_tick,
                        &mut camera_slot,
                        format!("spec_player {target}"),
                    );
                }
                push(switch_tick, &mut camera_slot, "spec_mode 3".into());
            }
        }

        if seq > 0 {
            push(
                setup_tick,
                &mut slot,
                format!(
                    "alias demodesk_wait_{} \"demo_pause; echo {MARK} seq {} of {n} settle\"",
                    seq + 1,
                    seq + 1
                ),
            );
        }

        // 4. Record. Later clips start only when the app finishes the real-time wait.
        let mut slot = 0;
        if seq > 0 {
            // The app clears this alias before resuming: HLAE can revisit the pause tick.
            push(start_tick, &mut slot, format!("demodesk_wait_{}", seq + 1));
        } else {
            push(
                start_tick,
                &mut slot,
                format!("echo {MARK} seq {} of {n} start", seq + 1),
            );
            push(start_tick, &mut slot, "mirv_streams record start".into());
        }
        let mut slot = 0;
        push(end_tick, &mut slot, "mirv_streams record end".into());
        push(
            end_tick,
            &mut slot,
            format!("echo {MARK} seq {} of {n} end", seq + 1),
        );

        let clip_ticks = (h.end_tick - h.start_tick).max(1) as f64;
        // At most 100 updates per clip, including long clips; each marker follows real demo time.
        for step in 0..=100 {
            let tick = start_tick + ((end_tick - start_tick) as i64 * step / 100) as i32;
            let fraction = (completed_ticks + clip_ticks * step as f64 / 100.0) / total_ticks;
            let mut slot = 10 + step as u32;
            push(
                tick,
                &mut slot,
                format!("echo {MARK} progress {fraction:.6}"),
            );
        }
        completed_ticks += clip_ticks;

        // 5. Quit one second after the last clip.
        if seq + 1 == n {
            let mut slot = 0;
            push(end_tick + rate, &mut slot, format!("echo {MARK} done"));
            if r.quit_when_done {
                push(end_tick + rate, &mut slot, "quit".into());
            }
        }
        prev_end = Some(end_tick);
    }
    out.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `mirv_cmd load` file: `<commandSystem><commands><c tick="…">cmd</c>…`.
/// Kept sorted so HLAE's (degenerate) interval tree visits them in order.
pub fn mirv_cmd_xml(schedule: &[Scheduled]) -> String {
    let mut s = String::from("<commandSystem>\n<commands>\n");
    for c in schedule {
        s.push_str(&format!(
            "<c tick=\"{:.3}\">{}</c>\n",
            c.tick,
            xml_escape(&c.cmd)
        ));
    }
    s.push_str("</commands>\n</commandSystem>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HighlightPlayer;
    use std::collections::BTreeMap;

    fn opts(render: &RenderOptions) -> ActionsOptions<'_> {
        ActionsOptions {
            render,
            tick_rate: 64.0,
            output_dir: "C:/test-output/r1".into(),
            ffmpeg_preset: "-c:v libx264 -pix_fmt yuv420p -crf 23".into(),
        }
    }

    fn clip(start: i32, end: i32, slot: Option<i32>) -> RenderClip {
        RenderClip {
            highlight: Highlight {
                id: "h".into(),
                player: HighlightPlayer {
                    steamid: (76561197960265728_u64 + 123).to_string(),
                    name: "Player A".into(),
                },
                round: 3,
                start_tick: start,
                end_tick: end,
                anchor_tick: start,
                score: 5.0,
                tags: vec![],
                title: "x".into(),
                kills: vec![],
                key_moments: vec![],
                round_result: None,
                breakdown: BTreeMap::new(),
            },
            slot,
            round_result_slot: None,
            account_id: steamid_to_account_id(&(76561197960265728_u64 + 123).to_string()),
        }
    }

    fn tick_of(sched: &[Scheduled], cmd: &str) -> i32 {
        sched
            .iter()
            .find(|a| a.cmd == cmd || a.cmd.starts_with(&format!("{cmd} ")))
            .unwrap()
            .tick
            .floor() as i32
    }
    fn idx(sched: &[Scheduled], cmd: &str) -> usize {
        sched
            .iter()
            .position(|a| a.cmd == cmd || a.cmd.starts_with(&format!("{cmd} ")))
            .unwrap()
    }

    #[test]
    fn round_result_camera_is_third_person_and_next_clip_resets() {
        let mut ending = clip(40591, 40847, Some(1));
        ending.highlight.round_result = Some(crate::model::RoundResultView {
            from_tick: 40288,
            player: (76561197960265728_u64 + 456).to_string(),
        });
        ending.round_result_slot = Some(7);
        let schedule = build_schedule(
            &[ending.clone(), clip(42000, 42512, Some(1))],
            &opts(&RenderOptions::default()),
        );
        assert_eq!(tick_of(&schedule, "spec_player 7"), 40528);
        assert_eq!(tick_of(&schedule, "spec_mode 3"), 40528);
        assert!(schedule
            .iter()
            .any(|c| c.tick.floor() as i32 == 41936 && c.cmd == "spec_mode 1"));
        assert!(schedule
            .iter()
            .any(|c| c.cmd.contains("drawtext=") && c.cmd.contains("gte(t,0.000000)")));
        ending.highlight.start_tick = 39000;
        let schedule = build_schedule(&[ending], &opts(&RenderOptions::default()));
        assert_eq!(tick_of(&schedule, "spec_mode 3"), 40289);
    }

    #[test]
    fn nearby_windows_merge_strictly_below_one_second_for_the_same_view() {
        for rate in [64.0, 128.0] {
            for gap in [-10, 0, rate as i32 - 1, rate as i32, rate as i32 + 1] {
                let first = clip(1000, 1100, Some(3)).highlight;
                let second = clip(1100 + gap, 1400, Some(3)).highlight;
                let merged =
                    super::super::merge_nearby_clips(vec![second.clone(), first.clone()], rate);
                assert_eq!(merged.len(), if (gap as f64) < rate { 1 } else { 2 });
                if merged.len() == 1 {
                    assert_eq!((merged[0].start_tick, merged[0].end_tick), (1000, 1400));
                }
                let mut other = second.clone();
                other.round += 1;
                assert_eq!(
                    super::super::merge_nearby_clips(vec![first.clone(), other], rate).len(),
                    2
                );
                let mut other = second;
                other.player.steamid = "another-player".into();
                assert_eq!(
                    super::super::merge_nearby_clips(vec![first, other], rate).len(),
                    2
                );
            }
        }
        let merged = super::super::merge_nearby_clips(
            vec![
                clip(1400, 1500, Some(3)).highlight,
                clip(1000, 1200, Some(3)).highlight,
                clip(1250, 1350, Some(3)).highlight,
            ],
            64.0,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!((merged[0].start_tick, merged[0].end_tick), (1000, 1500));
        let s = build_schedule(
            &[RenderClip {
                highlight: merged[0].clone(),
                slot: Some(3),
                round_result_slot: None,
                account_id: None,
            }],
            &opts(&RenderOptions::default()),
        );
        assert_eq!(
            s.iter()
                .filter(|a| a.cmd == "mirv_streams record start")
                .count(),
            1
        );
        assert_eq!(tick_of(&s, "mirv_streams record end"), 1500);
        assert!(!s.iter().any(|a| a.cmd.starts_with("demo_pause")));
    }

    #[test]
    fn later_clips_cannot_start_recording_in_the_pause_batch() {
        for second_start in [10_500, 20_000] {
            let s = build_schedule(
                &[
                    clip(second_start, 21_000, Some(3)),
                    clip(10_000, 11_000, Some(3)),
                ],
                &opts(&RenderOptions::default()),
            );
            let pauses: Vec<_> = s
                .iter()
                .filter(|a| a.cmd.starts_with("demodesk_wait_"))
                .collect();
            assert_eq!(pauses.len(), 1);
            assert_eq!(pauses[0].cmd, "demodesk_wait_2");
            let setup = tick_of(&s, "echo [demodesk] seq 2 of 2 setup");
            assert!(s.iter().any(|a| a.tick.floor() as i32 == setup
                && a.cmd
                    == "alias demodesk_wait_2 \"demo_pause; echo [demodesk] seq 2 of 2 settle\""));
            assert_eq!(pauses[0].tick.floor() as i32, second_start.max(setup + 2));
            assert!(s.iter().any(|a| a.cmd == "spec_player 3"
                && a.tick < pauses[0].tick
                && a.tick >= setup as f64));
            // Even a frame crossing both setup and start cannot enqueue recording early.
            assert!(!s
                .iter()
                .any(|a| a.tick >= setup as f64 && a.cmd == "mirv_streams record start"));
            assert!(!s.iter().any(|a| a.cmd == "demo_resume"));
        }
        let first = build_schedule(
            &[clip(10_000, 11_000, Some(3))],
            &opts(&RenderOptions::default()),
        );
        assert!(!first
            .iter()
            .any(|a| a.cmd.contains("demodesk_wait_") || a.cmd.contains("demo_pause")));
    }

    #[test]
    fn every_clip_keeps_game_audio_for_recording() {
        let clips = [clip(10_000, 11_000, Some(3)), clip(20_000, 21_000, Some(3))];
        for show_game in [false, true] {
            let render = RenderOptions {
                show_game,
                ..RenderOptions::default()
            };
            let schedule = build_schedule(&clips, &opts(&render));
            let volumes: Vec<_> = schedule
                .iter()
                .filter(|a| a.cmd.starts_with("volume "))
                .collect();
            let starts: Vec<_> = schedule
                .iter()
                .filter(|a| {
                    a.cmd == "mirv_streams record start" || a.cmd.starts_with("demodesk_wait_")
                })
                .collect();
            assert_eq!(volumes.len(), clips.len());
            for (volume, start) in volumes.iter().zip(starts) {
                assert_eq!(volume.cmd, "volume 1");
                assert!(volume.tick < start.tick);
            }
        }
    }

    #[test]
    fn recording_progress_follows_duration_across_unequal_clips() {
        let schedule = build_schedule(
            &[clip(10_000, 11_000, Some(3)), clip(20_000, 23_000, Some(3))],
            &opts(&RenderOptions::default()),
        );
        let values: Vec<(i32, f64)> = schedule
            .iter()
            .filter_map(|a| {
                a.cmd
                    .strip_prefix("echo [demodesk] progress ")
                    .map(|p| (a.tick.floor() as i32, p.parse().unwrap()))
            })
            .collect();
        assert_eq!(values.first(), Some(&(10_000, 0.0)));
        assert_eq!(values.last(), Some(&(23_000, 1.0)));
        assert!(values.contains(&(11_000, 0.25)));
        assert!(values.contains(&(20_000, 0.25)));
        assert!(values.windows(2).all(|w| w[0].1 <= w[1].1));
    }

    #[test]
    fn schedule_is_ordered() {
        let s = build_schedule(
            &[clip(10_000, 11_000, Some(3))],
            &opts(&RenderOptions::default()),
        );
        assert_eq!(tick_of(&s, "demo_gototick"), 96);
        assert!(s.iter().any(|a| a.cmd == "demo_gototick 9935"));
        assert_eq!(tick_of(&s, "mirv_streams record name"), 9936);
        assert_eq!(tick_of(&s, "spec_mode 1"), 9936);
        assert_eq!(tick_of(&s, "spec_player"), 9936);
        assert_eq!(tick_of(&s, "mirv_streams record start"), 10_000);
        assert_eq!(tick_of(&s, "mirv_streams record end"), 11_000);
        assert_eq!(tick_of(&s, "quit"), 11_064);
        assert!(idx(&s, "spec_mode 1") < idx(&s, "spec_player 3"));
        assert!(idx(&s, "mirv_streams record name") < idx(&s, "mirv_streams record start"));
        assert!(s.windows(2).all(|w| w[0].tick <= w[1].tick));
        let preset = s
            .iter()
            .find(|a| a.cmd.starts_with("mirv_streams settings add ffmpeg"))
            .unwrap();
        assert_eq!(preset.cmd, "mirv_streams settings add ffmpeg demodesk1 \"-c:v libx264 -pix_fmt yuv420p -crf 23 {QUOTE}C:/test-output/r1/1-sequence\\\\video.mp4{QUOTE}\"");
        let xml = mirv_cmd_xml(&s);
        assert!(xml.starts_with(
            "<commandSystem>\n<commands>\n<c tick=\"96.000\">echo [demodesk] seq 1 of 1 seek</c>"
        ));
        assert!(xml.contains("<c tick=\"9936.000\">echo [demodesk] seq 1 of 1 setup</c>"));
    }

    #[test]
    fn clamps_and_chains() {
        let s = build_schedule(
            &[clip(20_000, 21_000, Some(1)), clip(50, 400, Some(1))],
            &opts(&RenderOptions::default()),
        );
        // clips are visited in tick order: the early clip becomes sequence 1
        assert_eq!(s.iter().map(|a| a.tick.floor() as i32).min(), Some(96));
        assert!(s
            .iter()
            .any(|a| a.cmd.contains("/1-sequence") && a.tick < 1000.0));
        assert!(s
            .iter()
            .any(|a| a.cmd.contains("/2-sequence") && a.tick > 19_000.0));
        assert!(s.iter().any(|a| a.cmd == "demo_gototick 19935"));
        assert_eq!(s.iter().filter(|a| a.cmd == "quit").count(), 1);
        let locked = RenderOptions {
            camera: Camera::Lock,
            ..RenderOptions::default()
        };
        let lock = build_schedule(&[clip(10_000, 11_000, Some(1))], &opts(&locked));
        assert!(lock.iter().any(|a| a.cmd == "spec_lock_to_accountid 123"));
    }

    #[test]
    fn overlapping_clips_never_jump_backwards() {
        let s = build_schedule(
            &[clip(10_000, 11_000, Some(1)), clip(10_500, 11_500, Some(1))],
            &opts(&RenderOptions::default()),
        );
        let mut last_goto = 0;
        for a in &s {
            if let Some(t) = a.cmd.strip_prefix("demo_gototick ") {
                let t: i32 = t.parse().unwrap();
                assert!(t > last_goto);
                assert!((t as f64) > a.tick);
                last_goto = t;
            }
        }
    }
}
