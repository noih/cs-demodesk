import { draw, DEFAULT_TOGGLES } from '../src/replay/draw.ts';
import assert from 'node:assert/strict';
import test from 'node:test';
import { Replay } from '../src/replay/engine.ts';
import { FLAG } from '../src/api.ts';

const DEFUSING = 256;
function replay(frames, events) {
  return new Replay({
    schemaVersion: 3, tickRate: 64, step: 4, firstTick: frames[0].t,
    lastTick: frames.at(-1).t, players: [{ steamid: 'ct0', name: 'CT 0' }, { steamid: 'ct1', name: 'CT 1' }],
    weapons: [''], frames, events,
  }, {
    rounds: [{ round: 9, startTick: 41266, freezeEndTick: 42500, endTick: 48743, officiallyEndedTick: 49191, roster: { ct0: 'CT', ct1: 'CT' } }],
    info: { players: [] }, stats: [], roundSummaries: [],
  }, []);
}
function frame(t, active = -1, alive = true) {
  return { t, p: [0, 1].map((pid) => [pid, 0, 0, 0, 0, alive ? 100 : 0, 0, (alive ? FLAG.alive : 0) | (pid === active ? DEFUSING : 0), 0, 0]), g: [] };
}

// Round 9 transitions from the reported demo; it contains no bomb_abortdefuse events.
test('round 9 fake defuses stop at the first sampled release, including seeking', () => {
  const r = replay([
    frame(47628), frame(47632, 0), frame(47644, 0), frame(47648),
    frame(47916), frame(47920, 1), frame(47936, 1), frame(47940),
    frame(48228), frame(48232, 1), frame(48424, 1), frame(48428), frame(48744),
  ], [
    { t: 46119, k: 'plant' }, { t: 47631, k: 'defuseStart', p: 0, kit: false },
    { t: 47917, k: 'defuseStart', p: 1, kit: false }, { t: 48231, k: 'defuseStart', p: 1, kit: false },
    { t: 48743, k: 'explode' },
  ]);
  for (const tick of [47648, 47940, 48428, 48740]) assert.equal(r.stateAt(tick).bomb.defuse, undefined, `released at ${tick}`);
  for (const tick of [47631, 47644, 47920, 48232, 48424]) assert.ok(r.stateAt(tick).bomb.defuse, `active at ${tick}`);
  assert.equal(r.stateAt(47917).bomb.defuse.remaining, 10);
  assert.equal(r.stateAt(48744).bomb.state, 'exploded');
});

test('a tap shorter than one replay sample cannot leave a countdown running', () => {
  const r = replay([frame(47628), frame(47632), frame(47636)], [{ t: 46119, k: 'plant' }, { t: 47630, k: 'defuseStart', p: 0, kit: false }]);
  assert.ok(r.stateAt(47630).bomb.defuse);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('the state belongs to the defusing player, not another CT', () => {
  const r = replay([frame(47628), frame(47632, 1)], [{ t: 46119, k: 'plant' }, { t: 47630, k: 'defuseStart', p: 0 }]);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('normal kit and no-kit defuses keep their countdown until completion', () => {
  for (const kit of [true, false]) {
    const needs = kit ? 5 : 10, since = 47632, finish = since + needs * 64;
    const r = replay([frame(since, 0), frame(since + 64, 0), frame(finish)], [
      { t: 46119, k: 'plant' }, { t: since, k: 'defuseStart', p: 0, kit }, { t: finish, k: 'defuse', p: 0 },
    ]);
    assert.equal(r.stateAt(since + 64).bomb.defuse.remaining, needs - 1);
    assert.equal(r.stateAt(finish).bomb.state, 'defused');
    assert.equal(r.stateAt(finish).bomb.defuse, undefined);
  }
});

test('an explicit abort stops immediately even before the next player sample', () => {
  const r = replay([frame(47628, 0), frame(47632, 0)], [
    { t: 46119, k: 'plant' }, { t: 47628, k: 'defuseStart', p: 0 }, { t: 47630, k: 'defuseAbort', p: 0 },
  ]);
  assert.ok(r.stateAt(47629).bomb.defuse);
  assert.equal(r.stateAt(47630).bomb.defuse, undefined);
});

test('a dead defuser cannot keep a countdown active', () => {
  const r = replay([frame(47628, 0), frame(47632, 0, false)], [
    { t: 46119, k: 'plant' }, { t: 47628, k: 'defuseStart', p: 0 },
  ]);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('player labels stay above every marker and focus is painted last', () => {
  const calls = [];
  const ctx = new Proxy({}, {
    get(target, key) {
      if (key in target) return target[key];
      if (key === 'measureText') return text => ({ width: text.length * 8 });
      return (...args) => calls.push({ key, args, color: target.fillStyle });
    },
  });
  const player = { x: 100, y: 100, z: 0, yaw: 0, hp: 100, alive: true, team: 'CT' };
  const players = [{ ...player, pid: 1, name: 'Focused' }, { ...player, pid: 2, name: 'Other' }];
  const map = { posX: 0, posY: 0, scale: 1, layers: [{ altitudeMin: -100, altitudeMax: 100 }] };
  const state = { players, grenades: [], effects: [], shots: [], deaths: [] };
  draw(ctx, 500, 500, map, [], state, { ...DEFAULT_TOGGLES, sound: 'off' }, { zoom: 2, panX: 0, panY: 0 }, 1);
  const firstLabel = calls.findIndex(call => call.key === 'fillText');
  assert.ok(firstLabel > calls.findLastIndex(call => call.key === 'arc'));
  assert.deepEqual(calls.filter(call => call.key === 'fillText').map(call => call.args[0]), ['Other', 'Focused']);
  assert.equal(calls.filter(call => call.key === 'fillRect')[2].color, '#b995ff');
  assert.deepEqual(players.map(p => p.pid), [1, 2], 'Drawing must not reorder replay state');
});

test('sampled fire cells disappear and seeking restores their recorded state', () => {
  const r = replay([frame(100), frame(104), frame(108)], []);
  r.data.schemaVersion = 5;
  r.data.frames[0].f = [[10, 20, 30], [40, 50, 30]];
  r.data.frames[1].f = [[10, 20, 30]];
  assert.deepEqual(r.stateAt(102).fireCells, [[10, 20, 30], [40, 50, 30]]);
  assert.deepEqual(r.stateAt(104).fireCells, [[10, 20, 30]]);
  assert.deepEqual(r.stateAt(108).fireCells, []);
  assert.equal(r.stateAt(100).fireCells.length, 2);
});
