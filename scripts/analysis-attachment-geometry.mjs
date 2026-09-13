// Cross-check engine attachments against a single-bone model definition.
// This verifies relative geometry, not network bone timing or historical asset compatibility.
import assert from 'node:assert/strict';
export const ATTACHMENT_GEOMETRY_VERSION='0.2.0';
const vector=(v,n)=>Array.isArray(v) && v.length===n && v.every(Number.isFinite);
const conjugate=([x,y,z,w])=>[-x,-y,-z,w];
const multiply=([x,y,z,w],[a,b,c,d])=>[w*a+x*d+y*c-z*b,w*b-x*c+y*d+z*a,w*c+x*b-y*a+z*d,w*d-x*a-y*b-z*c];
function quaternion(value) {
  assert(vector(value,4) && Math.abs(Math.hypot(...value)-1)<0.001,'Expected unit x/y/z/w quaternion');
  const norm=Math.hypot(...value);
  return value.map(v=>v/norm);
}
const rotate=(q,v)=>multiply(multiply(q,[...v,0]),conjugate(q)).slice(0,3);

export function attachmentBoneTransform(sample,definition,scale) {
  assert(Number.isFinite(scale) && scale>0,'Invalid model scale');
  assert(definition.influences===1 && JSON.stringify(definition.weights)==='[1,0,0]' && JSON.stringify(definition.rootTransforms)==='[false,false,false]' && definition.ignoreRotation===false,'Unsupported attachment transform');
  assert(vector(definition.offset,3) && vector(sample.position,3),'Invalid attachment position');
  const rotation=multiply(quaternion(sample.rotation),conjugate(quaternion(definition.rotation)));
  const offset=rotate(rotation,definition.offset.map(v=>v*scale));
  const origin=sample.position.map((v,i)=>v-offset[i]);
  return {transformPoint(point) {
    assert(vector(point,3),'Invalid bone-local point');
    const rotated=rotate(rotation,point.map(v=>v*scale));
    return origin.map((v,i)=>v+rotated[i]);
  },rotation};
}

export function compareAttachments(points,model,observedModelId,{scale=1,maxPositionError=0.001,maxAngleError=0.01}={}) {
  assert(model && typeof model.resourceId==='string' && model.resourceId===observedModelId,'Attachment model identity mismatch');
  assert(typeof model.contentFingerprint==='string' && /^(sha1:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/.test(model.contentFingerprint),'Missing model content fingerprint');
  assert([scale,maxPositionError,maxAngleError].every(n=>Number.isFinite(n) && n>0),'Invalid attachment validation parameters');
  const definitions=Object.entries(model.definitions).sort(([a],[b])=>a.localeCompare(b));
  assert(definitions.length>=2,'Expected at least two independently sampled attachments');
  const bone=definitions[0][1].bone;
  assert(typeof bone==='string' && bone.length,'Missing bone identity');
  const local=new Map();
  for(const [name,d] of definitions) {
    assert(d.bone===bone && d.influences===1 && JSON.stringify(d.weights)==='[1,0,0]' && JSON.stringify(d.rootTransforms)==='[false,false,false]' && d.ignoreRotation===false,'Unsupported multi-bone/root/rotation attachment');
    assert(vector(d.offset,3),'Invalid attachment offset');
    local.set(name,{position:d.offset.map(v=>v*scale),rotation:quaternion(d.rotation)});
  }
  const missing=definitions.map(([name])=>name).filter(name=>!points[name]?.position || !points[name]?.rotation);
  if(missing.length) return {status:'insufficientData',bone,missing,comparisons:[]};
  const world=new Map(definitions.map(([name])=>{
    assert(vector(points[name].position,3),'Invalid sampled attachment position');
    return [name,{position:points[name].position,rotation:quaternion(points[name].rotation)}];
  }));
  const [anchor,anchorDefinition]=definitions[0];
  const transform=attachmentBoneTransform(world.get(anchor),anchorDefinition,scale);
  const boneRotation=transform.rotation;
  const comparisons=definitions.slice(1).map(([name])=>{
    const target=world.get(name),definition=local.get(name);
    const predictedPosition=transform.transformPoint(model.definitions[name].offset);
    const predictedRotation=multiply(boneRotation,definition.rotation);
    const positionError=Math.hypot(...target.position.map((v,i)=>v-predictedPosition[i]));
    const cosine=Math.min(1,Math.abs(target.rotation.reduce((sum,v,i)=>sum+v*predictedRotation[i],0)));
    const angleError=2*Math.acos(cosine)*180/Math.PI;
    return {anchor,target:name,positionError,angleError,passes:positionError<=maxPositionError && angleError<=maxAngleError};
  });
  return {status:comparisons.every(c=>c.passes)?'matched':'mismatch',bone,missing:[],comparisons,validation:{scale,maxPositionError,maxAngleError},scope:'Relative rendered attachment geometry only; network bone timing and asset version match remain unverified'};
}
