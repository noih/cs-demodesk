import assert from 'node:assert/strict';
import { test } from 'node:test';
import { averagePaths, projectReference, projectShots, stepRecoilZoom } from '../src/recoil.ts';
const shot = (origin = [0,0,64], pitch = 0, yaw = 0) => ({ tick:0, origin, viewPitch: pitch, viewYaw: yaw });
const close = (a,b) => assert.ok(Math.abs(a-b)<1e-8, `${a} != ${b}`);

test('view trajectory corrects movement and crouch height at fixed 10 m', () => {
  const points=projectShots([shot(),shot([0,-20,64]),shot([0,-20,54]),shot([50,-20,46],-5,-4)]);
  close(points[1].x,50.8);
  close(points[2].y,-25.4);
  const targetYaw=Math.atan2(50.8,873), targetPitch=-Math.atan2(45.72,Math.hypot(873,50.8));
  const correctedYaw=-4*Math.PI/180-targetYaw, correctedPitch=-5*Math.PI/180-targetPitch;
  close(points[3].x,-1000*Math.tan(correctedYaw));
  close(points[3].y,-1000*Math.tan(correctedPitch)/Math.cos(correctedYaw));
});

test('tracking a fixed target while moving and crouching stays centred', () => {
  const points=projectShots([[0,0,64],[20,30,54],[40,-30,46]].map(origin=>{
    const dx=1000/2.54-origin[0],dy=-origin[1],dz=64-origin[2];
    return shot(origin,-Math.atan2(dz,Math.hypot(dx,dy))*180/Math.PI,Math.atan2(dy,dx)*180/Math.PI);
  }));
  for(const p of points) {close(p.x,0);close(p.y,0);}
});

test('perfect compensation follows the reference instead of collapsing to a point', () => {
  const angles=[{x:0,y:0,samples:1},{x:-2,y:-5,samples:1}];
  const reference=projectReference(angles);
  assert.deepEqual(reference,projectShots(angles.map(a=>shot([0,0,0],-a.y,-a.x))));
  assert.ok(reference[1].x<0 && reference[1].y<0);
  close(projectShots([shot([0,0,0],0,179),shot([0,0,0],0,-179)])[1].x,-1000*Math.tan(2*Math.PI/180));
  close(projectShots([shot([0,0,0],45,90),shot([0,0,0],40,90)])[1].y,1000*Math.tan(5*Math.PI/180));
});

test('average preserves individual paths, missing intersections and shot sample counts', () => {
  const a=projectShots([shot(),shot([0,-10,64]),shot([0,-20,64]),shot([0,-30,64])]);
  const b=projectShots([shot(),shot([0,10,64]),shot([0,0,64],0,90)]);
  const before=structuredClone([a,b]);
  const mean=averagePaths([a,b]);
  assert.equal(b[2],null);
  assert.deepEqual(mean.map(p=>p.samples),[2,2,1,1]);
  close(mean[1].x,0);close(mean[2].x,50.8);
  assert.deepEqual([a,b],before);
  assert.deepEqual(averagePaths([[null,null],[null]]),[null,null]);
  assert.equal(projectShots([shot(),shot([500,0,64])])[1],null);
  assert.equal(projectShots([shot(),shot([0,0,64],0,180)])[1],null);
});

test('not compensating leaves the view path at zero despite growing bullet recoil', () => {
  const recoil=[{x:0,y:0,samples:1},{x:-2,y:-5,samples:1},{x:1,y:-10,samples:1}];
  const view=projectShots(recoil.map(()=>shot()));
  for(const p of view) { close(p.x,0);close(p.y,0); }
  assert.ok(Math.abs(projectReference(recoil)[2].y)>100);
});

test('one fixed reference overlaps ideal control across orientations, movement and crouch transitions', () => {
  const recoil=[{x:0,y:0,samples:1},{x:-2,y:-5,samples:1},{x:1,y:-10,samples:1}];
  const paths=[]; const references=[];
  for (const [pitch,yaw] of [[45,179],[-35,-170],[0,90]]) {
    const p=pitch*Math.PI/180,y=yaw*Math.PI/180;
    const target=[Math.cos(p)*Math.cos(y)*1000/2.54,Math.cos(p)*Math.sin(y)*1000/2.54,64-Math.sin(p)*1000/2.54];
    const origins=[[0,0,64],[15,-20,54],[30,10,46]];
    const ideal=origins.map((origin,i)=>{
      const [dx,dy,dz]=target.map((v,j)=>v-origin[j]);
      return shot(origin,-Math.atan2(dz,Math.hypot(dx,dy))*180/Math.PI-recoil[i].y,Math.atan2(dy,dx)*180/Math.PI-recoil[i].x);
    });
    const reference=projectReference(recoil), actual=projectShots(ideal);
    for(let i=0;i<actual.length;i++){close(actual[i].x,reference[i].x);close(actual[i].y,reference[i].y);}
    // Normalization must preserve player errors against the fixed standard.
    const wrong=ideal.map((s,i)=>i?{...s,viewPitch:s.viewPitch+3,viewYaw:s.viewYaw-4}:s);
    assert.ok(Math.hypot(projectShots(wrong)[1].x-reference[1].x,projectShots(wrong)[1].y-reference[1].y)>10);
    paths.push(actual);references.push(reference);
  }
  const mean=averagePaths(paths),referenceMean=averagePaths(references);
  for(let i=0;i<mean.length;i++){close(mean[i].x,referenceMean[i].x);close(mean[i].y,referenceMean[i].y);}
  assert.deepEqual(projectShots([]),[]);
  assert.equal(projectShots([shot(),shot([1000/2.54,0,64])])[1],null);
});

test('different burst lengths share the complete fixed reference without padding player paths', () => {
  const recoil=Array.from({length:30},(_,i)=>({x:Math.sin(i)*2,y:-i/3,samples:1}));
  const ideal=recoil.map(p=>shot([0,0,64],-p.y,-p.x));
  const reference=projectReference(recoil);
  const mean=averagePaths([projectShots(ideal.slice(0,3)),projectShots(ideal.slice(0,20))]);
  assert.equal(reference.length,30);
  assert.equal(mean.length,20);
  for(let i=0;i<mean.length;i++){close(mean[i].x,reference[i].x);close(mean[i].y,reference[i].y);}
});

test('zoom follows 20% marks and snaps inward from both end stops', () => {
  const marks=[0.05,...Array.from({length:12},(_,i)=>(i+1)/5),2.5];
  for(let i=0;i<marks.length;i++) {
    assert.equal(stepRecoilZoom(marks[i],1),marks[Math.min(i+1,marks.length-1)]);
    assert.equal(stepRecoilZoom(marks[i],-1),marks[Math.max(i-1,0)]);
  }
});
