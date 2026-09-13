// Shared wire boundary; data producers have no scoring imports.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
export const fingerprint = bytes => 'sha256:'+createHash('sha256').update(bytes).digest('hex');
export function requireArtifact(value, module, schemaVersion=1) {
  assert(value?.contract?.module===module, 'Unexpected analysis module');
  assert(value.contract.schemaVersion===schemaVersion, 'Unsupported analysis schema');
  assert(typeof value.contract.implementationVersion==='string' && value.contract.implementationVersion.length, 'Missing implementation version');
  assert(value.source && ['demoFingerprint','gameBuild','mapContentFingerprint'].every(key=>value.source[key]===null || typeof value.source[key]==='string'), 'Missing source identity; unknown versions must be explicit null');
  assert(Array.isArray(value.dependencies) && value.dependencies.every(id=>typeof id==='string' && /^(sha1:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/.test(id)), 'Invalid dependency fingerprints');
  assert(Object.hasOwn(value,'data'),'Missing artifact payload');
  return value.data;
}
export function artifact(module, data, {source={demoFingerprint:null,gameBuild:null,mapContentFingerprint:null},dependencies=[],implementationVersion='0.1.0'}={}) {
  const result={contract:{module,schemaVersion:1,implementationVersion},source,dependencies,data};
  requireArtifact(result,module);
  return result;
}
