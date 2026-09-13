// HLAE mirv-script module for an isolated offline replay. Render samples are not network bone truth.
// Requires HLAE >= 2.187.2 (Entity.getAttachment). Never starts playback or changes the view.
// Frame stage 12 verified with CS2 14181 / HLAE 2.191.1; validate again after engine updates.
// Pack only values already exactly representable as f32; never round measurements.
function packed(values) {
  if(!Array.isArray(values) || !values.every(v=>Number.isFinite(v)&&Math.fround(v)===v))return values;
  const bytes=new Uint8Array(values.length*4),view=new DataView(bytes.buffer);
  values.forEach((v,i)=>view.setFloat32(i*4,v,true));
  const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';let result='';
  for(let i=0;i<bytes.length;i+=3){const n=(bytes[i]<<16)|((bytes[i+1]??0)<<8)|(bytes[i+2]??0);result+=alphabet[(n>>>18)&63]+alphabet[(n>>>12)&63]+(i+1<bytes.length?alphabet[(n>>>6)&63]:'=')+(i+2<bytes.length?alphabet[n&63]:'=');}
  return result;
}
export function startAttachmentCapture({firstTick,lastTick,attachments,renderStage=12,includeIdentity=false,stateChanges=true}) {
  if (!Number.isInteger(renderStage) || renderStage<0) throw Error('Expected a verified render frame stage');
  if (!Number.isInteger(firstTick) || firstTick<0 || !Number.isInteger(lastTick) || lastTick<firstTick || lastTick-firstTick>=4096) throw Error('Expected a bounded interval of at most 4096 ticks');
  if (!Array.isArray(attachments) || !attachments.length || attachments.length>32 || new Set(attachments).size!==attachments.length || !attachments.every(name=>/^[a-zA-Z0-9_]{1,32}$/.test(name))) throw Error('Expected unique, explicit model attachment names');
  if (!globalThis.mirv) throw Error('Load this module inside HLAE');
  const previous=mirv.onClientFrameStageNotify;
  let last=-1,frames=0,stopped=false;
  const states=new Map();let seen=new Set();
  const write = values => {
    const wire=stateChanges && !['ready','done','frame','name'].includes(values[0])?[values[0],...values.slice(2)]:values;
    if(stateChanges && ['point','rotation'].includes(wire[0])){wire[2]=attachments.indexOf(values[3]);wire[3]=packed(wire[3]);}
    else if(stateChanges && ['origin','eye','view'].includes(wire[0]))wire[2]=packed(wire[2]);
    const text='DEMODESK_POSE '+JSON.stringify(wire);
    if (text.length>220) throw Error('Record exceeds safe netcon line length');
    mirv.message(text+'\n');
  };
  const emit = values => {
    if(!stateChanges){write(values);return;}
    const [kind,tick,id]=values;
    if(kind==='frame'){seen=new Set();write(values);return;}
    if(kind==='end'){
      for(const previous of states.keys())if(!seen.has(previous)){write(['remove',tick,previous]);states.delete(previous);}
      write(values);return;
    }
    if(['player','identity','origin','eye','view','point','rotation'].includes(kind)) {
      if(kind==='player') {
        seen.add(id);
        if(!states.has(id) || states.get(id).handle!==values[3]){states.set(id,{handle:values[3],rows:new Map()});write(['reset',tick,id]);}
      }
      const state=states.get(id);if(!state)throw Error('Field without player state');
      const key=kind+(['point','rotation'].includes(kind)?':'+values[3]:'');
      const payload=JSON.stringify(values.slice(2));
      if(state.rows.get(key)===payload)return;
      state.rows.set(key,payload);
    }
    write(values);
  };

  const finite = values => values.every(Number.isFinite) ? values : null;
  const stop = () => { if (!stopped) { stopped=true;mirv.onClientFrameStageNotify=previous; } };
  mirv.onClientFrameStageNotify=function(event) {
    const previousResult=previous ? previous(event) : undefined;
    if (event.isBefore || event.curStage!==renderStage || !mirv.isPlayingDemo()) return previousResult;
    const tick=mirv.getDemoTick();
    if (!Number.isInteger(tick) || tick<firstTick || tick===last) return previousResult;
    if (tick>lastTick) { stop();emit(['done',frames]);return previousResult; }
    try {
      if (last>=0 && tick<last) throw Error('Playback moved backwards during capture');
      last=tick;frames++;
      emit(['frame',tick,mirv.getDemoTime(),mirv.getCurTime()]);
      let players=0;
      for (let index=0;index<=mirv.getHighestEntityIndex();index++) {
        const player=mirv.getEntityFromIndex(index);
        if (!player || !player.isValid() || !player.isPlayerPawn() || player.getHealth()<=0) continue;
        players++;
        emit(['player',tick,index,player.getPlayerControllerHandle(),player.getHealth(),player.getTeam()]);
        if(includeIdentity) {
          const handle=player.getPlayerControllerHandle();
          const controller=mirv.isHandleValid(handle) ? mirv.getEntityFromIndex(mirv.getHandleEntryIndex(handle)) : null;
          const pawnHandle=controller?.isPlayerController() ? controller.getPlayerPawnHandle() : null;
          const id=controller?.isValid() && pawnHandle!==null && mirv.isHandleValid(pawnHandle) && mirv.getHandleEntryIndex(pawnHandle)===index ? controller.getSteamId() : 0n;
          emit(['identity',tick,index,id>0n?id.toString():null]);
        }
        emit(['origin',tick,index,finite(player.getOrigin())]);
        emit(['eye',tick,index,finite(player.getRenderEyeOrigin())]);
        emit(['view',tick,index,finite(player.getRenderEyeAngles())]);
        for (const name of attachments) {
          const attachment=player.getAttachment(name);
          emit(['point',tick,index,name,attachment ? finite([attachment.position.x,attachment.position.y,attachment.position.z]) : null]);
          emit(['rotation',tick,index,name,attachment ? finite([attachment.angles.x,attachment.angles.y,attachment.angles.z,attachment.angles.w]) : null]);
        }
      }
      emit(['end',tick,players,attachments.length]);
    } catch (error) { stop();mirv.message('DEMODESK_POSE_ERROR '+String(error)+'\n'); }
    return previousResult;
  };
  emit(['ready',firstTick,lastTick,attachments.length,'render-pass-after',renderStage,...(stateChanges?[4,includeIdentity]:includeIdentity?[2]:[])]);
  if(stateChanges)attachments.forEach((name,index)=>emit(['name',index,name]));
  return stop;
}
