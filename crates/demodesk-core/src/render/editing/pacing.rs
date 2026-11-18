//! Shared timing constraints for discrete editing viewpoints (not continuous camera motion).
use super::*;

const MIN_VIEW_SECONDS: f64 = 1.5;
const DEATH_HOLD_SECONDS: f64 = 0.7;
// Demo-rendered deaths trail their event ticks; keep the visible aftermath, too.
const EVENT_RENDER_MARGIN_SECONDS: f64 = 0.125;

pub(in crate::render) fn minimum(rate: i32) -> i32 {
    (f64::from(rate) * MIN_VIEW_SECONDS).ceil() as i32
}
pub(in crate::render) fn event_tail(rate: i32) -> i32 {
    (f64::from(rate) * (DEATH_HOLD_SECONDS + EVENT_RENDER_MARGIN_SECONDS)).ceil() as i32
}

/// Rewind within known bounds instead of leaving a brief cutaway.
pub(in crate::render) fn lead_start(start: i32, end: i32, earliest: i32, rate: i32) -> Option<i32> {
    let start = start.min(end - minimum(rate)).max(earliest);
    (end - start >= minimum(rate)).then_some(start)
}

/// Finish a short continuation before the replay rather than flash back to it afterward.
pub(in crate::render) fn replay_cut(
    requested: i32,
    start: i32,
    end: i32,
    rate: i32,
) -> Option<i32> {
    let cut = requested.max(start + minimum(rate));
    if cut > end {
        return None;
    }
    Some(if end - cut < minimum(rate) { end } else { cut })
}

pub(super) fn validate(plan: &Plan, selected: &[&str], rate: i32) -> Result<()> {
    ensure!(
        plan.recording
            .iter()
            .filter(|h| selected.contains(&h.id.as_str()))
            .all(|h| h.end_tick - h.start_tick >= minimum(rate)),
        "Editing combination leaves a viewpoint shorter than 1.5 seconds"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimum_and_death_hold_scale_with_demo_rate() {
        for (rate, tail) in [(64, 53), (128, 106)] {
            assert_eq!(minimum(rate), rate * 3 / 2);
            assert_eq!(event_tail(rate), tail);
            assert_eq!(
                lead_start(1000, 1032, 700, rate),
                Some(1032 - minimum(rate))
            );
            assert!(lead_start(1000, 1032, 1000, rate).is_none());
            assert_eq!(
                lead_start(1000, 1000 + 3 * rate, 700, rate),
                Some(1000),
                "longer views retain their full duration"
            );
        }
    }
    #[test]
    fn replay_absorbs_short_tail_without_rewinding_original_pov() {
        assert_eq!(replay_cut(1032, 800, 1035, 64), Some(1035));
        assert_eq!(replay_cut(1032, 800, 1128, 64), Some(1032));
        assert_eq!(replay_cut(830, 800, 1100, 64), Some(896));
        assert_eq!(replay_cut(830, 800, 860, 64), None);
    }
}
