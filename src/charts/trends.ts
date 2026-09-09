import type { ParsedDemo } from '../api.ts';
export const TRENDS = ['kills', 'deaths', 'damage', 'awp', 'flashed', 'difference', 'cash'] as const;
export type Trend = typeof TRENDS[number];
export function trendSeries(parsed: ParsedDemo, metric: Trend) {
  if (metric === 'difference') {
    let score = 0;
    return [{ id: 'difference', values: parsed.roundSummaries.map(r => score += r.winner === 'A' ? 1 : r.winner === 'B' ? -1 : 0) }];
  }
  if (metric === 'cash') return (['A', 'B'] as const).map(team => ({ id: team, values: parsed.roundSummaries.map(r => {
    const players = parsed.stats.filter(p => p.team === team);
    const cash = players.map(p => r.players?.[p.steamid]?.cash);
    return !players.length || cash.some(v => v == null) ? null : cash.reduce<number>((sum, v) => sum + v!, 0);
  }) }));
  return parsed.stats.map(p => {
    let total = 0;
    let missing = false;
    return { id: p.steamid, values: parsed.roundSummaries.map(r => {
      const value = r.players?.[p.steamid]?.[metric];
      if (value == null) missing = true;
      return missing ? null : total += value!;
    }) };
  });
}
