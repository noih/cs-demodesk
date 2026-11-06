use super::*;

pub(in crate::render) fn shot(
    h: &Highlight,
    player: &str,
    start: i32,
    end: i32,
    demo: &DemoInfo,
) -> Highlight {
    let mut out = h.clone();
    out.start_tick = start;
    out.end_tick = end;
    out.anchor_tick = h.anchor_tick.clamp(start, end);
    out.key_moments.clear();
    out.round_result = None;
    out.player.steamid = player.into();
    out.player.name = demo
        .players
        .iter()
        .find(|p| p.steamid == player)
        .map_or_else(|| h.player.name.clone(), |p| p.name.clone());
    out
}
