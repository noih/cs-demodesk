import test from 'node:test';
import {gunzipSync} from 'node:zlib';
import {auditJournal} from './check-analysis-run.mjs';
import assert from 'node:assert/strict';
import {buildInputs,buildInputWindows,buildCaptureWindows,writeBodyJournal} from './build-crosshair-input.mjs';
import {mkdtemp,writeFile,readFile,readdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {artifact} from './analysis-contract.mjs';
test('adapter requires source/identity alignment and splits rounds/models and refuses ambiguous pawn lifetimes',async t=>{
  const players=tick=>[1,2].map(id=>({entityId:id,controllerHandle:id+10,playerId:String(id),team:id===1?2:3,origin:[tick*tick,id,0],eye:[tick*tick,id,64],view:[0,tick*tick%180,0],attachments:{point:{position:[tick*tick,id,70],rotation:[0,0,0,1]}}}));
  const capture={identityProtocol:true,firstTick:10,lastTick:49,tickRate:64,missingTicks:[],frames:Array.from({length:40},(_,i)=>({tick:i+10,players:players(i+8)}))};
  const source={demoFingerprint:'source',gameBuild:null,mapContentFingerprint:null};
  const scene=artifact('packet-scene',Array.from({length:48},(_,i)=>{const tick=i+6;return {tick,entities:players(tick).map(p=>({entityId:p.entityId,serial:1,className:'CCSPlayerPawn',properties:{'p.m_cellX':32,'p.m_cellY':32,'p.m_cellZ':32,'p.m_vecX':p.origin[0],'p.m_vecY':p.origin[1],'p.m_vecZ':0,'p.m_angEyeAngles':p.view,'p.m_hModel':tick<35?'1':'2'}}))};}),{source});
  const context=artifact('match-context',{tickRate:64,players:[{steamid:'1'},{steamid:'2'}],rounds:[{round:1,freezeEndTick:8,endTick:29},{round:2,freezeEndTick:30,endTick:50}]},{source});
  const result=buildInputs(capture,scene,context,0.01);
  const compact=structuredClone(scene);compact.contract.module='player-tracking-scene';
  assert.deepEqual(buildInputs(capture,compact,context,0.01).inputs,result.inputs);
  for(const input of result.inputs)assert.equal(new Set(input.tracks.map(t=>JSON.stringify([t.round,t.targetId,t.pointId]))).size,input.tracks.length);

  assert.equal(result.inputs.length,2);assert.equal(result.alignment.packetTickShift,-2);
  const directory=await mkdtemp(path.join(tmpdir(),'demodesk-journal-'));t.after(()=>rm(directory,{recursive:true,force:true}));
  const file=path.join(directory,'scene.ndjson');
  const rows=[artifact('scene-journal',{profile:'player-tracking'},{source}),...scene.data.map((frame,index)=>index===0?{tick:frame.tick,netTick:frame.tick,fields:[['properties']],create:frame.entities}:{tick:frame.tick,netTick:frame.tick,set:frame.entities.map(e=>[e.entityId,0,e.properties])}),{end:scene.data.length,lastTick:scene.data.at(-1).tick}];
  await writeFile(file,rows.map(r=>JSON.stringify(r)).join('\n')+'\n');
  const batches=[];for await(const batch of buildInputWindows(capture,file,context,0.01))batches.push(batch);
  assert.deepEqual(batches.flatMap(b=>b.inputs),result.inputs,'Streamed windows preserve existing adapter output');
  const pieces=[{...capture,lastTick:29,frames:capture.frames.slice(0,20)},{...capture,firstTick:30,frames:capture.frames.slice(20)}];
  const segmented=[];for await(const batch of buildCaptureWindows(pieces,file,context,0.01))segmented.push(batch);
  const samples=inputs=>inputs.flatMap(i=>i.tracks.flatMap(t=>t.samples.map(s=>JSON.stringify([i.playerId,t.round,t.pointId,s])))).sort();
  assert.deepEqual(samples(segmented.flatMap(b=>b.inputs)),samples(result.inputs),'Adjacent captures preserve every measured sample');
  await assert.rejects(async()=>{for await(const _ of buildCaptureWindows([pieces[1],pieces[0]],file,context,0.01)){}},/overlap|order/);
  const bodyFile=path.join(directory,'body.ndjson');
  const clock={tickRate:64,sampleStepTicks:1,angularResolutionDegrees:0.01,measurementSource:result.inputs[0].measurementSource};
  await writeBodyJournal(batches,bodyFile,source,[],clock);
  const bodyRows=(await readFile(bodyFile,'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(bodyRows[0].contract.module,'body-measurement-journal');
  assert.equal(bodyRows[0].contract.schemaVersion,2);
  assert.equal(bodyRows.at(-1).end,40);
  assert.equal(bodyRows.at(-1).alignment[0].status,'matched');
  const audit=await auditJournal(bodyFile,context);assert.equal(audit.frames,40);assert.equal(audit.samples,80);
  const missingAlignment=structuredClone(bodyRows);missingAlignment.at(-1).alignment=[];
  const invalidAudit=path.join(directory,'unqualified.ndjson');await writeFile(invalidAudit,missingAlignment.map(JSON.stringify).join('\n')+'\n');
  await assert.rejects(()=>auditJournal(invalidAudit,context),/approved alignment/);
  const compressed=bodyFile+'.gz';
  await writeBodyJournal(batches,compressed,source,[],clock);
  assert.equal(gunzipSync(await readFile(compressed)).toString(),await readFile(bodyFile,'utf8'));
  async function* interrupted(){yield batches[0];throw Error('interrupted capture');}
  await assert.rejects(()=>writeBodyJournal(interrupted(),path.join(directory,'failed.gz'),source,[],clock),/interrupted/);
  assert(!(await readdir(directory)).some(name=>name.startsWith('failed.gz')),'Failed compression must leave no published or pending output');
  assert.equal(bodyRows[1].active.length,2);
  assert.equal(bodyRows[2].active,undefined,'Unchanged active identities are not repeated');
  await assert.rejects(()=>writeBodyJournal(batches,bodyFile,source,[],clock),/EEXIST/);
  await writeFile(file,rows.slice(0,-1).map(r=>JSON.stringify(r)).join('\n')+'\n');
  await assert.rejects(async()=>{for await(const _ of buildInputWindows(capture,file,context,0.01)){}},/end/);

  for(const input of result.inputs) for(const track of input.tracks) {
    assert.ok(track.samples.every(s=>s.obstruction==='unknown'));
    const first=track.samples[0].tick,last=track.samples.at(-1).tick;
    assert.ok(!(first<30 && last>=30) && !(first<35 && last>=35),'No track crosses a round or model boundary');
  }
  assert.throws(()=>buildInputs({...capture,identityProtocol:false},scene,context,0.01));
  scene.data.find(f=>f.tick===25).entities[0].serial=2;
  assert.throws(()=>buildInputs(capture,scene,context,0.01),/alignment/);
  context.source={...source,demoFingerprint:'different'};
  assert.throws(()=>buildInputs(capture,scene,context,0.01),/Mismatched/);
});
