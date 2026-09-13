// Rule adapter: shared render/packet/context data -> offline crosshair-lock inputs.
import assert from 'node:assert/strict';
import {readFile,writeFile,mkdir,open,link,unlink} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import path from 'node:path';
import {createReadStream} from 'node:fs';
import {createGzip} from 'node:zlib';
import {pipeline} from 'node:stream/promises';
import {Writable} from 'node:stream';
import {fileURLToPath} from 'node:url';
import {requireArtifact,artifact,fingerprint} from './analysis-contract.mjs';
import {readSceneJournal} from './analysis-journal.mjs';
import {parseCapture,captureText,alignCapture,alignCaptureWindows} from './analyze-attachment-capture.mjs';
export function buildInputs(capture,scene,context,angularResolutionDegrees) {
  assert(capture.identityProtocol,'Identity-capable capture protocol required');
  assert(Number.isFinite(angularResolutionDegrees) && angularResolutionDegrees>0,'Explicit angular resolution required');
  const packets=requireArtifact(scene,scene.contract?.module==='player-tracking-scene'?'player-tracking-scene':'packet-scene'),match=requireArtifact(context,'match-context');
  assert(scene.source.demoFingerprint && scene.source.demoFingerprint===context.source.demoFingerprint,'Mismatched demo context');
  const whole=alignCapture(capture,packets);
  const windows=whole.status==='matched'?[{firstTick:capture.firstTick,lastTick:capture.lastTick,...whole}]:alignCaptureWindows(capture,packets);
  const qualified=windows.filter(w=>w.status==='matched');
  assert(qualified.length>0,'No unique complete render/packet alignment');
  const alignment=whole.status==='matched'?whole:{status:'partial',packetTickShift:null,windows};
  assert(Math.abs(match.tickRate-capture.tickRate)<1e-6,'Tick rates disagree');
  const roster=new Set(match.players.map(player=>player.steamid));
  const lookup=new Map(packets.map(frame=>[frame.tick,new Map(frame.entities.map(e=>[e.entityId,e]))]));
  const inputs=new Map(),tracks=new Map();
  for(const frame of capture.frames) {
    const window=qualified.find(w=>frame.tick>=w.firstTick && frame.tick<=w.lastTick);
    if(!window) continue;
    const tick=frame.tick+window.packetTickShift;
    const rounds=match.rounds.filter(r=>r.round>=1 && r.freezeEndTick<=tick && tick<=r.endTick);
    if(rounds.length!==1) continue;
    const identities=frame.players.filter(p=>p.playerId).map(p=>p.playerId);
    assert(new Set(identities).size===identities.length,'Duplicate player identities in frame');
    const players=frame.players.filter(p=>p.playerId && roster.has(p.playerId));
    for(const subject of players) {
      if(!subject.eye || !subject.view) continue;
      if(!inputs.has(subject.playerId)) inputs.set(subject.playerId,{demoFingerprint:scene.source.demoFingerprint,playerId:subject.playerId,measurementSource:'Unqualified HLAE render attachments; measured packet alignment; explicit diagnostic angle-resolution budget. Not server hitboxes.',tickRate:capture.tickRate,sampleStepTicks:1,angularResolutionDegrees,tracks:[]});
      for(const target of players) {
        if(subject.playerId===target.playerId || subject.team===target.team) continue;
        const entity=lookup.get(tick)?.get(target.entityId);
        const modelIds=Object.entries(entity?.properties??{}).filter(([name])=>name.endsWith('.m_hModel'));
        if(modelIds.length!==1 || typeof modelIds[0][1]!=='string') continue;
        for(const [name,point] of Object.entries(target.attachments)) {
          if(!point.position) continue;
          const subjectEntity=lookup.get(tick)?.get(subject.entityId);
          if(!subjectEntity) continue;
          const key=JSON.stringify([subject.playerId,subject.entityId,subjectEntity.serial,subject.controllerHandle,subject.team,target.playerId,target.controllerHandle,target.team,entity.serial,modelIds[0][1],name,rounds[0].round]);
          if(!tracks.has(key)) {
            const track={round:rounds[0].round,targetId:target.playerId,pointId:key,enemy:true,samples:[]};
            tracks.set(key,track);inputs.get(subject.playerId).tracks.push(track);
          }
          tracks.get(key).samples.push({tick,eye:subject.eye,view:subject.view.slice(0,2),target:point.position,obstruction:'unknown'});
        }
      }
    }
  }
  return {alignment,inputs:[...inputs.values()]};
}
// Bounded batches for consumers that maintain rule state across windows. Never persist
// the expanded subject × target × point samples or sum independently scored windows.
export function buildInputWindows(capture,journalFile,context,resolution) {
  return buildCaptureWindows([capture],journalFile,context,resolution);
}
export async function* buildCaptureWindows(captures,journalFile,context,resolution) {
  const iterator=readSceneJournal(journalFile)[Symbol.asyncIterator]();
  try {
    const first=await iterator.next();assert(first.value?.header,'Missing scene journal header');
    const header=first.value.header;
    assert(header.source.demoFingerprint && header.source.demoFingerprint===context.source.demoFingerprint,'Mismatched demo context');
    let next=await iterator.next(),packets=[],lastCaptureTick=-1;
    for await(const capture of captures) {
      assert(Number.isInteger(capture.firstTick) && Number.isInteger(capture.lastTick) && capture.firstTick>lastCaptureTick && capture.lastTick>=capture.firstTick,'Capture ranges overlap or are out of order');
      assert(capture.tickRate===context.data.tickRate,'Capture tick rates disagree');
      lastCaptureTick=capture.lastTick;
    for(let start=capture.firstTick;start<=capture.lastTick;start+=128) {
      const end=Math.min(start+127,capture.lastTick);
      packets=packets.filter(frame=>frame.tick>=start-4);
      while(!next.done && next.value.frame.tick<=end+4){if(next.value.frame.tick>=start-4)packets.push(next.value.frame);next=await iterator.next();}
      const frames=capture.frames.filter(f=>f.tick>=start&&f.tick<=end);
      const part={...capture,firstTick:start,lastTick:end,frames,missingTicks:capture.missingTicks.filter(t=>t>=start&&t<=end)};
      const alignment=alignCapture(part,packets);
      if(alignment.status!=='matched'){yield {firstTick:start,lastTick:end,alignment,inputs:[]};continue;}
      const scene=artifact(header.data.profile==='player-tracking'?'player-tracking-scene':'packet-scene',packets,{source:header.source});
      yield {firstTick:start,lastTick:end,...buildInputs(part,scene,context,resolution)};
    }
    }
    while(!next.done)next=await iterator.next(); // Validate the end marker before publishing a result.
  } finally {await iterator.return?.();}
}

