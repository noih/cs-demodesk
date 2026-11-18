use super::*;

#[derive(Serialize, Deserialize)]
pub(in crate::render::editing) struct Prepared {
    opportunities: utility::Prepared,
}
pub(super) fn analyze(ctx: &Context<'_>) -> Detected {
    let mut opportunities = utility::prepare(ctx.originals, ctx.demo, &ctx.events);
    if let Some(spatial) = &ctx.spatial {
        utility::prepare_observers(
            &mut opportunities,
            &spatial.tracks,
            &spatial.projectiles,
            ctx.demo,
            &|a, b| spatial.geometry.world.ray(a, b),
            ctx.cancel,
        );
    }
    let offers = ctx
        .originals
        .iter()
        .filter_map(|h| {
            let clips = ctx.clips(&h.id);
            let tick = clips.first()?.start_tick;
            let (shots, cameras) = utility::compose(clips, ctx.demo, &opportunities);
            cameras.iter().any(Option::is_some).then(|| Offer {
                candidate: candidate(&h.id, Kind::Teamwork, tick, String::new(), true),
                priority: 2,
                claims: shots
                    .iter()
                    .filter(|s| s.player.steamid != h.player.steamid)
                    .map(|s| Claim::Perspective {
                        player: s.player.steamid.clone(),
                        start: s.start_tick,
                        end: s.end_tick,
                    })
                    .collect(),
            })
        })
        .collect();
    Detected {
        data: super::Prepared::Teamwork(Prepared { opportunities }),
        offers,
        warnings: vec![],
    }
}
impl Feature for Prepared {
    fn kind(&self) -> Kind {
        Kind::Teamwork
    }
    fn phase(&self) -> Phase {
        Phase::Story
    }
    fn compose(
        &self,
        selected: &[&Candidate],
        ctx: &Composition<'_>,
        plan: &mut Plan,
    ) -> Result<()> {
        let mut recording = vec![];
        let mut utilities = vec![];
        for h in std::mem::take(&mut plan.recording) {
            if selected.iter().any(|c| c.highlight_id == h.id) {
                let (shots, cameras) = utility::compose(vec![h], ctx.demo, &self.opportunities);
                recording.extend(shots);
                utilities.extend(cameras);
            } else {
                recording.push(h);
                utilities.push(None);
            }
        }
        plan.replace_story(recording, utilities);
        Ok(())
    }
    fn validate(&self, selected: &[&Candidate], plan: &Plan) -> Result<()> {
        ensure!(
            selected.iter().all(|c| plan
                .recording
                .iter()
                .zip(&plan.utilities)
                .any(|(h, camera)| h.id == c.highlight_id && camera.is_some())),
            "Selected teamwork conflicts with another edit"
        );
        Ok(())
    }
}
