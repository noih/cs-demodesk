import { createCameraTransition } from '../crates/demodesk-core/src/render/camera-transition.mjs';
export { planEncounterAim } from '../crates/demodesk-core/src/render/team-push-camera.mjs';
const angleDifference=(a,b)=>((b-a+180)%360+360)%360-180;

// Local experiment: same-team nearby players only; no world collision query.
export function startTeamPush({steamId,startTick,endTick,overview=false,frontId,encounter}) {
  const id='demodesk/team-push-experiment'; let yaw, lastLog=-Infinity, targetView, lastTick, transition;
  mirv.events.cViewRenderSetupView.on(id,()=>{
    const tick=mirv.getDemoTick();
    if(tick>=endTick+8){mirv.events.cViewRenderSetupView.off(id);return;}
    if(tick<startTick||!mirv.isPlayingDemo())return;
    const players=[];
    for(let i=1;i<=64;i++){
      const c=mirv.getEntityFromIndex(i); if(!c?.isPlayerController())continue;
      const h=c.getPlayerPawnHandle();if(!mirv.isHandleValid(h))continue;
      const p=mirv.getEntityFromIndex(mirv.getHandleEntryIndex(h));
      if(!p?.isValid()||!p.isPlayerPawn()||p.getHealth()<=0)continue;
      const eye=p.getRenderEyeOrigin(),angles=p.getRenderEyeAngles();
      if([...eye,...angles].every(Number.isFinite))players.push({id:c.getSteamId().toString(),team:p.getTeam(),eye,angles});
    }
    const main=players.find(p=>p.id===steamId);if(!main)return;
    if(tick>=endTick)return {x:main.eye[0],y:main.eye[1],z:main.eye[2],rX:main.angles[0],rY:main.angles[1],rZ:0};
    // Independent shot can use a precomputed, gently damped subject-follow pan.
    if(encounter && tick>=encounter.startTick && tick<encounter.endTick){
      const [x,y,z]=encounter.position;
      const [tx,ty,tz]=encounter.target;
      let pitch=Math.atan2(z-tz,Math.hypot(tx-x,ty-y))*180/Math.PI,yaw=Math.atan2(ty-y,tx-x)*180/Math.PI;
      if(encounter.aimPath?.length){
        const path=encounter.aimPath;
        const index=path.findIndex(p=>p.tick>=tick);
        const b=index<0?path.at(-1):path[index],a=index>0?path[index-1]:b;
        const t=b.tick===a.tick?0:Math.max(0,Math.min(1,(tick-a.tick)/(b.tick-a.tick)));
        pitch=a.pitch+(b.pitch-a.pitch)*t;yaw=a.yaw+angleDifference(a.yaw,b.yaw)*t;
      }
      return {x,y,z,rX:pitch,rY:yaw,rZ:0,fov:encounter.fov};
    }
    let overviewView;
    if(overview){
      // Map-qualified B apartments dolly: independent of any player's view.
      const t=Math.min(1,Math.max(0,(tick-startTick)/96));
      const blend=t*t*(3-2*t);
      const front=players.find(p=>p.id===frontId)??main;
      const back=players.filter(p=>p.team===main.team&&p.id!==frontId&&Math.hypot(...p.eye.map((v,i)=>v-main.eye[i]))<360);
      const center=[0,1,2].map(i=>back.reduce((s,p)=>s+p.eye[i],0)/Math.max(1,back.length));
      const target=front.eye.map((v,i)=>v+(center[i]-v)*blend);
      target[2]-=28;
      const x=-1650+100*blend,y=850-50*blend,z=68;
      overviewView = {x,y,z,rX:Math.atan2(z-target[2],Math.hypot(target[0]-x,target[1]-y))*180/Math.PI,rY:Math.atan2(target[1]-y,target[0]-x)*180/Math.PI,rZ:0,fov:90};
      if(tick < startTick + 96) return overviewView;
    }
    const distance=p=>Math.hypot(...p.eye.map((v,i)=>v-main.eye[i]));
    const group=players.filter(p=>p.team===main.team&&distance(p)<360).sort((a,b)=>distance(a)-distance(b)).slice(0,3);
    yaw??=main.angles[1];const rad=yaw*Math.PI/180;
    const center=[0,1,2].map(i=>group.reduce((s,p)=>s+p.eye[i],0)/group.length);
    const rear=main;
    const x=rear.eye[0]-70*Math.cos(rad),y=rear.eye[1]-70*Math.sin(rad),z=rear.eye[2]+32;
    const target=[center[0]+70*Math.cos(rad),center[1]+70*Math.sin(rad),center[2]-20];
    const alpha=1-Math.exp(-Math.max(0,tick-(lastTick??tick))/10);
    targetView=targetView?target.map((v,i)=>targetView[i]+(v-targetView[i])*alpha):target;
    lastTick=tick;
    if(tick-lastLog>=16){lastLog=tick;mirv.message('TEAM_PUSH '+JSON.stringify({tick,players:group.map(p=>({id:p.id,eye:p.eye,distance:distance(p)}))})+'\n');}
    const follow={x,y,z,rX:Math.atan2(z-targetView[2],Math.hypot(targetView[0]-x,targetView[1]-y))*180/Math.PI,rY:Math.atan2(targetView[1]-y,targetView[0]-x)*180/Math.PI,rZ:0,fov:100};
    if(!overviewView)return follow;
    transition ??= createCameraTransition(overviewView, follow);
    return transition(overviewView, follow, (tick-startTick-96)/64);
  });
}
