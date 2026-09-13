import test from 'node:test';
import assert from 'node:assert/strict';
import {SceneJournal} from './analysis-journal.mjs';
import {artifact} from './analysis-contract.mjs';
test('state changes share unchanged data and preserve historical frames, array edits, null and deletion',()=>{
 const j=new SceneJournal(artifact('scene-journal',{profile:'scene'}));
 const entity={entityId:1,serial:1,className:'Pawn',properties:{model:'fixed',position:[1.25,2,3],optional:7},smokeVoxelBytes:[0,null,0]};
 const first=j.apply({tick:1,netTick:100,create:[entity]});
 const second=j.apply({tick:2,netTick:101,fields:[['properties','position','1'],['smokeVoxelBytes','1'],['properties','optional']],set:[[1,0,4],[1,1,255]],unset:[[1,2]]});
 assert.deepEqual(first.entities[0],entity);assert.equal(second.entities[0].properties.position[1],4);
 assert.equal(second.entities[0].smokeVoxelBytes[1],255);assert(!Object.hasOwn(second.entities[0].properties,'optional'));
 const third=j.apply({tick:3,netTick:102});assert.equal(third.entities[0],second.entities[0]);
 j.apply({tick:4,netTick:103,remove:[1]});
 j.apply({tick:5,netTick:104,create:[{...entity,serial:2}]});
 assert.equal(j.apply({end:5,lastTick:5}),null);assert.throws(()=>j.apply({tick:6,netTick:105}));
});
test('journal refuses unknown versions, missing parents, duplicate clocks and prototype paths',()=>{
 const make=()=>new SceneJournal(artifact('scene-journal',{profile:'player-tracking'}));
 const j=make();j.apply({tick:1,netTick:1,create:[{entityId:1,serial:1,className:'Pawn'}]});
 assert.throws(()=>j.apply({tick:1,netTick:1}));
 assert.throws(()=>make().apply({tick:1,netTick:1,fields:[['__proto__','polluted']]}));
 assert.throws(()=>j.apply({tick:2,netTick:2,fields:[['missing','child']],set:[[1,0,1]]}));
 assert.throws(()=>make().apply({end:1,lastTick:1}));
});
