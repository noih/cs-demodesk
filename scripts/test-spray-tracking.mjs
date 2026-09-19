import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyseSpray } from '../src/sprayTracking.ts';
import { projectReference, projectShots, withOriginalFallback } from '../src/recoil.ts';

function scenario({ rate = 100, delay = 0, motion = () => [0, 0], error = () => 0 } = {}) {
  const reference = Array.from({ length: 21 }, (_, i) => ({ x: i * 0.05, y: -i * 0.2, samples: 1 }));
  const position = t => { const [pitch, yaw] = motion(t); return [1000, 1000 * Math.tan(yaw * Math.PI / 180), 64 - 1000 * Math.tan(pitch * Math.PI / 180) / Math.cos(yaw * Math.PI / 180)]; };
  const view = t => { const [p, y] = motion(t - delay); const index = Math.max(0, Math.min(20, (t - 1) * 10)); return [p + index * 0.2, y - index * 0.05 + error(t)]; };
  const tracking = Array.from({ length: Math.round(2.25 * rate) + 1 }, (_, i) => {
    const tick = Math.round(0.75 * rate) + i;
    return { tick, eye: [0, 0, 64], view: view(tick / rate), targets: [{ id: 'enemy-a', position: position(tick / rate) }] };
  });
  const shots = reference.map((_, i) => { const tick = Math.round(rate * (1 + i / 10)); const [viewPitch, viewYaw] = view(tick / rate); return { tick, origin: [0, 0, 64], viewPitch, viewYaw }; });
  return { burst: { round: 1, startTick: rate, shots, tracking }, reference, rate };
}
function analyse(s) { return analyseSpray(s.burst, s.reference, s.rate); }

test('constant tracking motion is corrected without inventing a delay', () => {
  const s = scenario({ delay: 0.15, motion: t => [0, t * 1.5] });
  const result = analyse(s);
  assert.ok(result.delayMs.every(ms => ms === null));
  const expected = projectReference(s.reference);
  assert.ok(result.points.every(Boolean));
  result.points.forEach((p,i) => assert.ok(Math.hypot(p.x-expected[i].x,p.y-expected[i].y)<1e-6));
  assert.ok(Math.abs(result.points.at(-1).x-projectShots(s.burst.shots).at(-1).x)>30);
});

test('unknown delay does not subtract an enemy jump before a response', () => {
  const s = scenario({ delay: 0.2, motion: t => [t < 1.15 ? 0 : -(t - 1.15) * 10, 0] });
  s.burst.shots = s.burst.shots.slice(0,4);
  s.burst.tracking = s.burst.tracking.filter(f => f.tick <= 130);
  const result = analyse(s), raw = projectShots(s.burst.shots);
  assert.ok(result.delayMs.every(ms => ms === null));
  const shown = withOriginalFallback(raw,result.points);
  shown.forEach((p,i) => assert.ok(Math.hypot(p.x-raw[i].x,p.y-raw[i].y)<1e-6));
});

test('stationary target retains compensation and does not erase sustained aim errors', () => {
  const s = scenario();
  const expected = projectReference(s.reference);
  analyse(s).points.forEach((p, i) => assert.ok(Math.hypot(p.x - expected[i].x, p.y - expected[i].y) < 1e-8));
  const off = scenario({ error: t => t > 1.5 ? 1 : 0 });
  assert.ok(Math.abs(analyse(off).points.at(-1).x - projectReference(off.reference).at(-1).x) > 15);
});

test('a single hit establishes the target outside approximate geometry without erasing aim error', () => {
  const s = scenario({ error: t => t >= 1.5 ? 8 : 0 });
  s.burst.contacts = [{ tick: 160, targetId: 'enemy-a', kill: false }];
  const result = analyse(s);
  assert.equal(result.targets[6], 'enemy-a');
  const shown = withOriginalFallback(projectShots(s.burst.shots), result.points);
  assert.ok(Math.abs(shown[6].x - projectReference(s.reference)[6].x) > 100);

  // A victim without a position cannot supply a correction, nor can multiple victims establish intent.
  s.burst.contacts.push({ tick: 160, targetId: 'enemy-b', kill: false });
  assert.equal(analyse(s).targets[6], null);
  s.burst.contacts = [{ tick: 160, targetId: 'missing-enemy', kill: false }];
  assert.equal(analyse(s).targets[6], null);
});

