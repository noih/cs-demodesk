import type { RecoilBurst, RecoilFrame, RecoilPoint, RecoilShot } from './api.ts';

type Angles = [number, number];
type Vector = [number, number, number];
export type SprayState = 'tracking' | 'reaction' | 'transfer' | 'unknown' | 'timingUnknown' | 'partialTracking';
export interface SprayAnalysis {
  points: (RecoilPoint | null)[];
  states: SprayState[];
  targets: (string | null)[];
  segments: number[];
  delayMs: (number | null)[];
}
const MAX_REACTION_SECONDS = 0.2;
const wrap = (a: number) => ((a + 180) % 360 + 360) % 360 - 180;
const subtract = (a: Angles, b: Angles): Angles => [a[0] - b[0], wrap(a[1] - b[1])];
const velocityChange = (a: Angles, b: Angles): Angles => [a[0] - b[0], a[1] - b[1]];
const length = (a: Angles) => Math.hypot(...a);
const dot = (a: Angles, b: Angles) => a[0] * b[0] + a[1] * b[1];
function direction(eye: Vector, point: Vector): Angles {
  const [x, y, z] = point.map((v, i) => v - eye[i]!);
  return [-Math.atan2(z!, Math.hypot(x!, y!)) * 180 / Math.PI, Math.atan2(y!, x!) * 180 / Math.PI];
}
function recoilAt(burst: RecoilBurst, reference: RecoilPoint[], tick: number): Angles {
  const next = burst.shots.findIndex(s => s.tick > tick);
  const i = next === -1 ? burst.shots.length - 1 : Math.max(0, next - 1);
  const a = reference[i], b = reference[i + 1];
  if (!a) return [NaN, NaN];
  const fraction = next > 0 && b ? (tick - burst.shots[i]!.tick) / (burst.shots[i + 1]!.tick - burst.shots[i]!.tick) : 0;
  return [a.y + ((b?.y ?? a.y) - a.y) * fraction, a.x + ((b?.x ?? a.x) - a.x) * fraction];
}
function firingView(burst: RecoilBurst, reference: RecoilPoint[], frame: RecoilFrame): Angles {
  const shot = burst.shots.find(s => s.tick === frame.tick);
  const view: Angles = shot ? [shot.viewPitch, shot.viewYaw] : frame.view;
  const recoil = recoilAt(burst, reference, frame.tick);
  return [view[0] + recoil[0], view[1] + recoil[1]];
}

/** Infer a persistent target, never infer screen visibility from a pawn position. */
function assignTargets(burst: RecoilBurst, reference: RecoilPoint[], rate: number): (string | null)[] {
  let current: string | null = null, pending: string | null = null, since = 0;
  let previousTick = -Infinity;
  const dead = new Set<string>();
  return (burst.tracking ?? []).map(frame => {
    if (frame.tick !== previousTick + 1) { current = null; pending = null; }
    previousTick = frame.tick;
    const contacts = (burst.contacts ?? []).filter(c => c.tick === frame.tick || c.tick === frame.tick + 1);
    const hitIds = new Set(contacts.map(c => c.targetId));
    const view = firingView(burst, reference, frame);
    const candidates = frame.targets.filter(t => !dead.has(t.id)).map(t => {
      const distance = Math.hypot(...t.position.map((v, i) => v - frame.eye[i]!));
      return { id: t.id, error: length(subtract(view, direction(frame.eye, t.position))),
        limit: 3 + Math.atan2(24, distance) * 180 / Math.PI };
    }).filter(t => Number.isFinite(t.error) && t.error <= t.limit).sort((a, b) => a.error - b.error);
    // A single recorded victim outweighs approximate torso/recoil geometry.
    // Multiple penetration victims still do not establish an intended target.
    let chosen = hitIds.size === 1
      ? frame.targets.find(t => hitIds.has(t.id) && !dead.has(t.id))?.id ?? null
      : null;
    if (!hitIds.size && candidates[0] && (!candidates[1] || candidates[1].error - candidates[0].error > 0.75)) {
      chosen = candidates[0].id;
    }
    if (!chosen) { current = null; pending = null; }
    else if (chosen !== current) {
      if (pending !== chosen) { pending = chosen; since = frame.tick; }
      if (hitIds.size === 1 || (frame.tick - since) / rate >= 0.05) current = chosen;
      else current = null;
    }
    const result = current;
    for (const c of burst.contacts ?? []) if (c.kill && c.tick <= frame.tick) dead.add(c.targetId);
    return result;
  });
}

