import test from 'node:test';
import assert from 'node:assert/strict';
import { entityTransform, worldToModel, traceScene } from './analysis-scene-geometry.mjs';
import {intersect,trace,loadPhysics} from './analysis-physics.mjs';
const provider=async file=>{ const {gltf,bytes}=await loadPhysics(file); return {trace:(start,end)=>trace(gltf,bytes,start,end)}; };
import { modelResourceId, resolveModels } from './resolve-analysis-models.mjs';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
const a=[0,-1,-1], b=[0,1,-1], c=[0,0,1];
test('segment intersects either winding; misses, parallel rays and endpoints are excluded',()=>{
  assert.equal(intersect([-1,0,0],[1,0,0],a,b,c),0.5);
  assert.equal(intersect([-1,0,0],[1,0,0],c,b,a),0.5);
  assert.equal(intersect([-1,2,0],[1,2,0],a,b,c),null);
  assert.equal(intersect([-1,0,0],[-1,1,0],a,b,c),null);
  assert.equal(intersect([-1,0,0],[0,0,0],a,b,c),null);
});
test('physics hit retains surface but never declares visibility; rejects corrupt geometry',()=>{
  const {g,bytes}=fixture();
  const result=trace(g,bytes,[-1,0,0],[1,0,0]);
  assert.equal(result.hits[0].surface,'glass');
  assert.equal(result.visibility,'unknown');
  assert.equal(result.triangles,1);
  assert.throws(()=>trace(g,bytes.subarray(0,40),[-1,0,0],[1,0,0]));
  bytes.writeUInt16LE(3,36);
  assert.throws(()=>trace(g,bytes,[-1,0,0],[1,0,0]));
});

function fixture() {
  const bytes=Buffer.alloc(42);
  [...a,...b,...c].forEach((v,i)=>bytes.writeFloatLE(v,i*4));
  [0,1,2].forEach((v,i)=>bytes.writeUInt16LE(v,36+i*2));
  const matrix=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
  const g={asset:{generator:'Source 2 Viewer test'},scenes:[{nodes:[0]}],nodes:[{matrix,mesh:0,extras:{SurfaceProperty:'glass',InteractAs:[]}}],meshes:[{primitives:[{attributes:{POSITION:0},indices:1}]}],accessors:[{type:'VEC3',componentType:5126,count:3,bufferView:0},{type:'SCALAR',componentType:5123,count:3,bufferView:1}],bufferViews:[{buffer:0,byteLength:36},{buffer:0,byteOffset:36,byteLength:6}]};
  g.buffers=[{uri:"shape.bin",byteLength:bytes.length}];
  return {g,bytes};
}

function rigid(id, model, angles=[0,90,0]) {
  const prefix='CPhysicsPropMultiplayer.CBodyComponentBaseAnimGraph.';
  const values={m_hModel:model,m_hParent:16777215,m_cellX:32,m_cellY:32,m_cellZ:32,m_vecX:10,m_vecY:20,m_vecZ:30,m_angRotation:angles,m_flScale:2,m_flRootBoneOffset_x:0,m_flRootBoneOffset_y:0,m_flRootBoneOffset_z:0,m_hGraphDefinitionAG2:'0',m_hSequence:'0'};
  return {entityId:id,serial:7,className:'CPhysicsPropMultiplayer',properties:Object.fromEntries(Object.entries(values).map(([key,value])=>[prefix+key,value]))};
}
const close=(actual,expected)=>actual.forEach((v,i)=>assert.ok(Math.abs(v-expected[i])<1e-9,`${actual} != ${expected}`));

test('model IDs match real Source 2 resources without losing 64-bit precision or choosing duplicate packages',()=>{
  const file='models/props/de_dust/hr_dust/dust_soccerball/dust_soccer_ball001.vmdl_c';
  assert.equal(modelResourceId(file),'11435759149502318708');
  assert.equal(modelResourceId(file.toUpperCase()),'11435759149502318708');
  assert.equal(modelResourceId('agents/models/ctm_sas/ctm_sas.vmdl_c'),'16117112829760262417');
  assert.throws(()=>modelResourceId('../model.vmdl_c'));
  const frames=[{entities:[rigid(1,'11435759149502318708'),rigid(2,'123')]}];
  const listing=file+' CRC:00c06bf4b6 size:12647';
  const result=resolveModels(frames,[{vpk:'map',listing},{vpk:'base',listing}]);
  assert.equal(result.models[0].candidates.length,2);
  assert.deepEqual(result.unresolved,['123']);
  assert.equal(result.assetVersionMatch,'unverified');
  frames[0].entities[0].properties['CPhysicsPropMultiplayer.CBodyComponentBaseAnimGraph.m_hModel']=11435759149502318708;
  assert.throws(()=>resolveModels(frames,[]));
});

test('cell position, Source pitch/yaw/roll and scale use inverse transforms; missing/parented/animated data is refused',()=>{
  const entity=rigid(1,'123');
  close(entityTransform(entity).origin,[10,20,30]);
  close(worldToModel([10,22,30],entityTransform(entity)),[1,0,0]);
  close(worldToModel([0,0,-2],{origin:[0,0,0],angles:[90,0,0],scale:2}),[1,0,0]);
  close(worldToModel([0,0,2],{origin:[0,0,0],angles:[0,0,90],scale:2}),[0,1,0]);
  const props=entity.properties,prefix='CPhysicsPropMultiplayer.CBodyComponentBaseAnimGraph.';
  props[prefix+'m_hParent']=13;
  assert.throws(()=>entityTransform(entity),/attachment/);
  props[prefix+'m_hParent']=16777215;props[prefix+'m_hSequence']='5';
  assert.throws(()=>entityTransform(entity),/Animated/);
  props[prefix+'m_hSequence']='0';delete props[prefix+'m_cellX'];
  assert.throws(()=>entityTransform(entity),/coordinate/);
});

test('scene rays follow the selected entity transform and expose unresolved models',async t=>{
  const directory=await mkdtemp(path.join(os.tmpdir(),'crosshair-geometry-'));
  t.after(async()=>{ assert.ok(path.resolve(directory).startsWith(path.resolve(os.tmpdir())+path.sep+'crosshair-geometry-')); await rm(directory,{recursive:true,force:true}); });
  const {g,bytes}=fixture();
  await writeFile(path.join(directory,'shape.gltf'),JSON.stringify(g));
  await writeFile(path.join(directory,'shape.bin'),bytes);
  const entity=rigid(1,'123'),frame={tick:100,netTick:120,entities:[entity,rigid(2,'456')]};
  const files={'123':path.join(directory,'shape.gltf')};
  const hit=await traceScene(frame,files,[10,18,30],[10,22,30],provider);
  assert.equal(hit.hits.length,1); assert.equal(hit.hits[0].fraction,0.5);
  close(hit.hits[0].point,[10,20,30]);assert.equal(hit.hits[0].serial,7);
  assert.equal(hit.unavailable[0].resourceId,'456');assert.equal(hit.visibility,'unknown');
  entity.properties['CPhysicsPropMultiplayer.CBodyComponentBaseAnimGraph.m_vecX']=20;
  assert.equal((await traceScene(frame,files,[10,18,30],[10,22,30],provider)).hits.length,0);
});
