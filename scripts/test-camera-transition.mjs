import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createCameraTransition } from '../crates/demodesk-core/src/render/camera-transition.mjs';

const view = { x: 0, y: 0, z: 64, rX: 0, rY: 179, rZ: 0, fov: 90 };
const close = { ...view, x: 100, rY: -179, fov: 100 };

test('nearby shots blend position, shortest rotation and FOV over 0.75 seconds', () => {
  const sample = createCameraTransition(view, close);
  assert.deepEqual(sample(view, close, 0), view);
  const middle = sample(view, close, 0.375);
  assert.equal(middle.x, 50);
  assert.equal(middle.rY, 180);
  assert.equal(middle.fov, 95);
  assert.strictEqual(sample(view, close, 0.75), close);
});

test('far position, opposite direction, or very different FOV cuts immediately', () => {
  for (const destination of [{ ...close, x: 201 }, { ...close, rY: 0 }, { ...close, fov: 116 }]) {
    assert.strictEqual(createCameraTransition(view, destination)(view, destination, 0), destination);
  }
});

test('time discontinuity and known obstruction bypass transition', () => {
  for (const options of [{ continuous: false }, { pathBlocked: true }]) {
    assert.strictEqual(createCameraTransition(view, close, options)(view, close, 0), close);
  }
  const sample = createCameraTransition(view, close);
  sample(view, close, 0.3);
  assert.strictEqual(sample(view, close, 0.1), close);
  assert.strictEqual(sample(view, close, 0.4), close);
});

test('a cut stays a cut when targets move back inside the threshold', () => {
  const far = { ...close, x: 500 };
  const sample = createCameraTransition(view, far);
  assert.strictEqual(sample(view, close, 0.2), close);
  const moving = createCameraTransition(view, close);
  assert.strictEqual(moving(view, far, 0.2), far);
  assert.strictEqual(moving(view, close, 0.3), close);
});

test('missing camera data never produces interpolated NaN', () => {
  assert.strictEqual(createCameraTransition({}, close)({}, close, 0), close);
});
