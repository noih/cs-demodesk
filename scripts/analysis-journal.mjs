// Immutable, structurally shared frames from a sequential state-change journal.
// Retain a bounded analysis window; unchanged entities/branches share their values.
import assert from 'node:assert/strict';
import {createReadStream} from 'node:fs';
import {createInterface} from 'node:readline';
import {requireArtifact} from './analysis-contract.mjs';
const integer=n=>Number.isSafeInteger(n)&&n>=0;
function patch(root,path,value,remove=false) {
  assert(path.length>0 && root!==null && typeof root==='object','Invalid patch parent');
  const [key,...rest]=path;
  const array=Array.isArray(root);
  assert(!array || (/^(0|[1-9][0-9]*)$/.test(key) && Number(key)<root.length),'Invalid array index');
  const next=array?root.slice():{...root};
  if(rest.length){assert(Object.hasOwn(root,key),'Missing patch parent');next[key]=patch(root[key],rest,value,remove);}
  else if(remove){assert(!array && Object.hasOwn(root,key),'Invalid unset');delete next[key];}
  else next[key]=value;
  return next;
}
export class SceneJournal {
  constructor(header) {
    const data=requireArtifact(header,'scene-journal');
    assert(['scene','player-tracking'].includes(data.profile),'Unsupported journal profile');
    this.header=header;this.fields=[];this.paths=new Set();this.entities=new Map();this.frames=0;this.lastTick=null;this.ended=false;
  }
  apply(record) {
    assert(!this.ended,'Data after journal end');
    if(Object.hasOwn(record,'end')) {
      assert(record.end===this.frames && record.lastTick===this.lastTick,'Incomplete journal');
      this.ended=true;return null;
    }
    assert(integer(record.tick) && integer(record.netTick) && (this.lastTick===null || record.tick>this.lastTick),'Invalid journal clock');
    for(const path of record.fields??[]) {
      assert(Array.isArray(path) && path.length>0 && path.length<=32 && path.every(k=>typeof k==='string' && k.length>0 && k.length<=4096 && !['__proto__','prototype','constructor'].includes(k)),'Invalid journal field path');
      const key=JSON.stringify(path);assert(!this.paths.has(key),'Repeated field declaration');this.paths.add(key);this.fields.push(path);
    }
    const changed=new Set();
    for(const id of record.remove??[]) {assert(integer(id) && this.entities.delete(id) && !changed.has(id),'Invalid entity removal');changed.add(id);}
    for(const entity of record.create??[]) {
      assert(integer(entity.entityId) && integer(entity.serial) && typeof entity.className==='string' && !changed.has(entity.entityId),'Invalid entity creation');
      const before=this.entities.get(entity.entityId);
      assert(!before || before.serial!==entity.serial || before.className!==entity.className,'Redundant entity recreation');
      this.entities.set(entity.entityId,entity);changed.add(entity.entityId);
    }
    for(const [operations,remove] of [[record.set??[],false],[record.unset??[],true]])for(const row of operations) {
      assert(Array.isArray(row)&&row.length===(remove?2:3),'Invalid journal operation');
      const [id,field,value]=row;
      assert(integer(id)&&integer(field)&&field<this.fields.length&&this.entities.has(id)&&!changed.has(id),'Invalid patch target');
      const path=this.fields[field];assert(!['entityId','serial','className'].includes(path[0]),'Identity must be recreated');
      this.entities.set(id,patch(this.entities.get(id),path,value,remove));
    }
    this.frames++;this.lastTick=record.tick;
    return {tick:record.tick,netTick:record.netTick,entities:[...this.entities.values()].sort((a,b)=>a.entityId-b.entityId)};
  }
}
export async function* readSceneJournal(file) {
  const stream=createReadStream(file,{encoding:'utf8'}),lines=createInterface({input:stream,crlfDelay:Infinity});
  let journal;
  try {
    for await(const line of lines) {
      assert(line.length>0 && line.length<=16*1024*1024,'Invalid journal line size');
      const row=JSON.parse(line);
      if(!journal){journal=new SceneJournal(row);yield {header:row};continue;}
      const frame=journal.apply(row);if(frame)yield {frame};
    }
    assert(journal?.ended,'Missing journal end');
  } finally {lines.close();stream.destroy();}
}
