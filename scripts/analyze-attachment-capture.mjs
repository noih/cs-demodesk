import {requireArtifact, artifact, fingerprint} from './analysis-contract.mjs';
// Strict HLAE capture validation and measured render-to-packet alignment. No credit score.
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const finiteVector=(value,size)=>value===null || (Array.isArray(value) && value.length===size && value.every(Number.isFinite));
const integer=value=>Number.isSafeInteger(value) && value>=0;

export function captureText(bytes) {
  assert(bytes.length<=128*1024*1024,'Capture segment exceeds 128 MiB');
  const decoded=bytes[0]===0x1f&&bytes[1]===0x8b?gunzipSync(bytes,{maxOutputLength:128*1024*1024}):bytes;
  return decoded.toString('utf8');
}

export function parseCapture(log) {
  assert(!log.includes('DEMODESK_POSE_ERROR'),'Capture reported an error');
  // Decode delta records as consumed; retaining the whole parsed log duplicates frame memory.
  function* records() {
    for(const match of log.matchAll(/[^\r\n]+/g)) {
      const offset=match[0].indexOf('DEMODESK_POSE ');
      // Engine diagnostics sometimes omit their newline before our complete record.
      if(offset>=0)yield JSON.parse(match[0].slice(offset+14));
    }
  }
  const iterator=records(),ready=iterator.next().value;
  if([3,4].includes(ready?.[6]))return parseDeltaRecords(ready,iterator);
  return parseRecords([ready,...iterator]);
}
function parseRecords(records) {
  let offset=0;
  const take=kind=>{
    const row=records[offset++];
    assert(Array.isArray(row) && row[0]===kind,`Missing or unexpected ${kind} record`);
    return row;
  };
  const ready=take('ready');
  assert((ready.length===6 || (ready.length===7 && ready[6]===2)) && ready[4]==='render-pass-after' && integer(ready[5]),'Unsupported capture protocol/phase');
  const [,firstTick,lastTick,pointCount,,renderStage]=ready;
  assert(integer(firstTick) && integer(lastTick) && lastTick>=firstTick && lastTick-firstTick<4096 && integer(pointCount) && pointCount>0 && pointCount<=32,'Invalid capture range/count');
  const frames=[];
  let pointNames=null;
  while(records[offset]?.[0]==='frame') {
    const row=take('frame');
    const [,tick,demoTime,curTime]=row;
    const previous=frames.at(-1);
    assert(row.length===4 && integer(tick) && tick>=firstTick && tick<=lastTick && Number.isFinite(demoTime) && demoTime>=0 && Number.isFinite(curTime),'Invalid frame timestamp');
    assert(!previous || (tick>previous.tick && demoTime>previous.demoTime && curTime>previous.curTime),'Non-monotonic capture');
    const players=[],ids=new Set();
    while(records[offset]?.[0]==='player') {
      const metadata=take('player');
      const [,playerTick,entityId,controllerHandle,health,team]=metadata;
      assert(metadata.length===6 && playerTick===tick && integer(entityId) && integer(controllerHandle) && Number.isFinite(health) && health>0 && [2,3].includes(team) && !ids.has(entityId),'Invalid/duplicate player');
      ids.add(entityId);
      const player={entityId,controllerHandle,health,team,attachments:{}};
      if(ready.length===7) {
        const identity=take('identity');
        assert(identity.length===4 && identity[1]===tick && identity[2]===entityId && (identity[3]===null || (typeof identity[3]==='string' && /^[1-9][0-9]*$/.test(identity[3]) && BigInt(identity[3])<(1n<<64n))),'Invalid player identity');
        player.playerId=identity[3];
      }
      for (const kind of ['origin','eye','view']) {
        const value=take(kind);
        assert(value.length===4 && value[1]===tick && value[2]===entityId && finiteVector(value[3],3),`Invalid ${kind}`);
        player[kind]=value[3];
      }
      const names=[];
      for(let i=0;i<pointCount;i++) {
        const point=take('point'),rotation=take('rotation');
        const name=point[3];
        assert(point.length===5 && point[1]===tick && point[2]===entityId && typeof name==='string' && /^[a-zA-Z0-9_]{1,32}$/.test(name) && !names.includes(name) && finiteVector(point[4],3),'Invalid/duplicate attachment');
        assert(rotation.length===5 && rotation[1]===tick && rotation[2]===entityId && rotation[3]===name && finiteVector(rotation[4],4),'Invalid attachment rotation');
        assert((point[4]===null)===(rotation[4]===null),'Incomplete attachment transform');
        if(rotation[4]) assert(Math.abs(Math.hypot(...rotation[4])-1)<0.001,'Invalid attachment quaternion');
        names.push(name);
        // defineProperty also handles names such as __proto__ without changing object prototypes.
        Object.defineProperty(player.attachments,name,{value:{position:point[4],rotation:rotation[4]},enumerable:true});
      }
      if(pointNames) assert.deepEqual(names,pointNames,'Attachment identities changed during capture');
      else pointNames=names;
      players.push(player);
    }
    const end=take('end');
    assert(end.length===4 && end[1]===tick && end[2]===players.length && end[3]===pointCount,'Incomplete frame records');
    frames.push({tick,demoTime,curTime,players});
  }
  const done=take('done');
  assert(done.length===2 && done[1]===frames.length && offset===records.length,'Incomplete or concatenated captures');
  const tickRate=frames.length<2 ? null : (frames[1].tick-frames[0].tick)/(frames[1].demoTime-frames[0].demoTime);
  if(tickRate!==null) {
    assert(Number.isFinite(tickRate) && tickRate>0,'Invalid measured tick rate');
    for(let i=1;i<frames.length;i++) {
      const expected=(frames[i].tick-frames[i-1].tick)/tickRate;
      assert(Math.abs(frames[i].demoTime-frames[i-1].demoTime-expected)<1e-6 && Math.abs(frames[i].curTime-frames[i-1].curTime-expected)<1e-6,'Capture clock discontinuity');
    }
  }
  const ticks=new Set(frames.map(frame=>frame.tick));
  return {schemaVersion:1,identityProtocol:ready.length===7,phase:'render-pass-after',renderStage,firstTick,lastTick,tickRate,pointNames:pointNames??[],missingTicks:Array.from({length:lastTick-firstTick+1},(_,i)=>firstTick+i).filter(tick=>!ticks.has(tick)),frames};
}

