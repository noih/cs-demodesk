import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {captureText,parseCapture} from './analyze-attachment-capture.mjs';

const multiply=([x,y,z,w],[a,b,c,d])=>[
  w*a+x*d+y*c-z*b,w*b-x*c+y*d+z*a,w*c+x*b-y*a+z*d,w*d-x*a-y*b-z*c,
];
const rotate=(q,v)=>multiply(multiply(q,[...v,0]),[-q[0],-q[1],-q[2],q[3]]).slice(0,3);

export function compare(capture,native,modelArtifact,shift,maximumError) {
  assert(Number.isInteger(shift)&&Math.abs(shift)<=16,'Explicit bounded clock shift required');
  assert(Number.isFinite(maximumError)&&maximumError>0,'Expected position tolerance');
  const {resourceId,definitions}=modelArtifact.data??modelArtifact;
  assert(resourceId&&definitions,'Expected model resource ID and attachment definitions');
  const frames=new Map(native.frames.map(frame=>[frame.tick,new Map(frame.players.map(player=>[player.entity,player]))]));
  let pairs=0,unavailable=0,otherModels=0,maximum=0;
  const byName={};
  for(const frame of capture.frames)for(const player of frame.players) {
    const measured=frames.get(frame.tick+shift)?.get(player.entityId);
    if(!measured?.hitboxSet) {unavailable++;continue;}
    const [model,set]=measured.hitboxSet;
    if(String(model)!==String(resourceId)) {otherModels++;continue;}
    const hitboxes=native.models[String(model)]?.[set]?.hitboxes;
    if(!hitboxes||hitboxes.length!==measured.transforms.length) {unavailable++;continue;}
    const bones=new Map(hitboxes.map((hitbox,index)=>[hitbox.bone.toLowerCase(),measured.transforms[index]]));
    for(const [name,attachment] of Object.entries(player.attachments)) {
      const definition=definitions[name],transform=definition&&bones.get(definition.bone.toLowerCase());
      if(!transform?.position||!attachment.position)continue;
      const local=definition.offset.map(value=>value*transform.scale);
      const point=rotate(transform.rotation,local).map((value,index)=>value+transform.position[index]);
      const error=Math.hypot(...point.map((value,index)=>value-attachment.position[index]));
      assert(Number.isFinite(error),'Non-finite attachment comparison');
      pairs++;maximum=Math.max(maximum,error);
      const entry=byName[name]??={pairs:0,maximumError:0};
      entry.pairs++;entry.maximumError=Math.max(entry.maximumError,error);
    }
  }
  return {status:pairs<32||unavailable?'insufficientData':maximum>maximumError?'mismatch':'matched',
    shift,resourceId:String(resourceId),pairs,unavailable,otherModels,maximumError:maximum,tolerance:maximumError,byName};
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const [logFile,nativeFile,modelFile,shiftText,toleranceText,output]=process.argv.slice(2);
  assert(output,'Usage: node scripts/compare-native-attachments.mjs CONSOLE.log NATIVE.json MODEL.json SHIFT MAX_ERROR OUTPUT.json');
  const [logBytes,nativeBytes,modelBytes]=await Promise.all([readFile(logFile),readFile(nativeFile),readFile(modelFile)]);
  const capture=parseCapture(captureText(logBytes));
  const native=JSON.parse(nativeBytes),model=JSON.parse(modelBytes);
  const result=compare(capture,native,model,Number(shiftText),Number(toleranceText));
  const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
  const report={...result,inputs:{capture:hash(logBytes),native:hash(nativeBytes),model:hash(modelBytes)},
    source:native.source??null,clientSha256:native.clientSha256??null};
  await writeFile(output,JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({status:result.status,pairs:result.pairs,unavailable:result.unavailable,maximumError:result.maximumError}));
  if(result.status!=='matched')process.exitCode=1;
}
