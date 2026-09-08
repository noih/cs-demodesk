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
//!    execute nothing in mirv_cmd, so overlapping clips are simply pushed later
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
    steamid64.parse::<u64>().ok().map(|v| (v - 76561197960265728).to_string())
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
        out.push(Scheduled { tick: tick.max(MIN_TICK) as f64 + (*slot as f64) * 0.001, cmd });
        *slot += 1;
    };

    let mut prev_end: Option<i32> = None;
    let n = order.len();
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
        push(jump_from, &mut slot, format!("echo {MARK} seq {} of {n} seek", seq + 1));
        if setup_tick - 1 > jump_from + 1 {
            push(jump_from, &mut slot, format!("demo_gototick {}", setup_tick - 1));
        }

        // 2. Baseline + recording configuration at the setup tick.
        let mut slot = 0;
        push(setup_tick, &mut slot, format!("echo {MARK} seq {} of {n} setup", seq + 1));
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
        push(setup_tick, &mut slot, format!("cl_demo_predict {}", r.true_view as u8));
        // HUD: cl_drawhud must stay on for any element; without the main HUD we go through
        // cl_draw_only_deathnotices and force radar / kill feed on or off individually.
        push(setup_tick, &mut slot, format!("cl_drawhud {}", (r.hud || r.radar || r.kill_feed) as u8));
        push(setup_tick, &mut slot, format!("cl_draw_only_deathnotices {}", (!r.hud) as u8));
        push(setup_tick, &mut slot, format!("crosshair {}", r.crosshair as u8));
        push(setup_tick, &mut slot, "cl_show_observer_crosshair 2".into());
        let force = |on: bool| if on { 1 } else { -1 };
        push(setup_tick, &mut slot, format!("cl_drawhud_force_radar {}", force(r.radar)));
        push(setup_tick, &mut slot, format!("cl_drawhud_force_deathnotices {}", force(r.kill_feed)));
        // teammate names / equipment over heads: never (clutter, and it is not the player's own view)
        push(setup_tick, &mut slot, "cl_drawhud_force_teamid_overhead -1".into());
        push(setup_tick, &mut slot, format!("cl_chatfilters {}", if r.chat { 63 } else { 0 }));
        push(setup_tick, &mut slot, format!("r_drawviewmodel {}", r.viewmodel as u8));
        push(setup_tick, &mut slot, format!("r_drawtracers_firstperson {}", r.tracers as u8));
        push(setup_tick, &mut slot, format!("hud_scaling {:.2}", r.hud_scale.clamp(0.5, 0.95)));
        push(setup_tick, &mut slot, format!("mirv_deathmsg lifetime {}", r.death_notice_seconds));
        push(setup_tick, &mut slot, "mirv_deathmsg filter clear".into());
        push(setup_tick, &mut slot, "mirv_deathmsg clear".into());
        push(setup_tick, &mut slot, format!("tv_listen_voice_indices {}", if r.voice { -1 } else { 0 }));
        push(setup_tick, &mut slot, format!("tv_listen_voice_indices_h {}", if r.voice { -1 } else { 0 }));
        push(setup_tick, &mut slot, "mirv_streams record startMovieWav 1".into());
        push(setup_tick, &mut slot, format!("mirv_streams record name \"{folder}\""));
        push(setup_tick, &mut slot, format!("spec_show_xray {}", r.xray as u8));
        push(setup_tick, &mut slot, "mp_display_kill_assists 1".into());
        // HLAE unescapes the doubled backslash before the file name; {QUOTE} is its quote placeholder.
        push(setup_tick, &mut slot, format!("mirv_streams settings add ffmpeg {preset} \"{} {{QUOTE}}{folder}\\\\video.{}{{QUOTE}}\"", o.ffmpeg_preset, r.container));
        push(setup_tick, &mut slot, format!("mirv_streams record screen settings {preset}"));
        push(setup_tick, &mut slot, format!("mirv_streams record fps {}", r.fps));

        // 3. Camera on the highlighted player (after the seek, before recording).
        push(setup_tick, &mut slot, "spec_mode 1".into());
        match (r.camera, &clip.account_id, clip.slot) {
            (Camera::Lock, Some(acc), _) => push(setup_tick, &mut slot, format!("spec_lock_to_accountid {acc}")),
            (_, _, Some(slot_no)) => push(setup_tick, &mut slot, format!("spec_player {slot_no}")),
            _ => {}
        }

        // 4. Record.
        let mut slot = 0;
        push(start_tick, &mut slot, format!("echo {MARK} seq {} of {n} start", seq + 1));
        push(start_tick, &mut slot, "mirv_streams record start".into());
        let mut slot = 0;
        push(end_tick, &mut slot, "mirv_streams record end".into());
        push(end_tick, &mut slot, format!("echo {MARK} seq {} of {n} end", seq + 1));

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
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `mirv_cmd load` file: `<commandSystem><commands><c tick="…">cmd</c>…`.
/// Kept sorted so HLAE's (degenerate) interval tree visits them in order.
pub fn mirv_cmd_xml(schedule: &[Scheduled]) -> String {
    let mut s = String::from("<commandSystem>\n<commands>\n");
    for c in schedule {
        s.push_str(&format!("<c tick=\"{:.3}\">{}</c>\n", c.tick, xml_escape(&c.cmd)));
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
        ActionsOptions { render, tick_rate: 64.0, output_dir: "C:/test-output/r1".into(), ffmpeg_preset: "-c:v libx264 -pix_fmt yuv420p -crf 23".into() }
    }

    fn clip(start: i32, end: i32, slot: Option<i32>) -> RenderClip {
        RenderClip {
            highlight: Highlight {
                id: "h".into(),
                player: HighlightPlayer { steamid: (76561197960265728_u64 + 123).to_string(), name: "Player A".into() },
                round: 3,
                start_tick: start,
                end_tick: end,
                anchor_tick: start,
                score: 5.0,
                tags: vec![],
                title: "x".into(),
                kills: vec![],
                breakdown: BTreeMap::new(),
            },
            slot,
            account_id: steamid_to_account_id(&(76561197960265728_u64 + 123).to_string()),
        }
    }

    fn tick_of(sched: &[Scheduled], cmd: &str) -> i32 {
        sched.iter().find(|a| a.cmd == cmd || a.cmd.starts_with(&format!("{cmd} "))).unwrap().tick.floor() as i32
    }
    fn idx(sched: &[Scheduled], cmd: &str) -> usize {
        sched.iter().position(|a| a.cmd == cmd || a.cmd.starts_with(&format!("{cmd} "))).unwrap()
    }

    #[test]
    fn every_clip_keeps_game_audio_for_recording() {
        let clips = [clip(10_000, 11_000, Some(3)), clip(20_000, 21_000, Some(3))];
        for show_game in [false, true] {
            let render = RenderOptions { show_game, ..RenderOptions::default() };
            let schedule = build_schedule(&clips, &opts(&render));
            let volumes: Vec<_> = schedule.iter().filter(|a| a.cmd.starts_with("volume ")).collect();
            let starts: Vec<_> = schedule.iter().filter(|a| a.cmd == "mirv_streams record start").collect();
            assert_eq!(volumes.len(), clips.len());
            for (volume, start) in volumes.iter().zip(starts) {
                assert_eq!(volume.cmd, "volume 1");
                assert!(volume.tick < start.tick);
            }
        }
    }

    #[test]
    fn schedule_is_ordered() {
        let s = build_schedule(&[clip(10_000, 11_000, Some(3))], &opts(&RenderOptions::default()));
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
        let preset = s.iter().find(|a| a.cmd.starts_with("mirv_streams settings add ffmpeg")).unwrap();
        assert_eq!(preset.cmd, "mirv_streams settings add ffmpeg demodesk1 \"-c:v libx264 -pix_fmt yuv420p -crf 23 {QUOTE}C:/test-output/r1/1-sequence\\\\video.mp4{QUOTE}\"");
        let xml = mirv_cmd_xml(&s);
        assert!(xml.starts_with("<commandSystem>\n<commands>\n<c tick=\"96.000\">echo [demodesk] seq 1 of 1 seek</c>"));
        assert!(xml.contains("<c tick=\"9936.000\">echo [demodesk] seq 1 of 1 setup</c>"));
    }

    #[test]
    fn clamps_and_chains() {
        let s = build_schedule(&[clip(20_000, 21_000, Some(1)), clip(50, 400, Some(1))], &opts(&RenderOptions::default()));
        // clips are visited in tick order: the early clip becomes sequence 1
        assert_eq!(s.iter().map(|a| a.tick.floor() as i32).min(), Some(96));
        assert!(s.iter().any(|a| a.cmd.contains("/1-sequence") && a.tick < 1000.0));
        assert!(s.iter().any(|a| a.cmd.contains("/2-sequence") && a.tick > 19_000.0));
        assert!(s.iter().any(|a| a.cmd == "demo_gototick 19935"));
        assert_eq!(s.iter().filter(|a| a.cmd == "quit").count(), 1);
        let locked = RenderOptions { camera: Camera::Lock, ..RenderOptions::default() };
        let lock = build_schedule(&[clip(10_000, 11_000, Some(1))], &opts(&locked));
        assert!(lock.iter().any(|a| a.cmd == "spec_lock_to_accountid 123"));
    }

    #[test]
    fn overlapping_clips_never_jump_backwards() {
        let s = build_schedule(&[clip(10_000, 11_000, Some(1)), clip(10_500, 11_500, Some(1))], &opts(&RenderOptions::default()));
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