function parseDeltaRecords(ready,records) {
  assert(ready.length===8 && typeof ready[7]==='boolean','Invalid delta protocol');
  const pointCount=ready[3];
  const states=new Map(),frames=[],names=[];let frame=null,ended=false,pointNames=null;
  const legacyReady=[...ready.slice(0,6),...(ready[7]?[2]:[])];
  for(const wire of records) {
    assert(!ended && Array.isArray(wire),'Data after delta capture end');
    if(wire[0]==='name'){assert(frame===null && frames.length===0 && wire.length===3 && wire[1]===names.length && names.length<pointCount && /^[a-zA-Z0-9_]{1,32}$/.test(wire[2]) && !names.includes(wire[2]),'Invalid attachment dictionary');names.push(wire[2]);continue;}
    assert(names.length===pointCount,'Incomplete attachment dictionary');
    const row=['ready','done','frame'].includes(wire[0])?wire:[wire[0],frame?.[1],...wire.slice(1)];
    if(['point','rotation'].includes(row[0])){assert(integer(row[3])&&row[3]<names.length,'Invalid attachment reference');row[3]=names[row[3]];}
    if(ready[6]===4 && ['origin','eye','view','point','rotation'].includes(row[0])) {
      const index=['point','rotation'].includes(row[0])?4:3,size=row[0]==='rotation'?4:3;
      if(typeof row[index]==='string'){
        const bytes=Buffer.from(row[index],'base64');assert(bytes.length===size*4 && bytes.toString('base64')===row[index],'Invalid packed vector');
        row[index]=Array.from({length:size},(_,i)=>bytes.readFloatLE(i*4));
      }
    }

    assert(!ended && Array.isArray(row),'Data after delta capture end');
    const [kind,tick,id]=row;
    if(kind==='frame'){assert(frame===null,'Unfinished delta frame');frame=row;continue;}
    if(kind==='done'){assert(frame===null && row.length===2 && tick===frames.length,'Incomplete delta capture');ended=true;continue;}
    assert(frame && tick===frame[1],'Delta outside its frame');
    if(kind==='reset'){assert(row.length===3 && integer(id),'Invalid delta reset');states.set(id,new Map());continue;}
    if(kind==='remove'){assert(row.length===3 && states.delete(id),'Unknown delta removal');continue;}
    if(kind==='end') {
      assert(row.length===4 && id===states.size && row[3]===pointCount,'Incomplete delta frame');
      const full=[ [...legacyReady.slice(0,1),tick,tick,...legacyReady.slice(3)],frame ];
      for(const [entity,values] of [...states].sort((a,b)=>a[0]-b[0])) {
        for(const type of ['player',...(ready[7]?['identity']:[]),'origin','eye','view']){const value=values.get(type);assert(value,'Missing delta player field');full.push([type,tick,...value]);}
        for(const [key,value] of values)if(key.startsWith('point:')){full.push(['point',tick,...value]);const rotation=values.get('rotation:'+value[1]);assert(rotation,'Missing delta rotation');full.push(['rotation',tick,...rotation]);}
      }
      full.push(row,['done',1]);const decoded=parseRecords(full);
      if(decoded.pointNames.length){if(pointNames)assert.deepEqual(decoded.pointNames,pointNames,'Delta attachment identities changed');else pointNames=decoded.pointNames;}
      frames.push(decoded.frames[0]);frame=null;continue;
    }
    assert(['player','identity','origin','eye','view','point','rotation'].includes(kind) && states.has(id),'Invalid delta field');
    const key=kind+(['point','rotation'].includes(kind)?':'+row[3]:'');
    states.get(id).set(key,row.slice(2));
  }
  assert(ended && frame===null,'Missing delta capture end');
  // Reuse the clock/coverage validation without expanding every player record again.
  const clocks=[legacyReady,...frames.flatMap(f=>[['frame',f.tick,f.demoTime,f.curTime],['end',f.tick,0,pointCount]]),['done',frames.length]];
  const capture=parseRecords(clocks);
  return {...capture,pointNames:pointNames??[],frames};
}

