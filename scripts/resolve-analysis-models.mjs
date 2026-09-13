import {requireArtifact, artifact, fingerprint} from './analysis-contract.mjs';
// Resolve demo model IDs against explicit, local VPK listings; no guessed model names.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const run = promisify(execFile);

// MurmurHash64B, Austin Appleby's public-domain algorithm (smhasher/src/MurmurHash2.cpp).
// Source 2 resource seed: 0xEDABCDEF. Hash the lowercase, uncompiled resource path.
export function modelResourceId(compiledPath) {
  assert(typeof compiledPath === 'string' && /^[a-z0-9_./-]+\.vmdl_c$/i.test(compiledPath), 'Expected an ASCII compiled model path');
  assert(!compiledPath.startsWith('/') && !compiledPath.split('/').some(p => !p || p === '.' || p === '..'), 'Invalid model path');
  const bytes = Buffer.from(compiledPath.slice(0,-2).toLowerCase(), 'ascii');
  const multiply = n => Math.imul(n,0x5bd1e995) >>> 0;
  const mix = n => { n=multiply(n); return multiply(n ^ (n>>>24)); };
  let first=(0xedabcdef ^ bytes.length)>>>0, second=0, offset=0;
  while (offset+8 <= bytes.length) {
    first=(multiply(first)^mix(bytes.readUInt32LE(offset)))>>>0;
    second=(multiply(second)^mix(bytes.readUInt32LE(offset+4)))>>>0;
    offset+=8;
  }
  if (offset+4 <= bytes.length) { first=(multiply(first)^mix(bytes.readUInt32LE(offset)))>>>0; offset+=4; }
  if (offset < bytes.length) {
    let tail=0;
    for (let i=0; offset+i<bytes.length; i++) tail|=bytes[offset+i]<<(8*i);
    second=multiply(second^tail);
  }
  first=multiply(first^(second>>>18)); second=multiply(second^(first>>>22));
  first=multiply(first^(second>>>17)); second=multiply(second^(first>>>19));
  return ((BigInt(first)<<32n)|BigInt(second)).toString();
}

export function resolveModels(frames, listings) {
  const ids = new Set();
  for (const frame of frames) for (const entity of frame.entities) {
    for (const [name,id] of Object.entries(entity.properties)) if (name.endsWith('.m_hModel')) {
      assert(typeof id === 'string' && /^(0|[1-9][0-9]*)$/.test(id) && BigInt(id)<(1n<<64n), 'Model ID must be an exact unsigned 64-bit decimal string');
      if (id !== '0') ids.add(id);
    }
  }
  const models = [...ids].sort().map(resourceId => ({resourceId,candidates:[]}));
  const byId = new Map(models.map(model => [model.resourceId,model]));
  for (const {vpk,listing} of listings) for (const line of listing.split(/\r?\n/)) {
    const entry = /^(.*\.vmdl_c) CRC:([0-9a-f]+) size:([0-9]+)$/i.exec(line.trim());
    if (!entry) continue;
    const resourceId=modelResourceId(entry[1]);
    byId.get(resourceId)?.candidates.push({vpk,compiledPath:entry[1],crc:entry[2].toLowerCase(),byteLength:Number(entry[3])});
  }
  // Multiple candidates remain explicit: package priority/content version is a caller decision.
  return {assetVersionMatch:'unverified',models,unresolved:models.filter(model => !model.candidates.length).map(model => model.resourceId)};
}

async function main() {
  const [vrf,sceneFile,...packages]=process.argv.slice(2);
  assert(vrf && sceneFile && packages.length, 'Usage: node scripts/resolve-analysis-models.mjs VRF SCENE.json VPK [VPK...]');
  const sceneBytes=await readFile(sceneFile);
  const scene=JSON.parse(sceneBytes.toString('utf8').replace(/^\uFEFF/,''));
  const frames=requireArtifact(scene,'packet-scene');
  const listings=[];
  for (const vpk of packages) {
    const {stdout}=await run(vrf,['-i',vpk,'--vpk_list','-e','vmdl_c'],{windowsHide:true,maxBuffer:32*1024*1024});
    listings.push({vpk:path.resolve(vpk),listing:stdout});
  }
  console.log(JSON.stringify(artifact('model-resolution',resolveModels(frames,listings),{source:scene.source,dependencies:[fingerprint(sceneBytes),...listings.map(item=>fingerprint(JSON.stringify(item)))]}),null,2));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
