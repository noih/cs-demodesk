// Composition root: wire the physics adapter to scene transforms here only.
import {requireArtifact, artifact, fingerprint} from './analysis-contract.mjs';
import {trace,loadPhysics,PHYSICS_VERSION} from './analysis-physics.mjs';
import {traceScene,DYNAMIC_VERSION} from './analysis-scene-geometry.mjs';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

async function main() {
  const [filename,queryFile] = process.argv.slice(2);
  assert(filename && queryFile,'Usage: node scripts/trace-analysis-geometry.mjs PHYSICS.gltf|SCENE.json QUERY.json');
  const query = JSON.parse((await readFile(queryFile,'utf8')).replace(/^\uFEFF/,''));
  if (query.modelFiles) {
    const sceneBytes=await readFile(filename);
    const scene=JSON.parse(sceneBytes.toString('utf8').replace(/^\uFEFF/,''));
    const frames=requireArtifact(scene,'packet-scene');
    const matches=frames.filter(frame=>frame.tick===query.tick);
    assert(matches.length===1,'Requested tick is absent or ambiguous; no nearest-tick fallback');
    const files=Object.fromEntries(Object.entries(query.modelFiles).map(([id,file])=>[id,path.resolve(path.dirname(queryFile),file)]));
    const dependencies=[fingerprint(sceneBytes),fingerprint(PHYSICS_VERSION),fingerprint(JSON.stringify(query))];
    const provider=async file=>{
      const physics=await loadPhysics(file);
      dependencies.push(fingerprint(await readFile(file)),fingerprint(physics.bytes));
      return {trace:(start,end)=>trace(physics.gltf,physics.bytes,start,end)};
    };
    const data=await traceScene(matches[0],files,query.start,query.end,provider);
    console.log(JSON.stringify(artifact('dynamic-collision',data,{source:scene.source,implementationVersion:DYNAMIC_VERSION,dependencies:[...new Set(dependencies)].sort()}),null,2));
  } else {
    const {gltf,bytes}=await loadPhysics(filename);
    console.log(JSON.stringify(artifact('static-collision',trace(gltf,bytes,query.start,query.end),{implementationVersion:PHYSICS_VERSION,dependencies:[fingerprint(await readFile(filename)),fingerprint(bytes),fingerprint(JSON.stringify(query))]}),null,2));
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
