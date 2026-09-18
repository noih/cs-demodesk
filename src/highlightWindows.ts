import type { Highlight } from './api.ts';

/** Same-player windows use the recorder's strict <1s merge rule. */
export function summarizeHighlights(highlights: Highlight[], tickRate: number, keyOnly: boolean) {
  const groups = new Map<string, [number, number][]>();
  for (const h of highlights) {
    const key = `${h.round}:${h.player.steamid}`;
    const windows = keyOnly && h.keyMoments?.length ? h.keyMoments : [[h.startTick, h.endTick] as [number, number]];
    const group = groups.get(key) ?? [];
    group.push(...windows);
    groups.set(key, group);
  }
  let ticks = 0;
  let segments = 0;
  for (const windows of groups.values()) {
    windows.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    let current: [number, number] | undefined;
    for (const [start, end] of windows) {
      if (end <= start) continue;
      if (current && start - current[1] < tickRate) {
        current[1] = Math.max(current[1], end);
      } else {
        if (current) ticks += current[1] - current[0];
        current = [start, end];
        segments++;
      }
    }
    if (current) ticks += current[1] - current[0];
  }
  return { seconds: ticks / tickRate, segments };
}
