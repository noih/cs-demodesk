import assert from 'node:assert/strict';
import test from 'node:test';
import { startAimFollow } from '../crates/demodesk-core/src/render/utility-camera.mjs';

test('flash and HE chase preserve direction, bridge the camera and release at the cut', () => {
  for (const className of ['flashbang_projectile', 'hegrenade_projectile']) {
    let callback, removed = false, tick = 100, alive = true;
    const pawn = { isValid:()=>true, isPlayerPawn:()=>true, getHealth:()=>100,
      getRenderEyeOrigin:()=>[100,100,64], getRenderEyeAngles:()=>[20,90,0] };
    const controller = { isValid:()=>true, isPlayerController:()=>true,
      getSteamId:()=>1n, getPlayerPawnHandle:()=>65 };
    const projectile = { isValid:()=>alive, getClassName:()=>className, getOrigin:()=>[150,100,80] };
    globalThis.mirv = {
      events:{cViewRenderSetupView:{on:(_,fn)=>callback=fn,off:()=>removed=true}},
      getDemoTick:()=>tick, isPlayingDemo:()=>true,
      getEntityFromIndex:i=>i===1?controller:i===65?pawn:i===506?projectile:null,
      isHandleValid:()=>true, getHandleEntryIndex:i=>i, message:()=>{},
    };
    try {
      startAimFollow({steamId:'1',startTick:100,endTick:120,grenade:{throwTick:105,entityIndex:506,className}});
      const shoulder=callback({curTime:1});
      assert.equal(shoulder.rY,90);
      tick=105;
      assert.deepEqual(callback({curTime:1.1}),shoulder);
      tick=119;
      const flight=callback({curTime:1.4});
      assert.ok(Math.abs(flight.x-150)<1e-9);
      assert.equal(flight.y,44);
      assert.equal(flight.z,98);
      assert.equal(flight.rY,90);
      alive=false;
      assert.deepEqual(callback({curTime:1.5}),flight);
      tick=120;
      assert.equal(callback({curTime:1.6}),undefined);
      assert.ok(removed);
    } finally { delete globalThis.mirv; }
  }
});
