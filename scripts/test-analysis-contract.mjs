import test from 'node:test';
import assert from 'node:assert/strict';
import {artifact,requireArtifact,fingerprint} from './analysis-contract.mjs';
import {traceScene} from './analysis-scene-geometry.mjs';
test('versioned contracts reject unknown schemas and retain independent producer identity',()=>{
  const geometry=artifact('static-collision',{hits:[]});
  const smoke=artifact('smoke-journal',{records:[]});
  const oldGeometry=fingerprint(JSON.stringify(geometry));
  const oldSmoke=fingerprint(JSON.stringify(smoke));
  smoke.contract.implementationVersion='0.2.0';
  assert.equal(fingerprint(JSON.stringify(geometry)),oldGeometry);
  assert.notEqual(fingerprint(JSON.stringify(smoke)),oldSmoke);
  assert.deepEqual(requireArtifact(smoke,'smoke-journal'),{records:[]});
  assert.throws(()=>requireArtifact(smoke,'static-collision'));
  smoke.contract.schemaVersion=2;
  assert.throws(()=>requireArtifact(smoke,'smoke-journal'));
  assert.equal(geometry.source.gameBuild,null);
  assert.throws(()=>requireArtifact([], 'packet-scene'));
});
test('dynamic transforms accept an alternate geometry provider without glTF or filesystem',async()=>{
  const prefix='CPhysicsProp.CBodyComponentBaseAnimGraph.';
  const raw={m_hModel:'1',m_hParent:16777215,m_flRootBoneOffset_x:0,m_flRootBoneOffset_y:0,m_flRootBoneOffset_z:0,m_cellX:32,m_cellY:32,m_cellZ:32,m_vecX:0,m_vecY:0,m_vecZ:0,m_angRotation:[0,0,0],m_flScale:1};
  const entity={entityId:1,serial:1,className:'CPhysicsProp',properties:Object.fromEntries(Object.entries(raw).map(([k,v])=>[prefix+k,v]))};
  let calls=0;
  const result=await traceScene({tick:1,netTick:2,entities:[entity]}, {'1':'in-memory'},[0,0,0],[10,0,0],async id=>{
    assert.equal(id,'in-memory');calls++;
    return {trace:(start,end)=>{assert.deepEqual(start,[0,0,0]);assert.deepEqual(end,[10,0,0]);return {triangles:1,hits:[{fraction:0.5,surface:'test'}]};}};
  });
  assert.equal(calls,1);assert.deepEqual(result.hits[0].point,[5,0,0]);
});
