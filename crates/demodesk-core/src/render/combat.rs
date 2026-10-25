use crate::model::Highlight;
use anyhow::{ensure, Result};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

pub type CombatTicks = BTreeMap<String, Vec<i32>>;

/// Read only the event fields from an existing replay; frames are not retained.
pub fn cached_ticks(path: &Path, tick_rate: f64) -> Result<CombatTicks> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Events {
        tick_rate: f64,
        players: Vec<crate::replay::ReplayPlayer>,
        events: Vec<crate::replay::ReplayEvent>,
    }
    let replay: Events =
        serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
    ensure!(
        tick_rate.is_finite() && tick_rate > 0.0 && replay.tick_rate == tick_rate,
        "replay tick rate mismatch"
    );
    let mut ticks = CombatTicks::new();
    for event in replay.events {
        let ids = match event.k.as_str() {
            "shot" => [event.p, None],
            "death" => [event.p, event.a],
            _ => continue,
        };
        for id in ids.into_iter().flatten() {
            if let Some(player) = usize::try_from(id).ok().and_then(|i| replay.players.get(i)) {
                ticks
                    .entry(player.steamid.clone())
                    .or_default()
                    .push(event.t);
            }
        }
    }
    for values in ticks.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    Ok(ticks)
}

/// Map a recorded POV window into its output file, clipping event padding at cuts.
pub fn windows(h: &Highlight, ticks: &CombatTicks, tick_rate: f64, seconds: f64) -> Vec<[f64; 2]> {
    if !tick_rate.is_finite() || tick_rate <= 0.0 || !seconds.is_finite() || seconds <= 0.0 {
        return vec![];
    }
    let mut spans = Vec::new();
    let switch = h
        .round_result
        .as_ref()
        .map(|v| v.from_tick)
        .unwrap_or(h.end_tick);
    let mut views = vec![(&h.player.steamid, h.start_tick, switch.min(h.end_tick))];
    if let Some(view) = &h.round_result {
        views.push((&view.player, view.from_tick.max(h.start_tick), h.end_tick));
    }
    for (player, start, end) in views {
        if end <= start {
            continue;
        }
        if let Some(events) = ticks.get(player) {
            for &tick in events {
                if tick < start || tick > end {
                    continue;
                }
                let t = (f64::from(tick) - f64::from(h.start_tick)) / tick_rate;
                let a = (t - 1.5)
                    .max((f64::from(start) - f64::from(h.start_tick)) / tick_rate)
                    .max(0.0);
                let b = (t + 0.75)
                    .min((f64::from(end) - f64::from(h.start_tick)) / tick_rate)
                    .min(seconds);
                if b > a {
                    spans.push([a, b]);
                }
            }
        }
    }
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f64; 2]> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span[0] <= last[1] + 0.5 {
                last[1] = last[1].max(span[1]);
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_trimmed_parts_without_using_demo_absolute_seconds() {
        let h: Highlight = serde_json::from_value(serde_json::json!({
            "id":"a","player":{"steamid":"p","name":"p"},"round":1,
            "startTick":640,"endTick":1280,"anchorTick":900,"score":1,"tags":[],"title":"a","kills":[],"breakdown":{}
        })).unwrap();
        let ticks = BTreeMap::from([
            ("p".into(), vec![100, 704, 768, 1216, 2000]),
            ("other".into(), vec![960]),
        ]);
        assert_eq!(
            windows(&h, &ticks, 64.0, 10.0),
            vec![[0.0, 2.75], [7.5, 9.75]]
        );
        assert!(windows(&h, &ticks, f64::NAN, 10.0).is_empty());
        let mut switched = h;
        switched.round_result = Some(crate::model::RoundResultView {
            from_tick: 960,
            player: "other".into(),
        });
        assert_eq!(
            windows(&switched, &ticks, 64.0, 10.0),
            vec![[0.0, 2.75], [5.0, 5.75]]
        );
    }
}