test('moving target is removed; a single segment delay is bounded to 200 ms', () => {
  for (const rate of [64, 100, 128]) for (const delay of [0, 0.15, 0.2]) {
    const s = scenario({ rate, delay, motion: t => [0, 1.5 * Math.sin(t * 9)] });
    const result = analyse(s);
    const delays = result.delayMs.filter(v => v !== null);
    assert.ok(delays.length > 10);
    assert.ok(delays.every(v => v <= 200));
    assert.ok(Math.abs(delays.at(-1) - delay * 1000) <= 1000 / rate + 1, `${rate}, ${delay}: ${delays.at(-1)}`);
    assert.ok(result.points.filter(Boolean).length >= 5);
  }
});

test('a sudden jump leaves reaction shots unclassified, and never grants more than 200 ms', () => {
  const s = scenario({ delay: 0.18, motion: t => [t < 1.5 ? 0 : -Math.min(5, (t - 1.5) * 10), 0] });
  const result = analyse(s);
  assert.equal(result.states[6], 'reaction');
  assert.equal(result.points[6], null);
  assert.notEqual(result.states[8], 'reaction');
  const fast = scenario({ delay: 0, motion: t => [t < 1.5 ? 0 : -Math.min(5, (t - 1.5) * 10), 0] });
  assert.notEqual(analyse(fast).states[6], 'reaction');
});

test('kill then transfer keeps shot index and separates targets; a kill alone is not a transfer', () => {
  const s = scenario();
  s.burst.contacts = [{ tick: 150, targetId: 'enemy-a', kill: true }];
  for (const f of s.burst.tracking) if (f.tick > 150) {
    f.targets = [{ id: 'enemy-b', position: [1000, 200, 64] }];
    if (f.tick >= 170) f.view[1] += Math.atan2(200, 1000) * 180 / Math.PI;
  }
  for (const shot of s.burst.shots) if (shot.tick >= 170) shot.viewYaw += Math.atan2(200, 1000) * 180 / Math.PI;
  const result = analyse(s);
  assert.equal(result.states[6], 'transfer');
  assert.equal(result.targets[8], 'enemy-b');
  assert.notEqual(result.segments[4], result.segments[8]);
  assert.ok(Math.abs(result.points[8].y - projectReference(s.reference)[8].y) < 1e-8);
  for (const f of s.burst.tracking) if (f.tick > 150) f.targets = [];
  assert.equal(analyse(s).states[6], 'unknown');
});

test('ambiguous targets, multi-hit penetration and missing samples are not corrected', () => {
  const s = scenario();
  for (const f of s.burst.tracking) f.targets.push({ id: 'enemy-b', position: [...f.targets[0].position] });
  assert.ok(analyse(s).points.every(p => p === null));
  s.burst.contacts = [{ tick: 160, targetId: 'enemy-a', kill: false }, { tick: 160, targetId: 'enemy-b', kill: false }];
  assert.equal(analyse(s).states[6], 'unknown');
  const missing = scenario();
  missing.burst.tracking = missing.burst.tracking.filter(f => f.tick !== 160);
  assert.equal(analyse(missing).points[6], null);
  assert.ok(analyseSpray({ ...s.burst, tracking: undefined }, s.reference, 100).points.every(p => p === null));
});

test('late reactions and reacquiring the same target cannot erase control errors', () => {
  const late = scenario({ delay: 0.35, motion: t => [0, 1.5 * Math.sin(t * 9)] });
  const result = analyse(late), expected = projectReference(late.reference);
  assert.ok(result.delayMs.every(ms => ms === null || ms <= 200));
  assert.ok(withOriginalFallback(projectShots(late.burst.shots), result.points).some((p, i) => p && Math.abs(p.x - expected[i].x) > 10));
  const gap = scenario({ error: t => t > 1.5 ? 1 : 0 });
  gap.burst.tracking = gap.burst.tracking.filter(f => f.tick !== 160);
  const recovered = analyse(gap);
  assert.ok(Math.abs(recovered.points.at(-1).x - projectReference(gap.reference).at(-1).x) > 15);
  assert.ok(!recovered.states.includes('transfer'));
});