// Shared state journal: cameras and world points occur once, regardless of observer count.
export async function writeBodyJournal(batches, output, source, dependencies, clock) {
  const temporary=output+'.pending-'+randomUUID();
  const file=await open(temporary,'wx');
  const compressor=output.endsWith('.gz')?createGzip():null;
  const completion=compressor?pipeline(compressor,new Writable({write(chunk,_,done){file.writeFile(chunk).then(()=>done(),done);}})):null;
  completion?.catch(()=>{}); // Observe errors immediately; await them before publication below.
  const write=value=>{
    const text=JSON.stringify(value)+'\n';
    return compressor?new Promise((resolve,reject)=>compressor.write(text,error=>error?reject(error):resolve())):file.writeFile(text);
  };
  const definitions=new Map(),pointKeys=new Map(),views=new Map(),points=new Map(),alignment=[];let active='',lastTick=null,count=0;
  try {
    const header=artifact('body-measurement-journal',clock,{source,dependencies});header.contract.schemaVersion=2;
    await write(header);
    for await(const batch of batches) {
      if(batch.alignment)alignment.push({firstTick:batch.firstTick,lastTick:batch.lastTick,...batch.alignment});
      const frames=new Map();
      for(const input of batch.inputs) for(const track of input.tracks) {
        // Identity layout is owned by buildInputs above; drop only the observer portion.
        const identity=JSON.parse(track.pointId);assert.equal(identity.length,12);
        const worldIdentity=JSON.stringify(identity.slice(5));
        if(!pointKeys.has(worldIdentity))pointKeys.set(worldIdentity,String(pointKeys.size));
        const pointKey=pointKeys.get(worldIdentity);
        const id=JSON.stringify([input.playerId,track.round,track.pointId]);
        for(const sample of track.samples) {
          if(!frames.has(sample.tick)) frames.set(sample.tick,{tick:sample.tick,define:{},views:{},points:{},active:[]});
          const frame=frames.get(sample.tick);
          if(!definitions.has(id)) {
            definitions.set(id,String(definitions.size));
            frame.define[definitions.get(id)]={playerId:input.playerId,round:track.round,targetId:track.targetId,pointId:track.pointId,pointKey,enemy:track.enemy};
          }
          frame.active.push(definitions.get(id));
          frame.views[input.playerId]={eye:sample.eye,view:sample.view};frame.points[pointKey]=sample.target;
        }
      }
      for(const frame of [...frames.values()].sort((a,b)=>a.tick-b.tick)) {
        assert(lastTick===null||frame.tick>lastTick,'Overlapping aligned windows');
        for(const [field,previous] of [['views',views],['points',points]]) {
          for(const [key,value] of Object.entries(frame[field])) {
            const encoded=JSON.stringify(value);if(previous.get(key)===encoded)delete frame[field][key];else previous.set(key,encoded);
          }
          if(!Object.keys(frame[field]).length)delete frame[field];
        }
        if(!Object.keys(frame.define).length)delete frame.define;
        frame.active.sort();const encoded=JSON.stringify(frame.active);
        if(encoded===active)delete frame.active;else active=encoded;
        await write(frame);lastTick=frame.tick;count++;
      }
    }
    await write({end:count,lastTick,alignment});
    if(compressor){compressor.end();await completion;}
    await file.sync();await file.close();
    await link(temporary,output); // Publish only a complete journal; never replace prior evidence.
  } finally {if(compressor){compressor.destroy();await completion.catch(()=>{});}await file.close();await unlink(temporary);}
  return {frames:count,tracks:definitions.size};
}
async function fileFingerprint(file) {
  const hash=createHash('sha256');for await(const chunk of createReadStream(file))hash.update(chunk);return 'sha256:'+hash.digest('hex');
}
async function captureFiles(manifestPath) {
  const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
  assert(manifest.schemaVersion===1 && Array.isArray(manifest.captures) && manifest.captures.length>0,'Expected capture manifest v1');
  return manifest.captures.map(entry=>{
    assert(typeof entry.path==='string' && /^sha256:[a-f0-9]{64}$/.test(entry.fingerprint),'Capture paths and SHA-256 fingerprints required');
    return {path:path.resolve(path.dirname(manifestPath),entry.path),expected:entry.fingerprint};
  });
}
async function main() {
  const started=performance.now();
  const args=process.argv.slice(2),multiple=args[0]==='--captures';
  const [logFile,sceneFile,contextFile,storeDir,resolution]=multiple?args.slice(1):args;
  assert(logFile && sceneFile && contextFile && storeDir && resolution,'Usage: [--captures] LOG_OR_MANIFEST SCENE.ndjson CONTEXT DATA_DIRECTORY ANGULAR_RESOLUTION');
  assert(!multiple || sceneFile.endsWith('.ndjson'),'Capture manifests require a scene journal');
  if(sceneFile.endsWith('.ndjson')) {
    const contextBytes=await readFile(contextFile),context=JSON.parse(contextBytes);
    requireArtifact(context,'match-context');
    const logFiles=multiple?await captureFiles(logFile):[{path:logFile}];
    const dependencies=[await fileFingerprint(sceneFile),fingerprint(contextBytes)];
    if(multiple)dependencies.push(await fileFingerprint(logFile));
    for(const file of logFiles) {
      file.fingerprint=await fileFingerprint(file.path);
      if(file.expected)assert.equal(file.fingerprint,file.expected,'Capture content changed');
      dependencies.push(file.fingerprint);
    }
    async function* captures() {
      for(const file of logFiles) {
        const handle=await open(file.path,'r');
        let log;
        try { assert((await handle.stat()).size<=128*1024*1024,'Capture exceeds the bounded 128 MiB segment limit');log=await handle.readFile(); }
        finally { await handle.close(); }
        assert.equal(fingerprint(log),file.fingerprint,'Capture changed during analysis');
        yield parseCapture(captureText(log));
      }
    }
    async function* batches() {
      for await(const batch of buildCaptureWindows(captures(),sceneFile,context,Number(resolution))) {
        yield batch;
      }
      assert.equal(await fileFingerprint(sceneFile),dependencies[0],'Scene changed during analysis');
    }
    const directory=path.join(storeDir,'analysis','body-measurements');await mkdir(directory,{recursive:true});
    const key=createHash('sha1').update(context.source.demoFingerprint).digest('hex');
    const result=await writeBodyJournal(batches(),path.join(directory,key+'.ndjson.gz'),context.source,dependencies,
      {tickRate:context.data.tickRate,sampleStepTicks:1,angularResolutionDegrees:Number(resolution),measurementSource:'Unqualified HLAE render attachments; measured packet alignment; explicit diagnostic angle-resolution budget. Not server hitboxes.'});
    console.log(JSON.stringify({...result,elapsedSeconds:(performance.now()-started)/1000,peakRssBytes:process.resourceUsage().maxRSS*1024}));return;
  }


  assert(logFile && sceneFile && contextFile && storeDir && resolution,'Usage: LOG SCENE CONTEXT DATA_DIRECTORY ANGULAR_RESOLUTION');
  const log=await readFile(logFile),sceneBytes=await readFile(sceneFile),contextBytes=await readFile(contextFile);
  const scene=JSON.parse(sceneBytes),context=JSON.parse(contextBytes);
  const result=buildInputs(parseCapture(captureText(log)),scene,context,Number(resolution));
  const directory=path.join(storeDir,'analysis','scoring-inputs');await mkdir(directory,{recursive:true});
  const alignmentBytes=Buffer.from(JSON.stringify(artifact('attachment-alignment',result.alignment,{source:scene.source,dependencies:[fingerprint(log),fingerprint(sceneBytes)]})));
  const alignmentHash=fingerprint(alignmentBytes);
  const alignmentDir=path.join(storeDir,'analysis','attachment-alignments');await mkdir(alignmentDir,{recursive:true});
  const alignmentPath=path.join(alignmentDir,alignmentHash.replace(':','-')+'.json');
  await writeFile(alignmentPath,alignmentBytes,{flag:'wx'});

  for(const input of result.inputs) {
    const key=createHash('sha1').update(input.demoFingerprint+'\0'+input.playerId).digest('hex');
    const file=artifact('scoring-measurements',{'crosshair-lock':input},{source:scene.source,dependencies:[fingerprint(log),fingerprint(sceneBytes),fingerprint(contextBytes),alignmentHash]});
    await writeFile(path.join(directory,key+'.json'),JSON.stringify(file),{flag:'wx'});
  }
  console.log(JSON.stringify({players:result.inputs.length,tracks:result.inputs.reduce((n,p)=>n+p.tracks.length,0),packetTickShift:result.alignment.packetTickShift}));
}
if(process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) await main();
