use super::*;
use crate::render::opening as camera;

#[derive(Serialize, Deserialize)]
pub(in crate::render::editing) struct Prepared {
    opportunities: camera::Prepared,
}
pub(super) fn analyze(ctx: &Context<'_>) -> Detected {
    let opportunities = camera::prepare(ctx.originals, ctx.demo, &ctx.events);
    let offers = ctx
        .originals
        .iter()
        .filter_map(|h| {
            let clips = ctx.clips(&h.id);
            let tick = clips.first()?.start_tick;
            let (_, _, intros) = camera::compose(
                clips.clone(),
                vec![None; clips.len()],
                std::slice::from_ref(h),
                ctx.demo,
                &opportunities,
            );
            intros.iter().any(Option::is_some).then(|| Offer {
                candidate: candidate(&h.id, Kind::Opening, tick, String::new(), true),
                claims: vec![Claim::Intro],
                priority: 1,
            })
        })
        .collect();
    Detected {
        data: super::Prepared::Opening(Prepared { opportunities }),
        offers,
        warnings: vec![],
    }
}
impl Feature for Prepared {
    fn kind(&self) -> Kind {
        Kind::Opening
    }
    fn phase(&self) -> Phase {
        Phase::Intro
    }
    fn compose(
        &self,
        selected: &[&Candidate],
        ctx: &Composition<'_>,
        plan: &mut Plan,
    ) -> Result<()> {
        let originals: Vec<_> = ctx
            .originals
            .iter()
            .filter(|h| selected.iter().any(|c| c.highlight_id == h.id))
            .cloned()
            .collect();
        (plan.recording, plan.utilities, plan.openings) = camera::compose(
            std::mem::take(&mut plan.recording),
            std::mem::take(&mut plan.utilities),
            &originals,
            ctx.demo,
            &self.opportunities,
        );
        Ok(())
    }
    fn validate(&self, selected: &[&Candidate], plan: &Plan) -> Result<()> {
        ensure!(
            selected.iter().all(|c| plan
                .recording
                .iter()
                .zip(&plan.openings)
                .any(|(h, camera)| h.id == c.highlight_id && camera.is_some())),
            "Selected opening conflicts with another edit"
        );
        Ok(())
    }
}
