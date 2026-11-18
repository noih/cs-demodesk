//! Offline editing analysis. Rendering consumes the saved, selected plan.
use super::*;
use anyhow::ensure;
use std::collections::HashSet;

mod context;
mod features;
pub(super) mod pacing;
#[cfg(test)]
mod tests;
pub(super) mod timeline;

pub const VERSION: u32 = 9;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayLink {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Opening,
    Combat,
    Teamwork,
    Push,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub highlight_id: String,
    pub kind: Kind,
    pub tick: i32,
    pub target: String,
    pub target_id: String,
    pub recommended: bool,
    pub conflicts: Vec<String>,
    pub added_seconds: f64,
    pub replay_group: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub candidates: Vec<Candidate>,
    pub cached: bool,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewShot {
    pub highlight_id: String,
    pub player: String,
    pub kind: &'static str,
    pub start_tick: i32,
    pub end_tick: i32,
}

#[derive(Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    pub id: String,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
    originals: Vec<Highlight>,
    recording: Vec<Highlight>,
    features: Vec<features::Prepared>,
}

#[derive(Serialize, Deserialize)]
pub struct Plan {
    pub(super) recording: Vec<Highlight>,
    pub(super) utilities: Vec<Option<utility::Camera>>,
    pub(super) openings: Vec<Option<opening::Opening>>,
    pub(super) teams: Vec<team_push::Sequence>,
}

fn candidate(id: &str, kind: Kind, tick: i32, target: String, recommended: bool) -> Candidate {
    Candidate {
        id: format!("{id}:{kind:?}:{tick}"),
        highlight_id: id.into(),
        kind,
        tick,
        target,
        target_id: String::new(),
        recommended,
        conflicts: vec![],
        added_seconds: 0.0,
        replay_group: None,
    }
}

impl Analysis {
    pub fn summary(&self, cached: bool) -> Summary {
        Summary {
            id: self.id.clone(),
            candidates: self.candidates.clone(),
            cached,
            warnings: self.warnings.clone(),
        }
    }

    pub fn prepare(
        id: String,
        originals: Vec<Highlight>,
        demo: &DemoInfo,
        options: &RenderOptions,
        tools: &ToolPaths,
        root: &Path,
    ) -> Result<Self> {
        Self::prepare_features(
            id,
            originals,
            demo,
            options,
            tools,
            root,
            features::REGISTRY,
            &AtomicBool::new(false),
        )
    }

    fn prepare_features(
        id: String,
        originals: Vec<Highlight>,
        demo: &DemoInfo,
        options: &RenderOptions,
        tools: &ToolPaths,
        root: &Path,
        registry: &[features::Registration],
        cancel: &AtomicBool,
    ) -> Result<Self> {
        let recording = merge_nearby_clips(
            recording_windows(
                &originals,
                demo.tick_rate,
                options.key_moments_only,
                options.short_combat_clips,
            ),
            demo.tick_rate,
        );
        let ctx = context::Context::load(
            demo,
            &originals,
            &recording,
            tools,
            root,
            registry.iter().any(|f| f.spatial),
            registry.iter().any(|f| f.kind == Kind::Teamwork),
            cancel,
        )?;
        Self::analyze_context(id, ctx, registry)
    }

