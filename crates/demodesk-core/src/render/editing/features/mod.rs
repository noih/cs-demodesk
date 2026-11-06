//! One registration and one typed payload per feature; no cross-feature decisions here.
use super::*;
use context::{Composition, Context};
mod combat;
mod opening;
mod push;
mod teamwork;

#[derive(Clone, Copy)]
pub(super) struct Registration {
    pub kind: Kind,
    pub spatial: bool,
    pub enabled: fn(&RenderOptions) -> bool,
    pub analyze: fn(&Context<'_>) -> Detected,
}
// Discovery order is stable for the UI. Compilation order belongs to each feature.
pub(super) const REGISTRY: &[Registration] = &[
    Registration {
        kind: Kind::Push,
        spatial: true,
        enabled: |o| o.team_push,
        analyze: push::analyze,
    },
    Registration {
        kind: Kind::Opening,
        spatial: false,
        enabled: |o| o.opening_camera,
        analyze: opening::analyze,
    },
    Registration {
        kind: Kind::Teamwork,
        spatial: false,
        enabled: |o| o.utility_teamwork,
        analyze: teamwork::analyze,
    },
    Registration {
        kind: Kind::Combat,
        spatial: false,
        enabled: |o| o.combat_perspectives,
        analyze: combat::analyze,
    },
];

/// Expansion precedes story edits; replays follow stories; introductions prepend last.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Phase {
    Expand,
    Story,
    Replay,
    Intro,
}

pub(super) trait Feature {
    fn kind(&self) -> Kind;
    fn phase(&self) -> Phase;
    fn compose(
        &self,
        selected: &[&Candidate],
        context: &Composition<'_>,
        plan: &mut Plan,
    ) -> Result<()>;
    fn validate(&self, _selected: &[&Candidate], _plan: &Plan) -> Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "lowercase")]
pub(super) enum Prepared {
    Push(push::Prepared),
    Opening(opening::Prepared),
    Teamwork(teamwork::Prepared),
    Combat(combat::Prepared),
}
impl Prepared {
    pub fn feature(&self) -> &dyn Feature {
        match self {
            Self::Push(data) => data,
            Self::Opening(data) => data,
            Self::Teamwork(data) => data,
            Self::Combat(data) => data,
        }
    }
}
pub(super) struct Detected {
    pub data: Prepared,
    pub offers: Vec<Offer>,
    pub warnings: Vec<String>,
}

/// Features declare resources they occupy; conflict resolution has no feature pairs.
pub(super) enum Claim {
    Intro,
    Perspective {
        player: String,
        start: i32,
        end: i32,
    },
}
impl Claim {
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Intro, Self::Intro) => true,
            (
                Self::Perspective {
                    player: a,
                    start: x,
                    end: y,
                },
                Self::Perspective {
                    player: b,
                    start: u,
                    end: v,
                },
            ) => a == b && x <= v && u <= y,
            _ => false,
        }
    }
}
pub(super) struct Offer {
    pub candidate: Candidate,
    pub claims: Vec<Claim>,
    pub priority: u8,
}
