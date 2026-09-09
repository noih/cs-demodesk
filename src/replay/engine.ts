// Pure replay logic (no React, no canvas): where everyone is at a tick, which
// effects are active, round clock and score. Drawing is in draw.ts.
import { FLAG, mmss, type KillEvent, type MapAssets, type ParsedDemo, type ReplayData, type ReplayEvent, type RoundInfo, type Team } from '../api.ts';

export interface PlayerState {
  pid: number;
  steamid: string;
  name: string;
  team: Team;
  x: number;
  y: number;
  z: number;
  yaw: number;
  hp: number;
  armor: number;
  alive: boolean;
  helmet: boolean;
  defuser: boolean;
  blind: boolean;
  bomb: boolean;
  ducking: boolean;
  walking: boolean;
  weapon: string;
  money: number;
  /** ground speed in units/s, from the surrounding frames */
  speed: number;
  kills: number;
  deaths: number;
  assists: number;
}
export interface GrenadeState {
  id: number;
  kind: number;
  x: number;
  y: number;
  z: number;
  pid: number;
}
/** An area effect drawn for a while: smoke cloud, fire, flash / HE burst. */
export interface Effect {
  kind: 'smoke' | 'fire' | 'flash' | 'he' | 'decoy';
  x: number;
  y: number;
  z: number;
  start: number;
  end: number;
}
export interface Shot {
  pid: number;
  x: number;
  y: number;
  z: number;
  yaw: number;
  t: number;
  /** 0 = just fired … 1 = about to disappear */
  age: number;
  /** weapon in hand at the time (for the silenced / loud hearing radius) */
  weapon: string;
}
export interface Death {
  pid: number;
  x: number;
  y: number;
  z: number;
  t: number;
}
export interface BombState {
  state: 'carried' | 'dropped' | 'planted' | 'defused' | 'exploded';
  x?: number;
  y?: number;
  z?: number;
  since: number;
  /** seconds left on the timer while planted */
  remaining?: number;
  /** defuse in progress: when it started and the seconds it needs (5 with kit, 10 without) */
  defuse?: { pid: number; since: number; needs: number; remaining: number };
}
export interface TickState {
  tick: number;
  round?: RoundInfo;
  players: PlayerState[];
  grenades: GrenadeState[];
  effects: Effect[];
  fireCells?: number[][];
  shots: Shot[];
  deaths: Death[];
  bomb?: BombState;
  /** kills shown in the feed right now (most recent last) */
  feed: KillEvent[];
  clock: string;
  /** CT score : T score for the round being watched */
  score: { ct: number; t: number };
}

/** Durations in seconds; converted to ticks per demo (64 or 128 tick). */
export const SECONDS = { smoke: 20, fire: 7, burst: 0.6, decoy: 45, shot: 0.25, feed: 6, round: 115, bomb: 40 } as const;

export function roundAt(rounds: RoundInfo[], tick: number): RoundInfo | undefined {
  let found: RoundInfo | undefined;
  for (const r of rounds) {
    if (r.startTick <= tick) found = r;
    else break;
  }
  return found;
}