    fn analyze_context(
        id: String,
        ctx: context::Context<'_>,
        registry: &[features::Registration],
    ) -> Result<Self> {
        let demo = ctx.demo;
        let originals = ctx.originals.to_vec();
        let recording = ctx.recording.to_vec();
        let mut prepared = vec![];
        let mut warnings = vec![];
        let mut offers = vec![];
        for feature in registry {
            Control::new(ctx.cancel, &mut |_| {}).check_cancelled()?;
            let detected = (feature.analyze)(&ctx);
            ensure!(
                detected.data.feature().kind() == feature.kind
                    && detected
                        .offers
                        .iter()
                        .all(|o| o.candidate.kind == feature.kind),
                "Feature returned an invalid kind"
            );
            prepared.push(detected.data);
            warnings.extend(detected.warnings);
            offers.extend(detected.offers);
        }
        Control::new(ctx.cancel, &mut |_| {}).check_cancelled()?;
        // Stable discovery order within each highlight; independently ordered compilation.
        offers.sort_by_key(|o| {
            originals
                .iter()
                .position(|h| h.id == o.candidate.highlight_id)
                .unwrap_or(usize::MAX)
        });
        resolve_claims(&mut offers);
        prepared.sort_by_key(|f| f.feature().phase());
        let mut analysis = Self {
            version: VERSION,
            id,
            candidates: offers.into_iter().map(|o| o.candidate).collect(),
            warnings,
            originals,
            recording,
            features: prepared,
        };
        // Validate actual compositions before offering them, including camera overlaps.
        let valid: HashSet<_> = analysis
            .candidates
            .iter()
            .filter(|c| analysis.select(std::slice::from_ref(&c.id), demo).is_ok())
            .map(|c| c.id.clone())
            .collect();
        analysis.candidates.retain(|c| valid.contains(&c.id));
        for c in &mut analysis.candidates {
            c.conflicts.retain(|id| valid.contains(id));
        }
        let base: i64 = analysis
            .recording
            .iter()
            .map(|h| i64::from(h.end_tick) - i64::from(h.start_tick))
            .sum();
        for i in 0..analysis.candidates.len() {
            Control::new(ctx.cancel, &mut |_| {}).check_cancelled()?;
            let plan = analysis.select(std::slice::from_ref(&analysis.candidates[i].id), demo)?;
            let ticks: i64 = plan
                .recording
                .iter()
                .map(|h| i64::from(h.end_tick) - i64::from(h.start_tick))
                .sum();
            analysis.candidates[i].added_seconds = (ticks - base).max(0) as f64 / demo.tick_rate;
        }
        for i in 0..analysis.candidates.len() {
            for j in i + 1..analysis.candidates.len() {
                Control::new(ctx.cancel, &mut |_| {}).check_cancelled()?;
                if analysis.candidates[i].highlight_id != analysis.candidates[j].highlight_id {
                    continue;
                }
                let ids = vec![
                    analysis.candidates[i].id.clone(),
                    analysis.candidates[j].id.clone(),
                ];
                if analysis.select(&ids, demo).is_err()
                    && !analysis.candidates[i].conflicts.contains(&ids[1])
                {
                    analysis.candidates[i].conflicts.push(ids[1].clone());
                    analysis.candidates[j].conflicts.push(ids[0].clone());
                }
            }
        }
        let mut defaults = vec![];
        for i in 0..analysis.candidates.len() {
            if !analysis.candidates[i].recommended {
                continue;
            }
            defaults.push(analysis.candidates[i].id.clone());
            if analysis.select(&defaults, demo).is_err() {
                defaults.pop();
                analysis.candidates[i].recommended = false;
            }
        }
        Ok(analysis)
    }

    pub fn select(&self, ids: &[String], demo: &DemoInfo) -> Result<Plan> {
        let links = features::combat::recommended_links(&self.candidates, ids);
        self.select_with_links(ids, &links, demo)
    }

    pub fn select_with_links(
        &self,
        ids: &[String],
        links: &[ReplayLink],
        demo: &DemoInfo,
    ) -> Result<Plan> {
        let selected: HashSet<_> = ids.iter().map(String::as_str).collect();
        ensure!(
            self.version == VERSION && selected.len() == ids.len(),
            "Invalid editing selection; analyze again"
        );
        ensure!(
            ids.iter()
                .all(|id| self.candidates.iter().any(|c| c.id == *id)),
            "Unknown editing candidate; analyze again"
        );
        ensure!(
            !self
                .candidates
                .iter()
                .filter(|c| selected.contains(c.id.as_str()))
                .any(|c| c.conflicts.iter().any(|id| selected.contains(id.as_str()))),
            "Conflicting editing selections"
        );
        ensure!(
            self.candidates
                .iter()
                .filter(|c| selected.contains(c.id.as_str()))
                .all(|c| self.features.iter().any(|f| f.feature().kind() == c.kind)),
            "Missing prepared feature; analyze again"
        );
        let mut plan = Plan::plain(self.recording.clone());
        features::combat::validate_links(&self.candidates, ids, links)?;
        let context = context::Composition {
            demo,
            originals: &self.originals,
            replay_links: links,
        };
        for feature in &self.features {
            let feature = feature.feature();
            let choices: Vec<_> = self
                .candidates
                .iter()
                .filter(|c| c.kind == feature.kind() && selected.contains(c.id.as_str()))
                .collect();
            feature.compose(&choices, &context, &mut plan)?;
        }
        // Features validate after all timeline transformations, so later edits cannot silently remove an earlier selection.
        for feature in &self.features {
            let feature = feature.feature();
            let choices: Vec<_> = self
                .candidates
                .iter()
                .filter(|c| c.kind == feature.kind() && selected.contains(c.id.as_str()))
                .collect();
            feature.validate(&choices, &plan)?;
        }
        plan.validate()?;
        let edited: Vec<_> = self
            .candidates
            .iter()
            .filter(|c| selected.contains(c.id.as_str()))
            .map(|c| c.highlight_id.as_str())
            .collect();
        pacing::validate(&plan, &edited, demo.tick_rate.round() as i32)?;
        Ok(plan)
    }

