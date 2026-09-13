// Composition root for independent model/packet/render evidence validation.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {artifact,requireArtifact,fingerprint} from './analysis-contract.mjs';
import {parseCapture,alignCapture} from './analyze-attachment-capture.mjs';
import {compareAttachments,ATTACHMENT_GEOMETRY_VERSION} from './analysis-attachment-geometry.mjs';
const [logFile,packetFile,modelFile,output]=process.argv.slice(2);
assert(logFile && packetFile && modelFile && output,'Usage: node scripts/validate-attachment-geometry.mjs CONSOLE.log SCENE.json ATTACHMENT_MODEL.json OUTPUT.json');
const logBytes=await readFile(logFile),packetBytes=await readFile(packetFile),modelBytes=await readFile(modelFile);
const scene=JSON.parse(packetBytes),modelArtifact=JSON.parse(modelBytes);
const packets=requireArtifact(scene,'packet-scene'),model=requireArtifact(modelArtifact,'attachment-definitions');
const capture=parseCapture(logBytes.toString('utf8')),alignment=alignCapture(capture,packets);
const frames=new Map(packets.map(frame=>[frame.tick,new Map(frame.entities.map(entity=>[entity.entityId,entity]))]));
const comparisons=[],unavailable=[];
if(alignment.status==='matched') for(const frame of capture.frames) for(const player of frame.players) {
  const tick=frame.tick+alignment.packetTickShift,entity=frames.get(tick)?.get(player.entityId);
  const identity={renderTick:frame.tick,packetTick:tick,entityId:player.entityId,serial:entity?.serial};
  const models=Object.entries(entity?.properties??{}).filter(([key])=>key.endsWith('.m_hModel'));
  if(models.length!==1 || models[0][1]!==model.resourceId) {unavailable.push({...identity,reason:'No matching model definition'});continue;}
  const prefix=models[0][0].slice(0,-'m_hModel'.length),scale=entity.properties[prefix+'m_flScale'];
  if(!Number.isFinite(scale) || scale<=0) {unavailable.push({...identity,reason:'Missing valid model scale'});continue;}
  const result=compareAttachments(player.attachments,model,models[0][1],{scale});
  comparisons.push({...identity,...result});
}
const pairs=comparisons.flatMap(result=>result.comparisons);
const status=alignment.status!=='matched' || pairs.length<32 || comparisons.some(c=>c.status==='insufficientData') ? 'insufficientData' : comparisons.every(c=>c.status==='matched') ? 'matched' : 'mismatch';
const summary={status,frames:capture.frames.length,packetTickShift:alignment.packetTickShift,evaluatedSamples:comparisons.length,pairs:pairs.length,unavailableSamples:unavailable.length,maxPositionError:pairs.length?pairs.reduce((max,p)=>Math.max(max,p.positionError),0):null,maxAngleError:pairs.length?pairs.reduce((max,p)=>Math.max(max,p.angleError),0):null};
const result=artifact('attachment-geometry',{summary,alignment,comparisons,unavailable,scope:'Selected model relative attachment geometry; network bone timing and historical asset compatibility remain unverified'}, {source:scene.source,implementationVersion:ATTACHMENT_GEOMETRY_VERSION,dependencies:[fingerprint(logBytes),fingerprint(packetBytes),fingerprint(modelBytes)]});
await writeFile(output,JSON.stringify(result,null,2),{flag:'wx'});
console.log(JSON.stringify(summary));
