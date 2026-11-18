import { planEncounterAim } from './team-push-camera.mjs';

const delta = (a,b) => ((b-a+180)%360+360)%360-180;

export function observerView(camera, path, tick) {
  const [x,y,z] = camera.position, [tx,ty,tz] = camera.target;
  let rY = Math.atan2(ty-y,tx-x)*180/Math.PI;
  let rX = Math.atan2(z-tz,Math.hypot(tx-x,ty-y))*180/Math.PI;
  if(path.length) {
    const i = path.findIndex(p=>p.tick>=tick), b = i<0?path.at(-1):path[i], a = i>0?path[i-1]:b;
    const t = a.tick===b.tick?0:Math.max(0,Math.min(1,(tick-a.tick)/(b.tick-a.tick)));
    rY = a.yaw+delta(a.yaw,b.yaw)*t; rX = a.pitch+(b.pitch-a.pitch)*t;
  }
  return {x,y,z,rX,rY,rZ:0,fov:camera.fov};
}

function framed(camera, path) {
  const points = camera.flight.map(([tick,p])=>({tick,p}));
  for(const sample of camera.samples) for(const p of sample.subjects)
    for(const dx of [-24,24]) for(const dy of [-24,24]) for(const dz of [0,72])
      points.push({tick:sample.tick,p:[p[0]+dx,p[1]+dy,p[2]+dz]});
  const tan = Math.tan(camera.fov*Math.PI/360);
  return points.every(({tick,p})=>{
    const v=observerView(camera,path,tick), yaw=v.rY*Math.PI/180, pitch=v.rX*Math.PI/180;
    const f=[Math.cos(pitch)*Math.cos(yaw),Math.cos(pitch)*Math.sin(yaw),-Math.sin(pitch)];
    const r=[Math.sin(yaw),-Math.cos(yaw),0], u=[Math.sin(pitch)*Math.cos(yaw),Math.sin(pitch)*Math.sin(yaw),Math.cos(pitch)];
    const d=[p[0]-v.x,p[1]-v.y,p[2]-v.z], dot=a=>a.reduce((n,x,i)=>n+x*d[i],0), depth=dot(f);
    return depth>16&&Math.abs(dot(r)/depth/tan)<0.9&&Math.abs(dot(u)/depth/tan*4/3)<0.9;
  });
}

export function startUtilityObserver(camera, tickRate) {
  const id='demodesk/utility-observer';
  let path=planEncounterAim(camera.samples,camera,tickRate);
  if(!framed(camera,path)) path=[];
  mirv.events.cViewRenderSetupView.off(id);
  mirv.events.cViewRenderSetupView.on(id,()=>{
    const tick=mirv.getDemoTick();
    if(tick>=camera.endTick) { mirv.events.cViewRenderSetupView.off(id); return; }
    if(tick<camera.startTick||!mirv.isPlayingDemo()) return;
    // Saved world-space framing survives the victim's death without following the ragdoll.
    return observerView(camera,path,tick);
  });
}
