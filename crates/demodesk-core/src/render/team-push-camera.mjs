import { createCameraTransition } from './camera-transition.mjs';

const angleDifference=(a,b)=>((b-a+180)%360+360)%360-180;
const clamp=(x,limit)=>Math.max(-limit,Math.min(limit,x));

// Offline trajectory input keeps both subjects in the composition after a death.
export function planEncounterAim(samples, encounter, tickRate=64) {
  const [x,y,z]=encounter.position;
  const angles=target=>({yaw:Math.atan2(target[1]-y,target[0]-x)*180/Math.PI,pitch:Math.atan2(z-target[2],Math.hypot(target[0]-x,target[1]-y))*180/Math.PI});
  const base=angles(encounter.target);
  const centerOf=sample=>[0,1,2].map(i=>sample.subjects.reduce((sum,p)=>sum+p[i],0)/sample.subjects.length+(i===2?36:0));
  const initial=samples.length?angles(centerOf(samples[0])):base;
  let yaw=0,pitch=0,lastTick=samples[0]?.tick;
  return samples.map(sample=>{
    const desired=angles(centerOf(sample));
    const alpha=1-Math.exp(-Math.max(0,sample.tick-lastTick)/(tickRate*0.45));
    yaw+=(clamp(angleDifference(initial.yaw,desired.yaw)*0.35,2)-yaw)*alpha;
    pitch+=(clamp(desired.pitch-initial.pitch,1)*0.35-pitch)*alpha;
    lastTick=sample.tick;
    return {tick:sample.tick,yaw:base.yaw+yaw,pitch:base.pitch+pitch};
  });
}

function viewAt(shot,tick) {
  const {position:[x,y,z],target:[tx,ty,tz],fov}=shot.camera;
  let rX=Math.atan2(z-tz,Math.hypot(tx-x,ty-y))*180/Math.PI,rY=Math.atan2(ty-y,tx-x)*180/Math.PI;
  const path=shot.aimPath;
  if(path.length){const index=path.findIndex(p=>p.tick>=tick);const b=index<0?path.at(-1):path[index],a=index>0?path[index-1]:b;const t=b.tick===a.tick?0:Math.max(0,Math.min(1,(tick-a.tick)/(b.tick-a.tick)));rX=a.pitch+(b.pitch-a.pitch)*t;rY=a.yaw+angleDifference(a.yaw,b.yaw)*t;}
  return {x,y,z,rX,rY,rZ:0,fov};
}

function framed(samples,shot) {
  const tan=Math.tan(shot.camera.fov*Math.PI/360);
  return samples.every(s=>{
    const v=viewAt(shot,s.tick),yaw=v.rY*Math.PI/180,pitch=v.rX*Math.PI/180;
    const f=[Math.cos(pitch)*Math.cos(yaw),Math.cos(pitch)*Math.sin(yaw),-Math.sin(pitch)],r=[Math.sin(yaw),-Math.cos(yaw),0],u=[Math.sin(pitch)*Math.cos(yaw),Math.sin(pitch)*Math.sin(yaw),Math.cos(pitch)];
    const dot=(a,b)=>a.reduce((sum,x,i)=>sum+x*b[i],0);
    return s.subjects.every(p=>[-16,16].every(dx=>[-16,16].every(dy=>[0,72].every(dz=>{const d=[p[0]+dx-v.x,p[1]+dy-v.y,p[2]+dz-v.z],depth=dot(d,f);return depth>16&&Math.abs(dot(d,r)/depth/tan)<0.95&&Math.abs(dot(d,u)/depth/tan*4/3)<0.95;}))));
  });
}

export function startTeamPushSequence({steamId,tickRate,shots}) {
  const id='demodesk/team-push';
  shots=shots.map(s=>({...s,aimPath:planEncounterAim(s.samples,s.camera,tickRate)}));
  for(const shot of shots)if(!framed(shot.samples,shot))shot.aimPath=[];
  let lastShot,lastView,transition,transitionSource,transitionTick,lastTick;
  const end=shots.at(-1)?.endTick??0;
  mirv.events.cViewRenderSetupView.on(id,()=>{
    const tick=mirv.getDemoTick();if(tick>=end+tickRate*0.125){mirv.events.cViewRenderSetupView.off(id);return;}
    if(!mirv.isPlayingDemo())return;
    if(lastTick!==undefined&&tick<lastTick){lastShot=lastView=transition=undefined;}
    lastTick=tick;
    const shot=shots.find(s=>tick>=s.startTick&&tick<s.endTick);
    if(shot){
      const view=viewAt(shot,tick);
      if(lastShot!==shot){
        // Only bridge contiguous, already qualified camera shots; seeks/POV gaps cut.
        transitionSource=lastView;
        transition=lastShot&&lastShot.endTick===shot.startTick&&shot.transitionSafe?createCameraTransition(lastView,view):undefined;
        transitionTick=tick;lastShot=shot;
      }
      lastView=transition?transition(transitionSource,view,(tick-transitionTick)/tickRate):view;
      return lastView;
    }
    if(!lastShot||tick-lastShot.endTick>=tickRate*0.125)return;
    // Hold the main eye across the asynchronous native spectator-mode change.
    for(let i=1;i<=64;i++){
      const c=mirv.getEntityFromIndex(i);if(!c?.isPlayerController()||c.getSteamId().toString()!==steamId)continue;
      const h=c.getPlayerPawnHandle();if(!mirv.isHandleValid(h))return;
      const p=mirv.getEntityFromIndex(mirv.getHandleEntryIndex(h));if(!p?.isValid()||p.getHealth()<=0)return;
      const eye=p.getRenderEyeOrigin(),angles=p.getRenderEyeAngles();if(![...eye,...angles].every(Number.isFinite))return;
      return {x:eye[0],y:eye[1],z:eye[2],rX:angles[0],rY:angles[1],rZ:0};
    }
  });
}
