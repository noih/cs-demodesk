// Read-only coverage audit of a completed body journal. No player identifiers are emitted.
import assert from 'node:assert/strict';
import {createReadStream} from 'node:fs';
import {readFile,writeFile} from 'node:fs/promises';
import {createInterface} from 'node:readline';
import {createGunzip} from 'node:zlib';
import {pathToFileURL} from 'node:url';
export async function auditJournal(file,context) {
  const source=createReadStream(file),decoded=file.endsWith('.gz')?source.pipe(createGunzip()):source;
  source.on('error',error=>decoded.destroy(error));
  const lines=createInterface({input:decoded,crlfDelay:Infinity});
  let header=null,footer=null,lastTick=null,frames=0,samples=0,active=[],gaps=0,continuous=0;
  const definitions=new Map(),lastSeen=new Map(),ranges=[];
  const rounds=context.data.rounds;
  try {for await(const line of lines){
    const row=JSON.parse(line);
    if(!header){header=row;assert.equal(header.source.demoFingerprint,context.source.demoFingerprint);continue;}
    assert(!footer,'Data after journal footer');
    if(row.end!==undefined){footer=row;assert.equal(row.end,frames);assert.equal(row.lastTick,lastTick);continue;}
    assert(Number.isInteger(row.tick)&&(lastTick===null||row.tick>lastTick),'Non-increasing tick');
    assert(rounds.some(r=>r.freezeEndTick<=row.tick&&row.tick<=r.endTick),'Measurement outside a live round');
    for(const [key,value] of Object.entries(row.define??{})){assert(!definitions.has(key));definitions.set(key,value);}
    if(row.active)active=row.active;
    assert.equal(new Set(active).size,active.length);
    for(const id of active){
      assert(definitions.has(id),'Unknown track');
      if(lastSeen.has(id)){if(row.tick-lastSeen.get(id)===header.data.sampleStepTicks)continuous++;else gaps++;}
      lastSeen.set(id,row.tick);samples++;
    }
    const range=ranges.at(-1);if(range&&row.tick===range[1]+1)range[1]=row.tick;else ranges.push([row.tick,row.tick]);
    lastTick=row.tick;frames++;
  }}finally{lines.close();decoded.destroy();source.destroy();}
  assert(footer,'Missing footer');assert(Array.isArray(footer.alignment),'Alignment audit missing');
  const matched=footer.alignment.filter(w=>w.status==='matched').map(w=>[w.firstTick+w.packetTickShift,w.lastTick+w.packetTickShift]);
  for(const [first,last] of ranges)for(let tick=first;tick<=last;tick++)assert(matched.some(([a,b])=>a<=tick&&tick<=b),'Measurement without approved alignment');
  const reasons={};for(const window of footer.alignment)reasons[window.status]=(reasons[window.status]??0)+1;
  const boundaries=footer.alignment.slice(1).filter((w,i)=>w.firstTick!==footer.alignment[i].lastTick+1).length;
  return {frames,samples,tracks:definitions.size,continuousTrackSteps:continuous,trackDiscontinuities:gaps,measuredTickRuns:ranges.length,alignmentWindows:footer.alignment.length,alignmentStatuses:reasons,uncoveredRenderWindowGaps:boundaries};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){const [file,contextPath,output]=process.argv.slice(2);assert(file&&contextPath&&output,'Expected JOURNAL CONTEXT SUMMARY');const summary=await auditJournal(file,JSON.parse(await readFile(contextPath,'utf8')));await writeFile(output,JSON.stringify(summary,null,2),{flag:'wx'});console.log(JSON.stringify(summary));}
