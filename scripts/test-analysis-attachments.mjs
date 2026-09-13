import test from 'node:test';
import {gzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {parseCapture,captureText,alignCapture} from './analyze-attachment-capture.mjs';
import {startAttachmentCapture} from './capture-analysis-attachments.mjs';

test('bounded render capture preserves frame hooks and emits complete short records without inventing missing attachments',t=>{
  const original=globalThis.mirv;
  t.after(()=>{ if(original===undefined) delete globalThis.mirv; else globalThis.mirv=original; });
  let tick=2;
  const lines=[],prior=()=>({fov:90});
  const player={isValid:()=>true,isPlayerPawn:()=>true,getHealth:()=>100,getTeam:()=>3,getPlayerControllerHandle:()=>123,getOrigin:()=>[1,2,3],getRenderEyeOrigin:()=>[1,2,65],getRenderEyeAngles:()=>[0,45,0],getAttachment:name=>name==='head' ? {position:{x:1,y:2,z:64},angles:{x:0,y:0,z:0,w:1}} : null};
  globalThis.mirv={onClientFrameStageNotify:prior,isPlayingDemo:()=>true,getDemoTick:()=>tick,getDemoTime:()=>tick/64,getCurTime:()=>100+tick/64,getHighestEntityIndex:()=>1,getEntityFromIndex:index=>index===1 ? player : null,message:line=>lines.push(line)};
  assert.throws(()=>startAttachmentCapture({firstTick:0,lastTick:4096,attachments:['head']}));
  startAttachmentCapture({firstTick:2,lastTick:3,attachments:['head','missing'],stateChanges:false});
  mirv.onClientFrameStageNotify({curStage:11,isBefore:false});
  mirv.onClientFrameStageNotify({curStage:12,isBefore:true});
  assert.equal(lines.length,1);
  assert.deepEqual(mirv.onClientFrameStageNotify({curStage:12,isBefore:false}),{fov:90});
  mirv.onClientFrameStageNotify({curStage:12,isBefore:false}); // Duplicate render tick must not create another sample.
  tick=3;mirv.onClientFrameStageNotify({curStage:12,isBefore:false});
  tick=4;mirv.onClientFrameStageNotify({curStage:12,isBefore:false});
  assert.equal(mirv.onClientFrameStageNotify,prior);
  assert.ok(lines.every(line=>line.length<=221));
  const records=lines.map(line=>JSON.parse(line.slice('DEMODESK_POSE '.length)));
  assert.deepEqual(records.filter(row=>row[0]==='frame').map(row=>row[1]),[2,3]);
  assert.deepEqual(records.find(row=>row[0]==='point' && row[3]==='head'),['point',2,1,'head',[1,2,64]]);
  assert.equal(records.find(row=>row[0]==='point' && row[3]==='missing')[4],null);
  assert.deepEqual(records.filter(row=>row[0]==='end'),[['end',2,1,2],['end',3,1,2]]);
  assert.deepEqual(records.at(-1),['done',2]);
  const log=lines.join('');
  const capture=parseCapture(log);
  const compressed=gzipSync(Buffer.from(log));
  assert.deepEqual(parseCapture(captureText(compressed)),capture);
  assert.throws(()=>captureText(compressed.subarray(0,compressed.length-3)));

  assert.deepEqual(parseCapture(log.replaceAll('DEMODESK_POSE ["frame"','engine diagnostic without newline DEMODESK_POSE ["frame"')),capture);
  assert.throws(()=>parseCapture(log.replace('DEMODESK_POSE ["frame"','DEMODESK_POSE ["truncated"')));

  const identifiedRecords=records.flatMap(row=>row[0]==='ready'?[ [...row,2] ]:row[0]==='player'?[row,['identity',row[1],row[2],'42']]:[row]);
  const identifiedLog=identifiedRecords.map(row=>'DEMODESK_POSE '+JSON.stringify(row)).join('\n');
  assert.equal(parseCapture(identifiedLog).frames[0].players[0].playerId,'42');
  assert.throws(()=>parseCapture(identifiedLog.replace('"42"','42')),/identity/);
  assert.equal(capture.tickRate,64);assert.deepEqual(capture.missingTicks,[]);
  assert.equal(alignCapture(capture,[]).status,'insufficientData');
  assert.throws(()=>parseCapture(lines.slice(0,-1).join('')),/done/);
  assert.throws(()=>parseCapture(log+log));
  assert.throws(()=>parseCapture(log.replace('["end",2,1,2]','["end",2,2,2]')));
  const deltaStart=lines.length;
  let health=100,x=0.123456789;
  player.getHealth=()=>health;player.getOrigin=()=>[x,2,3];
  tick=10;startAttachmentCapture({firstTick:10,lastTick:14,attachments:['head','missing']});
  for(tick=10;tick<=15;tick++){
    if(tick===12)x=5;
    if(tick===13)health=0;
    if(tick===14)health=100;
    mirv.onClientFrameStageNotify?.({curStage:12,isBefore:false});
  }
  const deltaLog=lines.slice(deltaStart).join(''),delta=parseCapture(deltaLog);
  assert.equal(delta.frames.length,5);assert.equal(delta.frames[2].players[0].origin[0],5);
  assert.equal(delta.frames[0].players[0].origin[0],0.123456789,'Non-f32 measurements retain their precision and historical state');
  assert.equal(delta.frames[3].players.length,0);assert.equal(delta.frames[4].players.length,1);
  assert.equal(deltaLog.split('"player"').length-1,2,'Metadata is emitted only for new player states');
  assert.equal(deltaLog.split('"origin"').length-1,3,'Unchanged positions are not repeated');
  assert.throws(()=>parseCapture(deltaLog.replace(/^DEMODESK_POSE \["reset".*\n/m,'')),/delta/);
  tick=3;startAttachmentCapture({firstTick:2,lastTick:4,attachments:['head']});
  mirv.onClientFrameStageNotify({curStage:12,isBefore:false});tick=2;mirv.onClientFrameStageNotify({curStage:12,isBefore:false});
  assert.equal(mirv.onClientFrameStageNotify,prior);
  assert.match(lines.at(-1),/DEMODESK_POSE_ERROR.*backwards/);
});


test('alignment measures a unique shift and refuses gaps, changed lifetimes and missing motion',()=>{
  const frames=Array.from({length:4},(_,i)=>({tick:10+i,players:[{entityId:1,controllerHandle:5,origin:[(8+i)**2,0,0],view:[0,(8+i)**2,0]}]}));
  const capture={frames,tickRate:64,missingTicks:[]};
  const packets=Array.from({length:12},(_,i)=>{
    const tick=6+i;
    return {tick,entities:[{entityId:1,serial:1,className:'CCSPlayerPawn',properties:{'p.m_cellX':32,'p.m_cellY':32,'p.m_cellZ':32,'p.m_vecX':tick**2,'p.m_vecY':0,'p.m_vecZ':0,'p.m_angEyeAngles':[0,tick**2,0]}}]};
  });
  const options={maxShift:4,minPairs:4};
  const result=alignCapture(capture,packets,options);
  assert.equal(result.status,'matched');assert.equal(result.packetTickShift,-2);
  assert.equal(alignCapture(capture,packets.slice(1),options).status,'insufficientData');
  packets.find(frame=>frame.tick===9).entities[0].serial=2;
  assert.notEqual(alignCapture(capture,packets,options).status,'matched');
  const stationary=structuredClone(capture);
  for(const frame of stationary.frames) frame.players[0].origin=[0,0,0],frame.players[0].view=[0,0,0];
  assert.equal(alignCapture(stationary,packets,options).status,'insufficientData');
});
