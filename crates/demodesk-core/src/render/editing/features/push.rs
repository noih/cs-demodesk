use super::*;

#[derive(Serialize, Deserialize)]
pub(in crate::render::editing) struct Prepared {
    expanded: Vec<Highlight>,
    sequences: Vec<team_push::Sequence>,
}
pub(super) fn analyze(ctx: &Context<'_>) -> Detected {
    let mut expanded = ctx.recording.to_vec();
    let mut warnings = vec![];
    let sequences = if let Some(spatial) = &ctx.spatial {
        team_push::detect(
            &mut expanded,
            ctx.demo,
            &ctx.events,
            &spatial.tracks,
            &|a, b| spatial.geometry.world.ray(a, b),
        )
    } else {
        warnings.push("pushUnavailable".into());
        vec![]
    };
    let offers = ctx
        .originals
        .iter()
        .filter_map(|h| {
            let clip = ctx.recording.iter().find(|c| c.id == h.id)?;
            sequences.iter().any(|p| p.id() == h.id).then(|| Offer {
                candidate: candidate(&h.id, Kind::Push, clip.start_tick, String::new(), true),
                claims: vec![Claim::Intro],
                priority: 2,
            })
        })
        .collect();
    Detected {
        data: super::Prepared::Push(Prepared {
            expanded,
            sequences,
        }),
        offers,
        warnings,
    }
}
impl Feature for Prepared {
    fn kind(&self) -> Kind {
        Kind::Push
    }
    fn phase(&self) -> Phase {
        Phase::Expand
    }
    fn compose(&self, selected: &[&Candidate], _: &Composition<'_>, plan: &mut Plan) -> Result<()> {
        for h in &mut plan.recording {
            if selected.iter().any(|c| c.highlight_id == h.id) {
                if let Some(expanded) = self
                    .expanded
                    .iter()
                    .find(|e| e.id == h.id && e.end_tick == h.end_tick)
                {
                    *h = expanded.clone();
                }
            }
        }
        plan.teams = self
            .sequences
            .iter()
            .filter(|p| selected.iter().any(|c| c.highlight_id == p.id()))
            .cloned()
            .collect();
        Ok(())
    }
    fn validate(&self, selected: &[&Candidate], plan: &Plan) -> Result<()> {
        ensure!(
            selected
                .iter()
                .all(|c| plan.teams.iter().any(|p| p.id() == c.highlight_id
                    && team_push::applies(p, &plan.recording, &plan.utilities))),
            "Selected team push conflicts with another edit"
        );
        Ok(())
    }
}