export function packetPlayer(entity) {
  const props=entity.properties;
  const cells=Object.keys(props).filter(key=>key.endsWith('.m_cellX'));
  const angles=Object.entries(props).filter(([key])=>key.endsWith('.m_angEyeAngles'));
  if(cells.length!==1 || angles.length!==1 || !Array.isArray(angles[0][1])) return null;
  const prefix=cells[0].slice(0,-'m_cellX'.length);
  const origin=[];
  for(const axis of ['X','Y','Z']) {
    const cell=props[prefix+'m_cell'+axis],offset=props[prefix+'m_vec'+axis];
    if(!integer(cell) || !Number.isFinite(offset)) return null;
    origin.push(cell*512-16384+offset);
  }
  const view=angles[0][1];
  return finiteVector(view,3) && view!==null ? {origin,view,serial:entity.serial} : null;
}
const angleError=(a,b)=>Math.max(...a.slice(0,2).map((value,i)=>Math.abs(((value-b[i])%360+540)%360-180)));

export function alignCapture(capture,packets,{maxShift=4,minPairs=32,maxPositionError=0.001,maxAngleError=0.0001,minPositionMotion=0.1,minAngularMotion=0.1}={}) {
  assert(integer(maxShift) && maxShift>=1 && maxShift<=64 && integer(minPairs) && minPairs>0 && [maxPositionError,maxAngleError,minPositionMotion,minAngularMotion].every(n=>Number.isFinite(n) && n>0),'Invalid alignment validation parameters');
  const lookup=new Map();
  for(const frame of packets) {
    assert(integer(frame.tick) && !lookup.has(frame.tick) && Array.isArray(frame.entities),'Invalid/duplicate packet frame');
    const entities=new Map();
    for(const entity of frame.entities) {
      assert(integer(entity.entityId) && integer(entity.serial) && !entities.has(entity.entityId),'Invalid/duplicate packet entity');
      entities.set(entity.entityId,entity);
    }
    lookup.set(frame.tick,entities);
  }
  let positionMotion=0,angularMotion=0;
  const previous=new Map();
  for(const frame of capture.frames) for(const player of frame.players) {
    const key=`${player.entityId}:${player.controllerHandle}`;
    const before=previous.get(key);
    if(before && player.origin && player.view && before.origin && before.view) { positionMotion=Math.max(positionMotion,Math.hypot(...player.origin.map((value,i)=>value-before.origin[i]))); angularMotion=Math.max(angularMotion,angleError(player.view,before.view)); }
    previous.set(key,player);
  }
  const candidates=[];
  for(let shift=-maxShift;shift<=maxShift;shift++) {
    let pairs=0,missing=0,positionMax=0,angleMax=0;
    const lifetimes=new Map();
    for(const frame of capture.frames) for(const player of frame.players) {
      const entity=lookup.get(frame.tick+shift)?.get(player.entityId);
      const measured=entity?.className==='CCSPlayerPawn' ? packetPlayer(entity) : null;
      if(!measured || !player.origin || !player.view) { missing++;continue; }
      const identity=`${player.entityId}:${player.controllerHandle}`;
      if(lifetimes.has(identity) && lifetimes.get(identity)!==measured.serial) { missing++;continue; }
      lifetimes.set(identity,measured.serial);
      pairs++;
      positionMax=Math.max(positionMax,Math.hypot(...player.origin.map((value,i)=>value-measured.origin[i])));
      angleMax=Math.max(angleMax,angleError(player.view,measured.view));
    }
    candidates.push({packetTickShift:shift,pairs,missing,maxPositionError:positionMax,maxAngleError:angleMax,passes:missing===0 && pairs>=minPairs && positionMax<=maxPositionError && angleMax<=maxAngleError});
  }
  const passing=candidates.filter(candidate=>candidate.passes);
  const enough=capture.tickRate!==null && capture.missingTicks.length===0 && (positionMotion>=minPositionMotion || angularMotion>=minAngularMotion) && candidates.every(candidate=>candidate.missing===0 && candidate.pairs>=minPairs);
  const status=!enough ? 'insufficientData' : passing.length===1 ? 'matched' : passing.length>1 ? 'ambiguous' : 'mismatch';
  return {status,packetTickShift:status==='matched'?passing[0].packetTickShift:null,positionMotion,angularMotion,candidates,validation:{maxShift,minPairs,maxPositionError,maxAngleError,minPositionMotion,minAngularMotion},scope:'Position/view alignment only; attachment bone timing and asset versions remain unverified'};
}

