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
        let rate = ctx.demo.tick_rate.round() as i32;
        let mut previous_tick = None;
        let mut group_tick = 0;
        for shot in possible
            .iter()
            .filter(|h| h.player.steamid != original.player.steamid)
        {
            let recommended = opportunities.recommended(
                &original.player.steamid,
                shot.start_tick + rate * 3 / 2,
                &shot.player.steamid,
            );
            let mut c = candidate(
                &original.id,
                Kind::Combat,
                shot.start_tick + rate * 3 / 2,
                shot.player.name.clone(),
                recommended,
            );
            c.target_id = shot.player.steamid.clone();
            c.id.push_str(&format!(":{}", c.target_id));
            if previous_tick.is_none_or(|tick| c.tick - tick > 3 * rate) {
                group_tick = c.tick;
            }
            previous_tick = Some(c.tick);
            c.replay_group = Some(format!("{}:{group_tick}", original.id));
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
        let groups = replay_groups(selected, ctx.replay_links);
        let (recording, utilities) = perspectives::compose_groups(
            std::mem::take(&mut plan.recording),
            std::mem::take(&mut plan.utilities),
            ctx.originals,
            ctx.demo,
            &self.opportunities,
            &groups,
        )?;
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

fn ordered<'a>(candidates: impl Iterator<Item = &'a Candidate>) -> Vec<&'a Candidate> {
    let mut selected: Vec<_> = candidates.filter(|c| c.kind == Kind::Combat).collect();
    selected
        .sort_by(|a, b| (&a.highlight_id, a.tick, &a.id).cmp(&(&b.highlight_id, b.tick, &b.id)));
    selected
}

pub(in crate::render::editing) fn recommended_links(
    candidates: &[Candidate],
    ids: &[String],
) -> Vec<ReplayLink> {
    ordered(candidates.iter().filter(|c| ids.contains(&c.id)))
        .windows(2)
        .filter(|pair| {
            pair[0].highlight_id == pair[1].highlight_id
                && pair[0].replay_group.is_some()
                && pair[0].replay_group == pair[1].replay_group
        })
        .map(|pair| ReplayLink {
            from: pair[0].id.clone(),
            to: pair[1].id.clone(),
        })
        .collect()
}

pub(in crate::render::editing) fn validate_links(
    candidates: &[Candidate],
    ids: &[String],
    links: &[ReplayLink],
) -> Result<()> {
    let selected = ordered(candidates.iter().filter(|c| ids.contains(&c.id)));
    let mut seen = HashSet::new();
    ensure!(
        links.iter().all(|link| seen.insert((&link.from, &link.to))
            && selected.windows(2).any(|pair| pair[0].id == link.from
                && pair[1].id == link.to
                && pair[0].highlight_id == pair[1].highlight_id)),
        "Replay links must connect adjacent selected exchanges in the same highlight"
    );
    Ok(())
}

fn replay_groups<'a>(selected: &[&'a Candidate], links: &[ReplayLink]) -> Vec<Vec<&'a Candidate>> {
    let mut groups: Vec<Vec<&Candidate>> = vec![];
    for candidate in ordered(selected.iter().copied()) {
        if let Some(group) = groups.last_mut().filter(|group| {
            links
                .iter()
                .any(|link| link.from == group.last().unwrap().id && link.to == candidate.id)
        }) {
            group.push(candidate);
        } else {
            groups.push(vec![candidate]);
        }
    }
    groups
}
