import test from 'node:test';
import assert from 'node:assert/strict';
import { openingView } from '../crates/demodesk-core/src/render/opening-camera.mjs';
const config = { startTick: 100, endTick: 207, tickRate: 64, path: [
  { tick: 96, position: [-80, -40, 72], target: [128, 0, 64] }, { tick: 216, position: [-80, -40, 72], target: [128, 0, 64] },
] };
const eye = [0, 0, 64], angles = [0, 0, 0];
test('stationary shoulder looks forward instead of down into the player', () => {
  const view = openingView(config, 100, eye, angles);
  assert.equal(view.x, -80); assert.equal(view.y, -40);
  assert.ok(Math.abs(view.rX) < 5); assert.ok(Math.abs(view.rY) < 15);
  assert.deepEqual(openingView(config, 120, eye, angles), view);
});
test('approach accelerates into the exact native eye pose without a snap back', () => {
  const early = openingView(config, 180, eye, angles);
  const late = openingView(config, 200, eye, angles);
  assert.ok(Math.abs(late.x) < Math.abs(early.x));
  assert.deepEqual(openingView(config, 207, eye, angles), { x: 0, y: 0, z: 64, rX: 0, rY: 0, rZ: 0 });
  assert.deepEqual(openingView(config, 208, eye, angles), openingView(config, 207, eye, angles));
  assert.equal(openingView(config, 99, eye, angles), undefined);
});