interface Motion { tick: number; target: Angles; player: Angles }
/** One lag for a whole target segment. Compare velocity, not closeness to the reference path. */
function segmentMotion(burst: RecoilBurst, reference: RecoilPoint[], frames: RecoilFrame[], target: string, rate: number) {
  const stride = Math.max(1, Math.round(rate * 0.03));
  const motions: Motion[] = [];
  for (let i = stride; i < frames.length; i++) {
    const a = frames[i - stride]!, b = frames[i]!;
    const pa = a.targets.find(t => t.id === target), pb = b.targets.find(t => t.id === target);
    if (!pa || !pb || b.tick - a.tick !== stride) continue;
    const dt = stride / rate;
    // Hold the eye fixed for enemy motion, and remove the shooter's translation from view motion.
    const targetMove = subtract(direction(b.eye, pb.position), direction(b.eye, pa.position));
    const selfMove = subtract(direction(b.eye, pa.position), direction(a.eye, pa.position));
    const viewMove = subtract(firingView(burst, reference, b), firingView(burst, reference, a));
    motions.push({ tick: b.tick, target: targetMove.map(v => v / dt) as Angles,
      player: [(viewMove[0] - selfMove[0]) / dt, wrap(viewMove[1] - selfMove[1]) / dt] });
  }
  const byTick = new Map(motions.map(m => [m.tick, m]));
  const maxLag = Math.floor(rate * MAX_REACTION_SECONDS);
  // Keep the same observations for every candidate lag, so larger lags cannot win by dropping errors.
  const observations = motions.filter(m => m.tick >= (motions[0]?.tick ?? Infinity) + maxLag);
  let lag: number | null = null;
  const changing = observations.filter(m => length(velocityChange(m.target, byTick.get(m.tick - stride)?.target ?? m.target)) > 3);
  if (observations.length / rate >= 0.25 && changing.length >= 3) {
    const costs = Array.from({ length: maxLag + 1 }, (_, delay) => {
      let error = 0, energy = 0;
      for (const m of observations) {
        const past = byTick.get(m.tick - delay);
        if (!past) return Infinity;
        error += length(velocityChange(m.player, past.target)) ** 2;
        energy += length(past.target) ** 2;
      }
      return energy > observations.length * 4 ? error / energy : Infinity;
    });
    const best = costs.indexOf(Math.min(...costs));
    // A weak fit is unknown, not a reason to optimise away a spray error.
    if (costs[best]! < 0.35 && (best === 0 || costs[best]! < costs[0]! * 0.7)) lag = best;
  }
  // Empty/short tracks and slow-moving targets do not prove zero tracking delay.
  if (motions.length >= Math.ceil(rate * 0.05) && motions.every(m => length(m.target) < 0.1)) lag = 0;
  const reactions: [number, number][] = [];
  for (let i = stride; i < motions.length; i++) {
    const a = motions[i - stride]!, b = motions[i]!;
    if (b.tick - a.tick !== stride) continue;
    const change = length(velocityChange(b.target, a.target));
    if (change < Math.max(6, length(a.target) * 0.65)) continue;
    const start = b.tick - stride;
    if (reactions.length && start <= reactions[reactions.length - 1]![1]) continue;
    // If a directional response is already apparent, end the grace interval early.
    const delta = velocityChange(b.target, a.target);
    const response = motions.find((m, index) => {
      if (m.tick < start || m.tick > start + maxLag) return false;
      const next = motions[index + 1];
      return next?.tick === m.tick + 1 && [m, next].every(v => {
        const adjustment = velocityChange(v.player, a.player);
        return length(adjustment) >= length(delta) * 0.5 && dot(adjustment, delta) > length(adjustment) * length(delta) * 0.7;
      });
    });
    reactions.push([start, Math.min(start + maxLag, response?.tick ?? start + (lag ?? maxLag))]);
  }
  return { lag, reactions };
}

/** Remove only the component shared by every plausible delay, never pick the best-looking delay. */
function sharedMovement(frames: Map<number, RecoilFrame>, target: string, previous: RecoilShot, shot: RecoilShot, rate: number): Angles | null {
  const moves: Angles[] = [];
  for (let delay = 0; delay <= Math.floor(rate * MAX_REACTION_SECONDS); delay++) {
    const a = frames.get(previous.tick - delay)?.targets.find(t => t.id === target);
    const b = frames.get(shot.tick - delay)?.targets.find(t => t.id === target);
    if (!a || !b) return null;
    moves.push(subtract(direction(shot.origin, b.position), direction(previous.origin, a.position)));
  }
  const common = (axis: number) => {
    const low = Math.min(...moves.map(v => v[axis]!)), high = Math.max(...moves.map(v => v[axis]!));
    return low > 0 ? low : high < 0 ? high : 0;
  };
  return [common(0), common(1)];
}