/** Index of the last element with key <= value in a sorted array (or -1). */
function lastAtOrBefore<T>(arr: T[], key: (t: T) => number, value: number): number {
  let lo = -1;
  let hi = arr.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (key(arr[mid]!) <= value) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function lerpAngle(a: number, b: number, k: number): number {
  const d = ((b - a + 540) % 360) - 180;
  return a + d * k;
}

type Kda = [number, number, number];

export class Replay {
  readonly data: ReplayData;
  readonly parsed: ParsedDemo;
  readonly kills: KillEvent[];
  /** ticks for the SECONDS table at this demo's tick rate */
  readonly ticks: Record<keyof typeof SECONDS, number>;
  private readonly teamByRound: Map<number, Map<string, Team>>;
  private readonly fallbackTeam: Map<string, Team>;
  /** stable team key (A/B) of the CT side per round (undefined when the roster is empty) */
  private readonly ctKeyByRound: Map<number, 'A' | 'B' | undefined>;
  /** cumulative K/D/A per player after kill i (kills are tick-sorted) */
  private readonly kdaAfter: Array<Map<string, Kda>>;
  /** index of the first event of each round, so a frame never scans earlier rounds */
  private readonly eventStart: Map<number, number>;

  constructor(data: ReplayData, parsed: ParsedDemo, kills: KillEvent[]) {
    this.data = data;
    this.parsed = parsed;
    this.kills = kills;
    this.ticks = Object.fromEntries(Object.entries(SECONDS).map(([k, s]) => [k, s * data.tickRate])) as Replay['ticks'];
    this.teamByRound = new Map(parsed.rounds.map((r) => [r.round, new Map(Object.entries(r.roster) as [string, Team][])]));
    this.fallbackTeam = new Map(parsed.info.players.map((p) => [p.steamid, p.teamNumber === 3 ? 'CT' : 'TERRORIST']));
    const keyOf = new Map(parsed.stats.map((s) => [s.steamid, s.team]));
    this.ctKeyByRound = new Map(
      parsed.rounds.map((r) => {
        const ctSid = Object.entries(r.roster).find(([, t]) => t === 'CT')?.[0];
        return [r.round, ctSid ? keyOf.get(ctSid) : undefined];
      }),
    );
    // running K/D/A: one snapshot per kill (a match has ~150 kills)
    let acc = new Map<string, Kda>();
    this.kdaAfter = kills.map((kv) => {
      acc = new Map(acc);
      const bump = (sid: string | undefined, i: 0 | 1 | 2) => {
        if (!sid) return;
        const v = [...(acc.get(sid) ?? [0, 0, 0])] as Kda;
        v[i]++;
        acc.set(sid, v);
      };
      if (kv.attacker && kv.attacker.steamid !== kv.victim.steamid && kv.attacker.team !== kv.victim.team) bump(kv.attacker.steamid, 0);
      bump(kv.victim.steamid, 1);
      bump(kv.assister?.steamid, 2);
      return acc;
    });
    this.eventStart = new Map(parsed.rounds.map((r) => [r.round, lastAtOrBefore(data.events, (e) => e.t, r.startTick - 1) + 1]));
  }

  get firstTick(): number {
    return this.data.firstTick;
  }
  get lastTick(): number {
    return this.data.lastTick;
  }

  private team(steamid: string, round?: RoundInfo): Team {
    return (round && this.teamByRound.get(round.round)?.get(steamid)) ?? this.fallbackTeam.get(steamid) ?? 'TERRORIST';
  }

  /** Which of Team A / Team B plays CT in this round. */
  ctKey(round: RoundInfo): 'A' | 'B' | undefined {
    return this.ctKeyByRound.get(round.round);
  }

  score(round?: RoundInfo): { ct: number; t: number } {
    if (!round) return { ct: 0, t: 0 };
    let a = 0;
    let b = 0;
    for (const s of this.parsed.roundSummaries) {
      if (s.round >= round.round) break;
      if (s.winner === 'A') a++;
      else if (s.winner === 'B') b++;
    }
    return this.ctKey(round) === 'B' ? { ct: b, t: a } : { ct: a, t: b };
  }

  clock(tick: number, round: RoundInfo | undefined, bomb?: BombState): string {
    if (!round) return '';
    const tr = this.data.tickRate;
    const up = (seconds: number) => mmss(Math.ceil(Math.max(0, seconds)));
    if (tick < round.freezeEndTick) return up((round.freezeEndTick - tick) / tr);
    if (tick > round.endTick) return up(0);
    if (bomb?.state === 'planted') return up(SECONDS.bomb - (tick - bomb.since) / tr);
    return up(SECONDS.round - (tick - round.freezeEndTick) / tr);
  }

  /** Kills inside a round, for timeline markers. */
  killsIn(round: RoundInfo): KillEvent[] {
    return this.kills.filter((k) => k.tick >= round.startTick && k.tick <= round.officiallyEndedTick);
  }

  /** Everything needed to draw one moment. `tick` may be fractional (interpolated). */
  stateAt(tick: number): TickState {
    const { data, ticks } = this;
    const round = roundAt(this.parsed.rounds, tick);
    const roundStart = round?.startTick ?? data.firstTick;
    const i = Math.max(0, lastAtOrBefore(data.frames, (f) => f.t, tick));
    const a = data.frames[i]!;
    const b = data.frames[i + 1];
    const span = b && b.t > a.t ? b.t - a.t : 0;
    const k = span ? Math.min(1, Math.max(0, (tick - a.t) / span)) : 0;
    const next: Array<number[] | undefined> = new Array(data.players.length);
    if (b) for (const row of b.p) next[row[0]!] = row;
    const killIdx = lastAtOrBefore(this.kills, (kv) => kv.tick, tick);
    const kda = killIdx >= 0 ? this.kdaAfter[killIdx] : undefined;

    const players: PlayerState[] = [];
    for (const row of a.p) {
      const [pid, x, y, z, yaw, hp, armor, flags, weapon, money] = row as [number, number, number, number, number, number, number, number, number, number];
      const n = next[pid];
      const alive = (flags & FLAG.alive) !== 0;
      // interpolate only while alive in both frames (a respawn / teleport must not glide)
      const dist = n ? Math.hypot(n[1]! - x, n[2]! - y) : 0;
      const glide = n && alive && (n[7]! & FLAG.alive) !== 0 && dist < 400;
      const p = data.players[pid]!;
      const [nk, nd, na] = kda?.get(p.steamid) ?? [0, 0, 0];
      players.push({
        pid,
        steamid: p.steamid,
        name: p.name,
        team: this.team(p.steamid, round),
        x: glide ? x + (n[1]! - x) * k : x,
        y: glide ? y + (n[2]! - y) * k : y,
        z: glide ? z + (n[3]! - z) * k : z,
        yaw: glide ? lerpAngle(yaw, n[4]!, k) : yaw,
        hp,
        armor,
        alive,
        helmet: (flags & FLAG.helmet) !== 0,
        defuser: (flags & FLAG.defuser) !== 0,
        blind: (flags & FLAG.blind) !== 0,
        bomb: (flags & FLAG.bomb) !== 0,
        ducking: (flags & FLAG.ducking) !== 0,
        walking: (flags & FLAG.walking) !== 0,
        weapon: data.weapons[weapon] ?? '',
        money: money ?? 0,
        speed: glide && span ? dist / (span / data.tickRate) : 0,
        kills: nk,
        deaths: nd,
        assists: na,
      });
    }
    const grenades: GrenadeState[] = (a.g ?? []).map((g) => ({ id: g[0]!, kind: g[1]!, x: g[2]!, y: g[3]!, z: g[4]!, pid: g[5]! }));

    // events since the round started (effects, shots, deaths, bomb)
    const acc: EventAccumulator = { effects: [], shots: [], deaths: [], open: new Map(), bomb: undefined };
    for (let j = round ? (this.eventStart.get(round.round) ?? 0) : 0; j < data.events.length; j++) {
      const e = data.events[j]!;
      if (e.t > tick) break;
      applyEvent(e, tick, acc, ticks);
    }
    const weaponOf = new Map(players.map((p) => [p.pid, p.weapon]));
    for (const sh of acc.shots) sh.weapon = weaponOf.get(sh.pid) ?? '';
    const bomb = acc.bomb;
    if (bomb?.state === 'planted') {
      bomb.remaining = Math.max(0, SECONDS.bomb - (tick - bomb.since) / data.tickRate);
      if (bomb.defuse) {
        const defuser = a.p.find((row) => row[0] === bomb.defuse!.pid);
        // Some CS2 demos omit bomb_abortdefuse. The next player sample is
        // authoritative; a sample preceding the start event cannot cancel it.
        const flags = defuser?.[7] ?? 0;
        if (a.t > bomb.defuse.since && (!(flags & FLAG.defusing) || !(flags & FLAG.alive))) {
          bomb.defuse = undefined;
        } else {
          bomb.defuse.remaining = Math.max(0, bomb.defuse.needs - (tick - bomb.defuse.since) / data.tickRate);
        }
      }
    }
    const feed = this.kills.slice(Math.max(0, killIdx - 5), killIdx + 1).filter((kv) => kv.tick > tick - ticks.feed && kv.tick >= roundStart);
    return {
      tick,
      round,
      players,
      grenades,
      fireCells: data.schemaVersion >= 5 ? a.f ?? [] : undefined,
      effects: acc.effects.filter((f) => f.end > tick),
      shots: acc.shots,
      deaths: acc.deaths,
      bomb,
      feed,
      clock: this.clock(tick, round, bomb),
      score: this.score(round),
    };
  }
}

interface EventAccumulator {
  effects: Effect[];
  shots: Shot[];
  deaths: Death[];
  /** effects that can be ended early by a matching *End event, by kind + entity id */
  open: Map<string, Effect>;
  bomb?: BombState;
}

function applyEvent(e: ReplayEvent, tick: number, acc: EventAccumulator, ticks: Replay['ticks']) {
  const x = e.x ?? 0;
  const y = e.y ?? 0;
  const z = e.z ?? 0;
  const key = `${e.k.replace('End', '')}:${e.id ?? `${x},${y}`}`;
  const begin = (kind: Effect['kind'], ttl: number) => {
    const eff: Effect = { kind, x, y, z, start: e.t, end: e.t + ttl };
    acc.effects.push(eff);
    acc.open.set(key, eff);
  };
  const finish = (kind: Effect['kind']) => {
    const eff = acc.open.get(key) ?? [...acc.open.values()].filter((f) => f.kind === kind && f.end > e.t).sort((p, q) => Math.hypot(p.x - x, p.y - y) - Math.hypot(q.x - x, q.y - y))[0];
    if (eff) eff.end = Math.min(eff.end, e.t);
  };
  switch (e.k) {
    case 'shot':
      if (e.t > tick - ticks.shot) acc.shots.push({ pid: e.p ?? -1, x, y, z, yaw: e.yaw ?? 0, t: e.t, age: (tick - e.t) / ticks.shot, weapon: '' });
      break;
    case 'smoke':
      begin('smoke', ticks.smoke);
      break;
    case 'smokeEnd':
      finish('smoke');
      break;
    case 'fire':
      begin('fire', ticks.fire);
      break;
    case 'fireEnd':
      finish('fire');
      break;
    case 'flash':
      begin('flash', ticks.burst);
      break;
    case 'he':
      begin('he', ticks.burst);
      break;
    case 'decoy':
      begin('decoy', ticks.decoy);
      break;
    case 'decoyEnd':
      finish('decoy');
      break;
    case 'death':
      if (e.p !== undefined) acc.deaths.push({ pid: e.p, x, y, z, t: e.t });
      break;
    case 'plant':
      acc.bomb = { state: 'planted', x, y, z, since: e.t };
      break;
    case 'defuseStart':
      if (acc.bomb?.state === 'planted') acc.bomb.defuse = { pid: e.p ?? -1, since: e.t, needs: e.kit ? 5 : 10, remaining: 0 };
      break;
    case 'defuseAbort':
      if (acc.bomb) acc.bomb.defuse = undefined;
      break;
    case 'defuse':
      acc.bomb = { state: 'defused', x, y, z, since: e.t };
      break;
    case 'explode':
      acc.bomb = { state: 'exploded', x, y, z, since: e.t };
      break;
    case 'bombDrop':
      acc.bomb = { state: 'dropped', x, y, z, since: e.t };
      break;
    case 'bombPickup':
      acc.bomb = { state: 'carried', since: e.t };
      break;
  }
}

/** World → radar image pixel (1024 × 1024 image space). */
export function toImage(m: MapAssets, x: number, y: number): [number, number] {
  return [(x - m.posX) / m.scale, (m.posY - y) / m.scale];
}

/** Which layer a world z belongs to (index into `m.layers`). */
export function layerOf(m: MapAssets, z: number): number {
  const i = m.layers.findIndex((l) => z >= l.altitudeMin && z < l.altitudeMax);
  return i < 0 ? 0 : i;
}

/** Playback clock: ticks advance with wall time × speed. */
export class Clock {
  tick: number;
  speed = 1;
  playing = false;
  constructor(
    tick: number,
    readonly tickRate: number,
    readonly first: number,
    readonly last: number,
  ) {
    this.tick = tick;
  }
  advance(dtMs: number): boolean {
    if (!this.playing) return false;
    this.tick = Math.min(this.last, this.tick + (dtMs / 1000) * this.tickRate * this.speed);
    if (this.tick >= this.last) this.playing = false;
    return true;
  }
  seek(tick: number) {
    this.tick = Math.min(this.last, Math.max(this.first, tick));
  }
}
