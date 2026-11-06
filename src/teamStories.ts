import type { KillEvent, ParsedDemo, ReplayData, ReplayFrame, TeamKey } from './api.ts';

export type StoryKind = 'utility' | 'trade' | 'converge' | 'advance' | 'relocate' | 'plant' | 'defuse' | 'clutch';
export interface StoryBeat { kind: StoryKind; tick: number; start: number; end: number; players: string[] }
export interface TeamStory { round: number; team: TeamKey; side: 'CT' | 'TERRORIST'; won: boolean; beats: StoryBeat[]; keyPlayer?: string }
const distance = (a: number[], b: number[]) => Math.hypot(a[1]! - b[1]!, a[2]! - b[2]!);
function frameBefore(frames: ReplayFrame[], tick: number): ReplayFrame | undefined {
  let lo = 0, hi = frames.length;
  while (lo < hi) { const mid = (lo + hi) >>> 1; if (frames[mid]!.t <= tick) lo = mid + 1; else hi = mid; }
  return frames[lo - 1];
}

/** Evidence-based storyboard; movement labels deliberately avoid guessing named sites or intent. */
export function teamStories(parsed: ParsedDemo, replay: ReplayData, kills: KillEvent[]): TeamStory[] {
  const rate = replay.tickRate;
  if (!Number.isFinite(rate) || rate <= 0) return [];
  const first = parsed.rounds[0];
  if (!first) return [];
  const identity = new Map(Object.entries(first.roster).map(([id, side]) => [id, side === 'CT' ? 'A' : 'B'] as const));
  const playerId = (index: number | undefined) => index == null ? undefined : replay.players[index]?.steamid;
  const stories: TeamStory[] = [];
  for (const round of parsed.rounds) {
    const frames = replay.frames.filter(f => f.t >= round.freezeEndTick && f.t <= round.endTick);
    const roundKills = kills.filter(k => k.tick >= round.freezeEndTick && k.tick <= round.endTick && k.attacker && k.attacker.team !== k.victim.team && !k.isFreezePeriod);
    const events = replay.events.filter(e => e.t >= round.freezeEndTick && e.t <= round.endTick);
    for (const team of ['A', 'B'] as const) {
      const members = new Set(Object.keys(round.roster).filter(id => identity.get(id) === team));
      const side = round.roster[[...members][0] ?? ''];
      if (!side || members.size < 2) continue;
      const beats: StoryBeat[] = [];
      const add = (kind: StoryKind, tick: number, players: string[], lead = 1.5, tail = 4) => {
        if (beats.some(b => b.kind === kind && Math.abs(b.tick - tick) < rate * 5)) return;
        beats.push({ kind, tick, start: Math.max(round.freezeEndTick, Math.round(tick - lead * rate)), end: Math.min(round.endTick, Math.round(tick + tail * rate)), players: [...new Set(players)] });
      };
      const ownKills = roundKills.filter(k => members.has(k.attacker!.steamid));
      for (const kill of ownKills) {
        const loss = roundKills.find(k => k.tick < kill.tick && kill.tick - k.tick <= rate * 3 && members.has(k.victim.steamid) && k.attacker?.steamid === kill.victim.steamid);
        if (loss) add('trade', kill.tick, [loss.victim.steamid, kill.attacker!.steamid], (kill.tick - loss.tick) / rate + 1.5, 2);
      }
      const utility = events.filter(e => ['smoke','flash','he','fire'].includes(e.k) && members.has(playerId(e.p) ?? ''));
      for (const e of utility) {
        const burst = utility.filter(other => other.t >= e.t && other.t <= e.t + 2 * rate);
        const players = [...new Set(burst.map(e => playerId(e.p)!))];
        if (players.length >= 2) add('utility', e.t, players, 2, 5);
      }
      for (const e of events.filter(e => ['plant','defuse'].includes(e.k) && members.has(playerId(e.p) ?? ''))) add(e.k as 'plant' | 'defuse', e.t, [playerId(e.p)!], 2, 2);
      const rows = (f: ReplayFrame) => f.p.filter(p => p.length >= 6 && p[5]! > 0 && members.has(playerId(p[0]) ?? ''));
      // Use one sample per second. Pair trajectories, not proximity alone, qualify a move.
      let previousTick = -Infinity;
      for (let i = 0; i < frames.length; i++) {
        const frame = frames[i]!;
        if (frame.t - previousTick < rate) continue;
        previousTick = frame.t;
        const before = frameBefore(frames, frame.t - 2 * rate);
        if (!before || frame.t - before.t > 3 * rate) continue;
        const moving = rows(frame).flatMap(p => {
          const prior = rows(before).find(q => q[0] === p[0]);
          return prior && distance(p, prior) >= 180 ? [{ p, prior }] : [];
        });
        const nearbyKill = roundKills.find(k => Math.abs(k.tick - frame.t) < 4 * rate && (members.has(k.attacker!.steamid) || members.has(k.victim.steamid)));
        const nextUtility = utility.some(e => Math.abs(e.t - frame.t) < 3 * rate);
        if (side === 'TERRORIST' && (nearbyKill || nextUtility)) {
          for (let a = 0; a < moving.length; a++) for (let b = a + 1; b < moving.length; b++) {
            const left = moving[a]!, right = moving[b]!;
            const dot = (left.p[1]! - left.prior[1]!) * (right.p[1]! - right.prior[1]!) + (left.p[2]! - left.prior[2]!) * (right.p[2]! - right.prior[2]!);
            const cosine = dot / (distance(left.p, left.prior) * distance(right.p, right.prior));
            const ids = [playerId(left.p[0])!, playerId(right.p[0])!];
            if (Math.abs(left.p[3]!-right.p[3]!) > 160) continue;
            if (distance(left.p, right.p) < 550 && cosine > 0.7) add('advance', frame.t, ids, 2, 4);
            if (nearbyKill) {
              const enemy = members.has(nearbyKill.victim.steamid) ? nearbyKill.attacker!.steamid : nearbyKill.victim.steamid;
              const target = frame.p.find(p => playerId(p[0]) === enemy);
              if (target && Math.abs(target[3]!-left.p[3]!) < 160 && distance(left.p,target) < 900 && distance(right.p,target) < 900 && cosine < 0.4 && distance(left.prior,target)-distance(left.p,target)>120 && distance(right.prior,target)-distance(right.p,target)>120) add('converge',frame.t,ids,2,4);
            }
          }
        }
        if (side === 'TERRORIST' && (!round.bombPlantedTick || frame.t < round.bombPlantedTick)) {
          const earlier = frameBefore(frames, frame.t - 8 * rate);
          if (earlier && frame.t - earlier.t <= 9 * rate) {
            const relocating = rows(frame).filter(p => { const old = rows(earlier).find(q => q[0] === p[0]); return old && distance(p,old)>1100; });
            // Require prior combat/utility: ordinary spawn-to-site travel is not a rotation.
            const priorAction = [...utility.map(e=>e.t), ...roundKills.map(k=>k.tick)].some(t=>t>=earlier.t-4*rate && t<=earlier.t);
            if(priorAction && relocating.length>=2 && distance(relocating[0]!,relocating[1]!)<650) add('relocate',frame.t,relocating.map(p=>playerId(p[0])!),8,2);
          }
        }
      }
      if (round.winner === side) {
        const loss = roundKills.filter(k => members.has(k.victim.steamid)).at(-1);
        const alive = [...members].filter(id => !roundKills.some(k => k.tick <= (loss?.tick ?? round.endTick) && k.victim.steamid === id));
        if(loss && alive.length === 1 && ownKills.some(k=>k.tick>loss.tick && k.attacker!.steamid===alive[0])) add('clutch',loss.tick,alive,1,Math.min(10,(round.endTick-loss.tick)/rate));
      }
      beats.sort((a,b)=>a.tick-b.tick);
      const impact = new Map([...members].map(id => [id, ownKills.filter(k=>k.attacker!.steamid===id).length*2 + beats.filter(b=>b.players.includes(id) && ['utility','trade','plant','defuse'].includes(b.kind)).length]));
      const keyPlayer = [...impact].sort((a,b)=>b[1]-a[1])[0];
      if(beats.length) stories.push({round:round.round,team,side,won:round.winner===side,beats,keyPlayer:keyPlayer && keyPlayer[1]>0 ? keyPlayer[0] : undefined});
    }
  }
  return stories;
}
