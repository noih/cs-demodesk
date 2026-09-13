import test from 'node:test';
import assert from 'node:assert/strict';
import {compareAttachments} from './analysis-attachment-geometry.mjs';
const definition=(offset,rotation=[0,0,0,1])=>({bone:'spine',offset,rotation,weights:[1,0,0],rootTransforms:[false,false,false],influences:1,ignoreRotation:false});
test('relative attachment geometry respects local rotations, world rotation, scale and quaternion sign',()=>{
  const q=Math.SQRT1_2;
  const model={resourceId:'1',contentFingerprint:'sha256:'+'a'.repeat(64),definitions:{a:definition([1,0,0],[0,0,q,q]),b:definition([0,1,0])}};
  const points={a:{position:[10,22,30],rotation:[0,0,1,0]},b:{position:[8,20,30],rotation:[0,0,-q,-q]}};
  assert.equal(compareAttachments(points,model,'1',{scale:2}).status,'matched');
  points.b.position[0]+=0.1;
  assert.equal(compareAttachments(points,model,'1',{scale:2}).status,'mismatch');
  points.b=null;
  assert.equal(compareAttachments(points,model,'1',{scale:2}).status,'insufficientData');
  assert.throws(()=>compareAttachments(points,model,'2'),/identity/);
  model.definitions.b.influences=2;
  assert.throws(()=>compareAttachments(points,model,'1'),/Unsupported/);
});
