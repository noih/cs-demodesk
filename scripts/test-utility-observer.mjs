import assert from 'node:assert/strict';
import test from 'node:test';
import { startUtilityObserver } from '../crates/demodesk-core/src/render/utility-observer-camera.mjs';

test('observer holds its world position through death and releases at the scheduled cut', () => {
  let callback, tick=99;
  const removed=[];
  const camera={startTick:100,endTick:250,position:[0,-250,80],target:[0,0,36],fov:75,
    samples:[{tick:100,subjects:[[0,0,0]]},{tick:200,subjects:[[5,0,0]]},{tick:250,subjects:[[5,0,0]]}],
    flight:[[100,[-50,0,40]],[200,[0,0,40]]]};
  globalThis.mirv={getDemoTick:()=>tick,isPlayingDemo:()=>true,
    events:{cViewRenderSetupView:{on:(_,fn)=>callback=fn,off:id=>removed.push(id)}},
    getEntityFromIndex:()=>{throw Error('observer must not depend on a living victim');}};
  try {
    startUtilityObserver(camera,64);
    assert.equal(callback(),undefined);
    tick=100;const initial=callback();
    tick=249;const afterDeath=callback();
    assert.deepEqual([afterDeath.x,afterDeath.y,afterDeath.z],camera.position);
    assert.ok(Math.abs(initial.rY-afterDeath.rY)<=2);
    tick=110; assert.ok(callback()); // A demo rewind uses the saved path deterministically.
    tick=250; assert.equal(callback(),undefined);
    assert.equal(removed.at(-1),'demodesk/utility-observer');
  } finally {delete globalThis.mirv;}
});