// Reject a bad local window without discarding independently validated parts of a long capture.
export function alignCaptureWindows(capture,packets) {
  const windows=[];
  for(let first=capture.firstTick;first<=capture.lastTick;first+=128) {
    const last=Math.min(first+127,capture.lastTick);
    const frames=capture.frames.filter(f=>f.tick>=first && f.tick<=last);
    const missingTicks=capture.missingTicks.filter(t=>t>=first && t<=last);
    const alignment=alignCapture({...capture,firstTick:first,lastTick:last,frames,missingTicks},packets.filter(f=>f.tick>=first-4 && f.tick<=last+4));
    windows.push({firstTick:first,lastTick:last,...alignment});
  }
  return windows;
}

async function main() {
  if(process.argv[2]==='--validate') {
    const capture=parseCapture(captureText(await readFile(process.argv[3])));
    assert.equal(capture.missingTicks.length,0,'Capture has missing ticks');
    console.log(JSON.stringify({firstTick:capture.firstTick,lastTick:capture.lastTick,tickRate:capture.tickRate,frames:capture.frames.length,peakRssBytes:process.resourceUsage().maxRSS*1024}));return;
  }
  const [logFile,packetFile,output]=process.argv.slice(2);
  assert(logFile && packetFile && output,'Usage: node scripts/analyze-attachment-capture.mjs CONSOLE.log PACKETS.json OUTPUT.json');
  const capture=parseCapture(captureText(await readFile(logFile)));
  const packetBytes=await readFile(packetFile);
  const scene=JSON.parse(packetBytes.toString('utf8').replace(/^\uFEFF/,''));
  const packets=requireArtifact(scene,'packet-scene');
  const alignment=alignCapture(capture,packets);
  // Exclusive create preserves previous evidence; failed writes cannot be mistaken for valid JSON.
  await writeFile(output,JSON.stringify(artifact('attachment-alignment',{capture,alignment},{source:scene.source,dependencies:[fingerprint(await readFile(logFile)),fingerprint(packetBytes)]}),null,2),{flag:'wx'});
  console.log(JSON.stringify({frames:capture.frames.length,alignment:alignment.status,packetTickShift:alignment.packetTickShift}));
}
if(process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) await main();
