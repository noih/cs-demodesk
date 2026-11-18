import type { EditingCandidate, ReplayLink } from './api.ts';

export function selectedReplays(candidates: EditingCandidate[], selected: string[]): EditingCandidate[] {
  return candidates.filter(c => c.kind === 'combat' && selected.includes(c.id))
    .sort((a, b) => a.highlightId.localeCompare(b.highlightId) || a.tick - b.tick || a.id.localeCompare(b.id));
}

export function replayPairs(candidates: EditingCandidate[], selected: string[]): ReplayLink[] {
  const replays = selectedReplays(candidates, selected);
  return replays.flatMap((c, i) => {
    const previous = replays[i - 1];
    return previous?.highlightId === c.highlightId ? [{ from: previous.id, to: c.id }] : [];
  });
}

export function recommendedReplayLinks(candidates: EditingCandidate[], selected: string[]): ReplayLink[] {
  return replayPairs(candidates, selected).filter(link => {
    const from = candidates.find(c => c.id === link.from);
    const to = candidates.find(c => c.id === link.to);
    return !!from?.replayGroup && from.replayGroup === to?.replayGroup;
  });
}

// Removing or inserting an exchange never silently reconnects its former neighbours.
export function retainReplayLinks(candidates: EditingCandidate[], selected: string[], links: ReplayLink[]): ReplayLink[] {
  const pairs = replayPairs(candidates, selected);
  return links.filter(link => pairs.some(pair => pair.from === link.from && pair.to === link.to));
}
