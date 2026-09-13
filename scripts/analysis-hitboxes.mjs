// Model-local hitboxes -> rendered world capsules through an injected bone provider.
// Shape encoding: ValveResourceFormat ResourceTypes/ModelData/Hitbox.cs (Capsule = 2).
import assert from 'node:assert/strict';
export const HITBOX_VERSION='0.1.0';
const vector=v=>Array.isArray(v) && v.length===3 && v.every(Number.isFinite);
const contentId=id=>typeof id==='string' && /^(sha1:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/.test(id);
export function reconstructHitboxes(model,{resourceId,setIndex,scale,recordedContentFingerprint=null},boneProvider) {
  assert(contentId(model.contentFingerprint),'Missing actual model fingerprint');
  assert(recordedContentFingerprint===null || contentId(recordedContentFingerprint),'Invalid recorded model fingerprint');
  assert(typeof resourceId==='string' && typeof model.resourceId==='string','Model IDs must be exact strings');
  assert(Number.isInteger(setIndex) && setIndex>=0 && Number.isFinite(scale) && scale>0,'Missing hitbox set or model scale');
  const compatibility=recordedContentFingerprint===null?'unknown':recordedContentFingerprint===model.contentFingerprint?'matched':'mismatch';
  const result={status:'unavailable',compatibility,eligibleForScoring:false,capsules:[],unavailable:[],scope:'Rendered bone geometry only; server hitbox timing is unverified'};
  if(resourceId!==model.resourceId || compatibility==='mismatch') return {...result,reason:'Model identity/content mismatch'};
  const set=model.hitboxSets[setIndex];
  if(!set) return {...result,reason:'Unknown hitbox set'};
  assert(typeof set.name==='string' && Array.isArray(set.hitboxes),'Invalid hitbox set');
  const ids=new Set();
  for(const box of set.hitboxes) {
    assert(Number.isInteger(box.index) && box.index>=0 && !ids.has(box.index),'Invalid/duplicate hitbox index');ids.add(box.index);
    assert(typeof box.bone==='string' && box.bone.length && vector(box.min) && vector(box.max),'Invalid hitbox geometry');
    if(box.shape!==2 || box.translationOnly!==false) {result.unavailable.push({index:box.index,reason:'Unsupported shape/translation-only transform'});continue;}
    assert(Number.isFinite(box.radius) && box.radius>0,'Invalid capsule radius');
    const bone=boneProvider(box.bone);
    if(!bone) {result.unavailable.push({index:box.index,reason:'No measured transform for bone'});continue;}
    // Capsule fields are endpoints; component-wise min/max sorting would change its axis.
    const start=bone.transformPoint(box.min),end=bone.transformPoint(box.max),radius=box.radius*scale;
    assert(vector(start) && vector(end) && Number.isFinite(radius),'Invalid world capsule');
    result.capsules.push({index:box.index,bone:box.bone,start,end,radius,center:start.map((v,i)=>(v+end[i])/2)});
  }
  return {...result,status:result.capsules.length?'diagnostic':'unavailable',setName:set.name};
}