    /// Non-interactive callers use the same pipeline, restricted to requested features.
    pub(super) fn defaults(
        originals: Vec<Highlight>,
        demo: &DemoInfo,
        options: &RenderOptions,
        tools: &ToolPaths,
        root: &Path,
        cancel: &AtomicBool,
    ) -> Result<Plan> {
        let registry: Vec<_> = features::REGISTRY
            .iter()
            .filter(|f| (f.enabled)(options))
            .copied()
            .collect();
        let analysis = Self::prepare_features(
            String::new(),
            originals,
            demo,
            options,
            tools,
            root,
            &registry,
            cancel,
        )?;
        let selected = analysis
            .candidates
            .iter()
            .filter(|c| c.recommended)
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        analysis.select(&selected, demo)
    }
}

impl Plan {
    pub fn preview(&self, originals: &[Highlight]) -> Vec<PreviewShot> {
        self.recording
            .iter()
            .zip(&self.openings)
            .map(|(h, opening)| PreviewShot {
                highlight_id: h.id.clone(),
                player: h.player.name.clone(),
                kind: if opening.is_some() {
                    "opening"
                } else if originals
                    .iter()
                    .any(|o| o.id == h.id && o.player.steamid == h.player.steamid)
                {
                    "main"
                } else {
                    "perspective"
                },
                start_tick: h.start_tick,
                end_tick: h.end_tick,
            })
            .collect()
    }

    pub(super) fn plain(recording: Vec<Highlight>) -> Self {
        Self {
            utilities: vec![None; recording.len()],
            openings: vec![None; recording.len()],
            recording,
            teams: vec![],
        }
    }
    /// Story and replay phases run before introductions; keep renderer arrays aligned.
    fn replace_story(
        &mut self,
        recording: Vec<Highlight>,
        utilities: Vec<Option<utility::Camera>>,
    ) {
        self.openings = vec![None; recording.len()];
        self.recording = recording;
        self.utilities = utilities;
    }
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.recording.len() == self.utilities.len()
                && self.recording.len() == self.openings.len(),
            "Invalid saved editing plan"
        );
        Ok(())
    }
}

fn resolve_claims(offers: &mut [features::Offer]) {
    for i in 0..offers.len() {
        for j in i + 1..offers.len() {
            let (a, b) = (&offers[i], &offers[j]);
            // A feature owns its internal sequencing. Claims reserve resources against other features.
            if a.candidate.highlight_id != b.candidate.highlight_id
                || a.candidate.kind == b.candidate.kind
                || !a
                    .claims
                    .iter()
                    .any(|x| b.claims.iter().any(|y| x.overlaps(y)))
            {
                continue;
            }
            let (left, right) = (a.candidate.id.clone(), b.candidate.id.clone());
            let lower = if a.priority < b.priority {
                Some(i)
            } else if b.priority < a.priority {
                Some(j)
            } else {
                None
            };
            offers[i].candidate.conflicts.push(right);
            offers[j].candidate.conflicts.push(left);
            if let Some(lower) = lower {
                offers[lower].candidate.recommended = false;
            }
        }
    }
}

pub(super) fn requested(options: &RenderOptions) -> bool {
    features::REGISTRY.iter().any(|f| (f.enabled)(options))
}
