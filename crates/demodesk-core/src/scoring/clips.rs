//! Convert saved observations into padded, per-rule video selections.
use super::{Assessment, State};
use crate::model::{DemoInfo, Highlight, HighlightPlayer};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub player_id: String,
    pub assessment_id: String,
    pub rule_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleClips {
    pub rule_id: String,
    pub title: String,
    pub demo_fingerprint: String,
    pub highlights: Vec<Highlight>,
}

pub fn build(
    record: &Assessment,
    demo: &DemoInfo,
    end_tick: i32,
    rules: &[String],
) -> Result<Vec<RuleClips>> {
    ensure!(
        demo.tick_rate.is_finite() && demo.tick_rate > 0.0 && demo.tick_rate <= 1024.0,
        "invalid demo tick rate"
    );
    ensure!(
        (record.tick_rate - demo.tick_rate).abs() < 0.001,
        "analysis tick rate differs from demo; analyze again"
    );
    let player = demo
        .players
        .iter()
        .find(|p| p.steamid == record.player_id)
        .ok_or_else(|| anyhow::anyhow!("analysis player not found"))?;
    ensure!(
        !rules.is_empty() && rules.len() <= record.checks.len(),
        "select observed rules"
    );
    let outer = (3.0 * demo.tick_rate).ceil() as i32;
    let inner = (1.5 * demo.tick_rate).ceil() as i32;
    let mut seen = HashSet::new();
    let mut groups = Vec::new();
    for id in rules {
        if !seen.insert(id) {
            continue;
        }
        let check = record
            .checks
            .iter()
            .find(|c| c.definition.id == *id)
            .ok_or_else(|| anyhow::anyhow!("unknown rule: {id}"))?;
        ensure!(
            !matches!(check.state, State::Unavailable | State::Failed)
                && !check.occurrences.is_empty(),
            "rule has no observed clips: {id}"
        );
        let mut events = check.occurrences.iter().collect::<Vec<_>>();
        events.sort_by_key(|e| e.start_tick);
        let title = format!("{} — {}", player.name, check.definition.name);
        let mut highlights: Vec<Highlight> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            ensure!(
                event.start_tick >= 0
                    && event.end_tick >= event.start_tick
                    && event.end_tick <= end_tick,
                "invalid occurrence time; analyze again"
            );
            let start = event
                .start_tick
                .saturating_sub(if index == 0 { outer } else { inner })
                .max(0);
            let end = event
                .end_tick
                .saturating_add(if index + 1 == events.len() {
                    outer
                } else {
                    inner
                })
                .min(end_tick);
            if let Some(last) = highlights.last_mut() {
                if last.round == event.round && start <= last.end_tick {
                    last.end_tick = last.end_tick.max(end);
                    continue;
                }
            }
            highlights.push(Highlight {
                id: format!("{}-{}-{}", record.id, id, highlights.len()),
                player: HighlightPlayer {
                    steamid: player.steamid.clone(),
                    name: player.name.clone(),
                },
                round: event.round,
                start_tick: start,
                end_tick: end,
                anchor_tick: event.start_tick,
                score: 0.0,
                tags: vec![id.clone()],
                title: format!("{title} · R{}", event.round),
                kills: vec![],
                breakdown: Default::default(),
            });
        }
        groups.push(RuleClips {
            rule_id: id.clone(),
            title,
            demo_fingerprint: record.demo_fingerprint.clone(),
            highlights,
        });
    }
    Ok(groups)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn fixture() -> (Assessment, DemoInfo) {
        let check = |id: &str, events: Vec<(i32, i32)>| json!({"definition":{"id":id,"version":"1","name":id,"description":"","category":"movement","parameters":{}},"state":"findings","reason":"","reasonCode":"","evaluatedSamples":6,"findings":[],"observations":[],"summary":[],"diagnostics":null,"occurrences":events.into_iter().enumerate().map(|(i,(start,end))|json!({"id":i.to_string(),"round":1,"startTick":start,"endTick":end,"targetId":"","sourceIds":[],"measurements":[]})).collect::<Vec<_>>()});
        let record=serde_json::from_value(json!({"schemaVersion":2,"id":"test","createdAt":"","demoId":"demo","demoFingerprint":"sha1:test","playerId":"1","tickRate":64.0,"rulesetVersion":"test","checks":[check("jump",(1..=6).map(|i|(i*640,i*640+64)).collect()),check("view",vec![(640,704),(768,832)])],"inputProvenance":{},"state":"findings"})).unwrap();
        let demo=serde_json::from_value(json!({"path":"test.dem","mapName":"test","serverName":"","tickRate":64.0,"players":[{"name":"Player","steamid":"1","teamNumber":2,"userId":4}]})).unwrap();
        (record, demo)
    }
    #[test]
    fn groups_rules_pads_edges_and_keeps_selected_player_pov() {
        let (record, demo) = fixture();
        let groups = build(&record, &demo, 6400, &["jump".into(), "view".into()]).unwrap();
        assert_eq!(groups.len(), 2);
        let clips = &groups[0].highlights;
        assert_eq!(clips.len(), 6);
        assert_eq!((clips[0].start_tick, clips[0].end_tick), (448, 800));
        assert_eq!((clips[1].start_tick, clips[1].end_tick), (1184, 1440));
        assert_eq!((clips[5].start_tick, clips[5].end_tick), (3744, 4096));
        assert_eq!(groups[1].highlights.len(), 1);
        assert_eq!(
            (
                groups[1].highlights[0].start_tick,
                groups[1].highlights[0].end_tick
            ),
            (448, 1024)
        );
        let mut render_demo=demo.clone();
        let mut render_clips=clips.clone();
        let synthetic_id=0x0110000100000001u64.to_string();
        render_demo.players[0].steamid=synthetic_id.clone();
        for clip in &mut render_clips {clip.player.steamid=synthetic_id.clone();}
        let plan = crate::render::to_render_clips(&render_demo, &render_clips);
        assert!(plan
            .iter()
            .all(|c| c.highlight.player.steamid == synthetic_id && c.slot == Some(5)));
        let root = tempfile::tempdir().unwrap();
        let store = crate::store::Store::open(root.path().into()).unwrap();
        let mut job = store
            .new_job(
                "demo",
                clips.iter().map(|h| h.id.clone()).collect(),
                crate::render::RenderOptions {
                    merge: true,
                    ..Default::default()
                },
            )
            .unwrap();
        job.analysis_clips = Some(Box::new(groups[0].clone()));
        store.save_job(&job).unwrap();
        let saved = store.get_job(&job.id).unwrap();
        assert!(saved.options.merge);
        assert_eq!(saved.analysis_clips.unwrap().highlights.len(), 6);
        assert!(build(&record, &demo, 6400, &["unknown".into()]).is_err());
    }
    #[test]
    fn clamps_recording_boundaries_and_rejects_invalid_intervals() {
        let (mut record, demo) = fixture();
        record.checks[0].occurrences.truncate(1);
        record.checks[0].occurrences[0].start_tick = 0;
        record.checks[0].occurrences[0].end_tick = 64;
        let groups = build(&record, &demo, 128, &["jump".into()]).unwrap();
        assert_eq!(
            (
                groups[0].highlights[0].start_tick,
                groups[0].highlights[0].end_tick
            ),
            (0, 128)
        );
        record.checks[0].occurrences[0].end_tick = -1;
        assert!(build(&record, &demo, 128, &["jump".into()]).is_err());
    }
}