export function analyseSpray(burst: RecoilBurst, reference: RecoilPoint[], rate: number): SprayAnalysis {
  const result: SprayAnalysis = { points: burst.shots.map(() => null), states: burst.shots.map(() => 'unknown'),
    targets: burst.shots.map(() => null), segments: burst.shots.map(() => -1), delayMs: burst.shots.map(() => null) };
  if (!Number.isFinite(rate) || rate <= 0 || !burst.tracking?.length) return result;
  const frames = burst.tracking;
  const history = new Map(frames.map(f => [f.tick, f]));
  const targets = assignTargets(burst, reference, rate);
  let segment = 0;
  let previousTarget: string | null = null;
  let anchor: Angles | null = null, anchorRecoil: RecoilPoint | undefined;
  for (let start = 0; start < frames.length;) {
    const target = targets[start];
    let end = start + 1;
    while (end < frames.length && targets[end] === target && frames[end]!.tick === frames[end - 1]!.tick + 1) end++;
    const indices = burst.shots.map((s, i) => s.tick >= frames[start]!.tick && s.tick <= frames[end - 1]!.tick ? i : -1).filter(i => i >= 0);
    if (!target) {
      const nextTarget = targets.slice(end).find(t => t !== null);
      for (const i of indices) result.states[i] = previousTarget && nextTarget && nextTarget !== previousTarget ? 'transfer' : 'unknown';
      start = end; continue;
    }
    if (previousTarget !== target) { anchor = null; anchorRecoil = undefined; }
    previousTarget = target;
    const context = frames.slice(start, end);
    const { lag, reactions } = segmentMotion(burst, reference, context, target, rate);
    const byTick = new Map(context.map(f => [f.tick, f]));
    let partial: Angles | null = null;
    let previousShot: RecoilShot | null = null;
    let previousReaction = false;
    for (const i of indices) {
      const shot = burst.shots[i]!;
      result.targets[i] = target; result.segments[i] = segment;
      result.delayMs[i] = lag === null ? null : lag * 1000 / rate;
      const reacting = reactions.some(([a, b]) => shot.tick >= a && shot.tick < b);
      if (!reference[i]) continue;
      let angles: Angles;
      if (lag === null) {
        let move: Angles | null = [0, 0];
        if (previousShot && partial) {
          move = !reacting && !previousReaction ? sharedMovement(history, target, previousShot, shot, rate) : null;
          const input = subtract([shot.viewPitch, shot.viewYaw], [previousShot.viewPitch, previousShot.viewYaw]);
          partial = [partial[0] + input[0] - (move?.[0] ?? 0), wrap(partial[1] + input[1] - (move?.[1] ?? 0))];
        } else partial = [-reference[i]!.y, -reference[i]!.x];
        previousShot = shot; previousReaction = reacting;
        result.states[i] = reacting ? 'reaction' : move ? 'partialTracking' : 'timingUnknown';
        if (reacting || !move) continue;
        angles = partial;
      } else {
        if (reacting) { result.states[i] = 'reaction'; continue; }
        const now = byTick.get(shot.tick), past = byTick.get(shot.tick - lag);
        const point = past?.targets.find(t => t.id === target)?.position;
        if (!now || !point) continue;
        const residual = subtract([shot.viewPitch, shot.viewYaw], direction(shot.origin, point));
        if (!anchor) { anchor = residual; anchorRecoil = reference[i]; }
        angles = [residual[0] - anchor[0] - anchorRecoil!.y, wrap(residual[1] - anchor[1] - anchorRecoil!.x)];
        result.states[i] = 'tracking';
      }
      const pitch = angles[0] * Math.PI / 180;
      const yaw = angles[1] * Math.PI / 180;
      if (Math.cos(pitch) * Math.cos(yaw) <= 1e-6) continue;
      result.points[i] = { x: -1000 * Math.tan(yaw), y: -1000 * Math.tan(pitch) / Math.cos(yaw), samples: 1 };
    }
    segment++; start = end;
  }
  return result;
}
