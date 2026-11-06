use super::*;

#[derive(Serialize, Deserialize)]
pub(in crate::render::editing) struct Prepared {
    opportunities: perspectives::Prepared,
    lead_ticks: i32,
}
pub(super) fn analyze(ctx: &Context<'_>) -> Detected {
    let opportunities = perspectives::prepare(ctx.originals, ctx.demo, &ctx.events);
    let all = opportunities.all(ctx.originals);
    let mut offers = vec![];
    for original in ctx.originals {
        let clips = ctx.clips(&original.id);
        let (possible, _) = perspectives::compose(
            clips.clone(),
            vec![None; clips.len()],
            std::slice::from_ref(original),
            ctx.demo,
            &opportunities,
            Some(&all),
        );
        let (recommended, _) = perspectives::compose(
            clips.clone(),
            vec![None; clips.len()],
            std::slice::from_ref(original),
            ctx.demo,
            &opportunities,
            None,
        );
        let rate = ctx.demo.tick_rate.round() as i32;
        for shot in possible
            .iter()
            .filter(|h| h.player.steamid != original.player.steamid)
        {
            let recommended = recommended.iter().any(|h| {
                h.player.steamid == shot.player.steamid && h.start_tick == shot.start_tick
            });
            let mut c = candidate(
                &original.id,
                Kind::Combat,
                shot.start_tick + rate * 3 / 2,
                shot.player.name.clone(),
                recommended,
            );
            c.target_id = shot.player.steamid.clone();
            c.id.push_str(&format!(":{}", c.target_id));
            offers.push(Offer {
                candidate: c,
                priority: 1,
                claims: vec![Claim::Perspective {
                    player: shot.player.steamid.clone(),
                    start: shot.start_tick,
                    end: shot.end_tick,
                }],
            });
        }
    }
    Detected {
        data: super::Prepared::Combat(Prepared {
            opportunities,
            lead_ticks: ctx.demo.tick_rate.round() as i32 * 3 / 2,
        }),
        offers,
        warnings: vec![],
    }
}
impl Feature for Prepared {
    fn kind(&self) -> Kind {
        Kind::Combat
    }
    fn phase(&self) -> Phase {
        Phase::Replay
    }
    fn compose(
        &self,
        selected: &[&Candidate],
        ctx: &Composition<'_>,
        plan: &mut Plan,
    ) -> Result<()> {
        let kills = selected
            .iter()
            .map(|c| (c.highlight_id.clone(), c.tick, c.target_id.clone()))
            .collect();
        let (recording, utilities) = perspectives::compose(
            std::mem::take(&mut plan.recording),
            std::mem::take(&mut plan.utilities),
            ctx.originals,
            ctx.demo,
            &self.opportunities,
            Some(&kills),
        );
        plan.replace_story(recording, utilities);
        Ok(())
    }
    fn validate(&self, selected: &[&Candidate], plan: &Plan) -> Result<()> {
        ensure!(
            selected
                .iter()
                .all(|c| plan.recording.iter().any(|h| h.id == c.highlight_id
                    && h.start_tick == c.tick - self.lead_ticks
                    && h.player.steamid == c.target_id)),
            "Selected replay conflicts with another edit"
        );
        Ok(())
    }
}
