// Dynamic rigid transforms depend only on a provider returning {trace(start,end)}.
import assert from 'node:assert/strict';
export const DYNAMIC_VERSION='0.1.0';
const vector=v=>Array.isArray(v) && v.length===3 && v.every(Number.isFinite);
const sub=(a,b)=>a.map((n,i)=>n-b[i]);
const dot=(a,b)=>a.reduce((n,x,i)=>n+x*b[i],0);

// Source's AngleMatrix convention: degrees in pitch, yaw, roll order.
export function worldToModel(point, {origin,angles,scale}) {
  assert(vector(point) && vector(origin) && vector(angles) && Number.isFinite(scale) && scale>0, 'Invalid rigid transform');
  const [pitch,yaw,roll]=angles.map(degrees => degrees*Math.PI/180);
  const cp=Math.cos(pitch),sp=Math.sin(pitch),cy=Math.cos(yaw),sy=Math.sin(yaw),cr=Math.cos(roll),sr=Math.sin(roll);
  const axes=[[cp*cy,cp*sy,-sp],[sr*sp*cy-cr*sy,sr*sp*sy+cr*cy,sr*cp],[cr*sp*cy+sr*sy,cr*sp*sy-sr*cy,cr*cp]];
  return axes.map(axis => dot(sub(point,origin),axis)/scale);
}

export function entityTransform(entity) {
  const props=entity.properties;
  const models=Object.keys(props).filter(name => name.endsWith('.m_hModel'));
  assert(models.length === 1, 'Missing or ambiguous model transform');
  const prefix=models[0].slice(0,-'m_hModel'.length);
  const get = name => props[prefix+name];
  assert([16777215,4294967295].includes(get('m_hParent')), 'Parented or unknown-parent transform requires attachment resolution');
  assert(['x','y','z'].every(axis => get('m_flRootBoneOffset_'+axis) === 0), 'Nonzero or missing root bone offset requires pose resolution');
  for (const key of ['m_hGraphDefinitionAG2','m_hSequence']) {
    assert(get(key) === undefined || get(key) === '0', 'Animated collision requires pose resolution');
  }
  const origin=['X','Y','Z'].map(axis => {
    const cell=get('m_cell'+axis),offset=get('m_vec'+axis);
    assert(Number.isInteger(cell) && cell>=0 && cell<=65535 && Number.isFinite(offset), 'Missing or invalid cell coordinate');
    return cell*512-16384+offset;
  });
  const transform={origin,angles:get('m_angRotation'),scale:get('m_flScale')};
  worldToModel(origin,transform); // Validate all transform components before returning evidence.
  return transform;
}

// Inject the physics provider; scene transforms do not depend on an exporter or disk layout.
export async function traceScene(frame, modelFiles, start, end, physicsProvider) {
  assert(Number.isInteger(frame.tick) && Number.isInteger(frame.netTick),'Expected one packet-aligned scene frame');
  assert(vector(start) && vector(end) && dot(sub(end,start),sub(end,start))>0,'Expected distinct finite endpoints');
  const assets=new Map(), hits=[], unavailable=[];
  let triangles=0, evaluatedEntities=0;
  for (const entity of frame.entities) {
    if (!['Door','Breakable','DynamicProp','PhysicsProp','PhysProp','FuncBrush','MovingToggle'].some(part => entity.className.includes(part))) continue;
    const identity={entityId:entity.entityId,serial:entity.serial,className:entity.className};
    let transform;
    try { transform=entityTransform(entity); }
    catch (error) { unavailable.push({...identity,reason:error.message}); continue; }
    const resourceId=Object.entries(entity.properties).find(([key])=>key.endsWith('.m_hModel'))[1];
    assert(typeof resourceId==='string' && /^(0|[1-9][0-9]*)$/.test(resourceId),'Model ID must remain an exact decimal string');
    const filename=Object.hasOwn(modelFiles,resourceId) ? modelFiles[resourceId] : null;
    if (!filename) { unavailable.push({...identity,resourceId,reason:'No resolved physics asset'}); continue; }
    if (!assets.has(filename)) assets.set(filename,await physicsProvider(filename));
    const physics=assets.get(filename);
    const result=physics.trace(worldToModel(start,transform),worldToModel(end,transform));
    evaluatedEntities++; triangles+=result.triangles;
    const collisionProperties=Object.fromEntries(Object.entries(entity.properties).filter(([name])=>/m_nSolidType$|m_usSolidFlags$|m_CollisionGroup$|m_fEffects$/.test(name)));
    for (const hit of result.hits) hits.push({...identity,resourceId,...hit,point:start.map((v,i)=>v+(end[i]-v)*hit.fraction),collisionProperties});
  }
  hits.sort((a,b)=>a.fraction-b.fraction || a.entityId-b.entityId || a.surface.localeCompare(b.surface));
  return {tick:frame.tick,netTick:frame.netTick,visibility:'unknown',assetVersionMatch:'unverified',evaluatedEntities,triangles,hits,unavailable};
}
