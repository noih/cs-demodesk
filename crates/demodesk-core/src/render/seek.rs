//! Every recorded seek uses this reset/preroll/pause protocol. Demo ticks cannot
//! implement the final delay: playback is paused, so the gate uses wall time.
use super::actions::MARK;
use crate::ErrorCode;
use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};

pub(super) const MIN_TICK: i32 = 96;
const PREROLL_SECONDS: i32 = 3;
const SETTLE_TIME: Duration = Duration::from_secs(3);

pub(super) fn waiting_message(index: usize, total: usize) -> String {
    format!(
        "clip {index}/{total}: waiting {} seconds before recording",
        SETTLE_TIME.as_secs()
    )
}

pub(super) fn minimum_gap(rate: i32) -> i64 {
    i64::from(rate) * i64::from(PREROLL_SECONDS) + i64::from(rate / 2) + 2
}

pub(super) struct SeekWindow {
    pub setup_tick: i32,
    pub start_tick: i32,
    pub end_tick: i32,
    jump_from: i32,
}

impl SeekWindow {
    pub fn new(start: i32, end: i32, previous_end: Option<i32>, rate: i32) -> Self {
        let mut setup_tick = (start - rate * PREROLL_SECONDS).max(MIN_TICK + 4);
        if let Some(previous) = previous_end {
            setup_tick = setup_tick.max(previous + rate / 2);
        }
        let start_tick = start.max(setup_tick + 2);
        Self {
            setup_tick,
            start_tick,
            end_tick: end.max(start_tick + 1),
            jump_from: previous_end.map_or(MIN_TICK, |previous| previous + rate / 4),
        }
    }

    /// Schedule the entire protected seek, returning the next setup command slot.
    /// Recording stays gated even if the clip is too early for a full preroll.
    pub fn schedule(
        &self,
        index: usize,
        total: usize,
        push: &mut impl FnMut(i32, &mut u32, String),
    ) -> u32 {
        let mut slot = 0;
        push(
            self.jump_from,
            &mut slot,
            format!("echo {MARK} seq {index} of {total} seek"),
        );
        if self.setup_tick - 1 > self.jump_from + 1 {
            push(
                self.jump_from,
                &mut slot,
                format!("demo_gototick {}", self.setup_tick - 1),
            );
        }
        slot = 0;
        for command in [
            format!("echo {MARK} seq {index} of {total} setup"),
            "sv_cheats 1".into(),
            "snd_soundevent_clear_deferred".into(),
            "snd_sos_stop_all_soundevents".into(),
            "demo_timescale 1".into(),
            format!("echo {MARK} seq {index} of {total} preroll"),
            format!("alias demodesk_wait_{index} \"demo_pause; echo {MARK} seq {index} of {total} settle\""),
        ] { push(self.setup_tick, &mut slot, command); }
        let mut start_slot = 0;
        push(
            self.start_tick,
            &mut start_slot,
            format!("demodesk_wait_{index}"),
        );
        slot
    }
}

#[derive(Default)]
pub(super) struct RecordingGate {
    pending: Option<(Instant, usize, usize)>,
    last_settled_clip: usize,
}

impl RecordingGate {
    /// Returns true only when a new clip starts its real-time wait.
    pub fn observe(
        &mut self,
        index: usize,
        total: usize,
        marker: &str,
        now: Instant,
    ) -> Result<bool> {
        if self.pending.is_some() && matches!(marker, "end" | "seek" | "start") {
            return Err(anyhow!("clip {index}/{total}: demo advanced past the recording boundary while waiting for audio").context(ErrorCode::RecordingFailed));
        }
        if marker != "settle" || index <= self.last_settled_clip {
            return Ok(false);
        }
        self.last_settled_clip = index;
        self.pending = Some((now + SETTLE_TIME, index, total));
        Ok(true)
    }

    pub fn take_recording_due(&mut self, now: Instant) -> Option<String> {
        let (deadline, index, total) = self.pending?;
        if now < deadline {
            return None;
        }
        self.pending = None;
        Some(format!("alias demodesk_wait_{index} \"\"; demo_resume; mirv_streams record start; echo {MARK} seq {index} of {total} start"))
    }

    #[cfg(test)]
    pub fn pending(&self) -> Option<(Instant, usize, usize)> {
        self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_settle_resumes_once_after_three_real_seconds() {
        let now = Instant::now();
        let mut gate = RecordingGate::default();
        assert_eq!(gate.take_recording_due(now), None);
        assert!(gate.observe(2, 3, "settle", now).unwrap());
        assert!(!gate
            .observe(2, 3, "settle", now + Duration::from_secs(1))
            .unwrap());
        assert_eq!(
            gate.take_recording_due(now + Duration::from_millis(2999)),
            None
        );
        assert_eq!(gate.take_recording_due(now + Duration::from_secs(3)).as_deref(), Some("alias demodesk_wait_2 \"\"; demo_resume; mirv_streams record start; echo [demodesk] seq 2 of 3 start"));
        assert_eq!(gate.take_recording_due(now + Duration::from_secs(4)), None);
        assert!(!gate
            .observe(2, 3, "settle", now + Duration::from_secs(4))
            .unwrap());
        assert_eq!(gate.pending(), None);
    }

    #[test]
    fn unsafe_progress_during_wait_is_rejected() {
        for marker in ["seek", "start", "end"] {
            let mut gate = RecordingGate::default();
            gate.observe(1, 2, "settle", Instant::now()).unwrap();
            assert!(gate.observe(1, 2, marker, Instant::now()).is_err());
        }
    }
}
