// Wire measured render attachments to model-local hitboxes. No scoring dependency.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {artifact,requireArtifact,fingerprint} from './analysis-contract.mjs';
import {parseCapture,alignCapture} from './analyze-attachment-capture.mjs';
import {attachmentBoneTransform,ATTACHMENT_GEOMETRY_VERSION} from './analysis-attachment-geometry.mjs';
import {reconstructHitboxes,HITBOX_VERSION} from './analysis-hitboxes.mjs';
const [logFile,packetFile,modelFile,output]=process.argv.slice(2);
assert(logFile && packetFile && modelFile && output,'Usage: node scripts/reconstruct-analysis-hitboxes.mjs CONSOLE.log SCENE.json PLAYER_MODEL.json OUTPUT.json');
const logBytes=await readFile(logFile),packetBytes=await readFile(packetFile),modelBytes=await readFile(modelFile);
const scene=JSON.parse(packetBytes),modelArtifact=JSON.parse(modelBytes);
const packets=requireArtifact(scene,'packet-scene'),model=requireArtifact(modelArtifact,'player-model');
const capture=parseCapture(logBytes.toString('utf8')),alignment=alignCapture(capture,packets);
const frames=new Map(packets.map(frame=>[frame.tick,new Map(frame.entities.map(entity=>[entity.entityId,entity]))]));
const samples=[],unavailable=[];
if(alignment.status==='matched') for(const frame of capture.frames) for(const player of frame.players) {
  const packetTick=frame.tick+alignment.packetTickShift,entity=frames.get(packetTick)?.get(player.entityId);
  const identity={renderTick:frame.tick,packetTick,entityId:player.entityId,serial:entity?.serial};
  const models=Object.entries(entity?.properties??{}).filter(([key])=>key.endsWith('.m_hModel'));
  if(models.length!==1 || models[0][1]!==model.resourceId) {unavailable.push({...identity,reason:'No matching model definition'});continue;}
  const prefix=models[0][0].slice(0,-'m_hModel'.length);
  const scale=entity.properties[prefix+'m_flScale'],setIndex=entity.properties[prefix+'m_nHitboxSet'];
  if(!Number.isInteger(setIndex) || setIndex<0 || !Number.isFinite(scale) || scale<=0) {unavailable.push({...identity,reason:'Missing model scale or hitbox set'});continue;}
  const provider=bone=>{
    const candidates=Object.entries(model.definitions).filter(([name,d])=>d.bone===bone && player.attachments[name]?.position && player.attachments[name]?.rotation).sort(([a],[b])=>a.localeCompare(b));
    if(!candidates.length) return null;
    const [name,definition]=candidates[0];
    return attachmentBoneTransform(player.attachments[name],definition,scale);
  };
  // The demo has no verified model-content fingerprint in this pipeline; never substitute a path hash.
  samples.push({...identity,...reconstructHitboxes(model,{resourceId:models[0][1],setIndex,scale,recordedContentFingerprint:null},provider)});
}
const summary={alignment:alignment.status,packetTickShift:alignment.packetTickShift,samples:samples.length,capsules:samples.reduce((n,s)=>n+s.capsules.length,0),unavailableHitboxes:samples.reduce((n,s)=>n+s.unavailable.length,0),unavailableSamples:unavailable.length,eligibleForScoring:false};
const result=artifact('rendered-hitboxes',{summary,alignment,samples,unavailable},{source:scene.source,implementationVersion:HITBOX_VERSION,dependencies:[fingerprint(logBytes),fingerprint(packetBytes),fingerprint(modelBytes),fingerprint(ATTACHMENT_GEOMETRY_VERSION)]});
await writeFile(output,JSON.stringify(result,null,2),{flag:'wx'});
console.log(JSON.stringify(summary));
