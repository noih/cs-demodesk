import assert from 'node:assert/strict';
import { packReplay, unpackReplay } from '../src/replay/transfer.ts';
import { createServer } from 'vite';
import { createRequire } from 'node:module';
import { createReadStream } from 'node:fs';
import { readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
const root = process.argv[2];
if (!root) throw new Error('Usage: node --experimental-transform-types scripts/benchmark-replay-load.mjs <data-directory>');
const files = await Promise.all((await readdir(path.join(root, 'parsed'))).filter(n => n.endsWith('.replay.json')).map(async name => ({name, bytes:(await stat(path.join(root,'parsed',name))).size})));
const largest = files.sort((a,b)=>b.bytes-a.bytes)[0];
if (!largest) throw new Error('No cached replay');
{
 const original=JSON.parse(await readFile(path.join(root,'parsed',largest.name),'utf8'));
 const packed=packReplay(original);
 const transferred=structuredClone(packed,{transfer:[packed.rows.buffer]});
 assert.deepEqual(unpackReplay(transferred),original);
 console.log('Full cached replay round-trip equality passed');
}
const parsedPath=path.join(root,'parsed',largest.name.replace('.replay.json','.json'));
const parsed=JSON.parse(await readFile(parsedPath,'utf8'));
const radarDir=path.join(root,'radar',parsed.info.mapName);
const assets=new Map([['/bench/replay',path.join(root,'parsed',largest.name)],['/bench/parsed',parsedPath]]);
const images=(await readdir(radarDir)).filter(n=>n.endsWith('.png'));
images.forEach((n,i)=>assets.set('/bench/radar/'+i,path.join(radarDir,n)));
const server=await createServer({server:{host:'127.0.0.1',port:0},plugins:[{name:'local-replay-benchmark',configureServer(s){s.middlewares.use((req,res,next)=>{
 if(req.url==='/bench'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>Local replay benchmark</title>');return;}
 const file=assets.get(req.url);if(!file)return next();
 res.setHeader('Cache-Control','no-store');createReadStream(file).pipe(res);
});}}]});
await server.listen();
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({channel:'chrome',headless:true});
const results=[];
try {
 console.log(JSON.stringify({replayMiB:largest.bytes/1048576,rounds:parsed.rounds.length,radarLayers:images.length}));
 for (const mode of ['object', 'packed', 'actual']) for(let run=0;run<4;run++) {
 const page=await browser.newPage();await page.goto(server.resolvedUrls.local[0]+'bench');
 const result=await page.evaluate(async ({layers,mode})=>{
  const {Replay}=await import('/src/replay/engine.ts');
  const {unpackReplay}=await import('/src/replay/transfer.ts');
  const {loadRadarImages}=await import('/src/replay/radar.ts');
  const parsed=await (await fetch('/bench/parsed')).json();
  if(mode==='actual') {
    window.__TAURI_INTERNALS__={invoke:async()=>({path:'/bench/replay',bytes:0}),convertFileSrc:()=>location.origin+'/bench/replay'};
    const {api}=await import('/src/api.ts');
    const start=performance.now();const data=await api.replay('benchmark');const received=performance.now();
    const replay=new Replay(data,parsed,parsed.kills);replay.stateAt(parsed.rounds[0].freezeEndTick);const initialized=performance.now();
    const images=await loadRadarImages(Array.from({length:layers},(_,i)=>location.origin+'/bench/radar/'+i),new AbortController().signal);
    const ready=performance.now();images.forEach(i=>i.close());
    return {loadMs:received-start,replayMs:initialized-received,radarMs:ready-initialized,totalMs:ready-start,frames:data.frames.length};
  }

  const code=`import {packReplay} from '${location.origin}/src/replay/transfer.ts'; self.onmessage=async()=>{const start=performance.now();const response=await fetch('${location.origin}/bench/replay');const text=await response.text();const read=performance.now();const data=JSON.parse(text);const parsed=performance.now();const packed='${mode}'==='packed'?packReplay(data):null;const packedAt=performance.now();self.postMessage({data:packed?null:data,packed,readMs:read-start,parseMs:parsed-read,packMs:packedAt-parsed,sentAt:performance.timeOrigin+performance.now()},packed?[packed.rows.buffer]:[]);};`;
  const url=URL.createObjectURL(new Blob([code],{type:'text/javascript'}));
  const worker=new Worker(url,{type:'module'});const start=performance.now();
  const loaded=await new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(new Error('Replay load timed out')),60000);worker.onmessage=e=>{clearTimeout(timeout);resolve(e.data);};worker.onerror=reject;worker.postMessage(null);});
  const received=performance.now();const transferMs=performance.timeOrigin+received-loaded.sentAt;
  worker.terminate();URL.revokeObjectURL(url);
  const data=loaded.packed?unpackReplay(loaded.packed):loaded.data;
  const unpacked=performance.now();
  const replay=new Replay(data,parsed,parsed.kills);replay.stateAt(parsed.rounds[0].freezeEndTick);
  const initialized=performance.now();
  const images=await loadRadarImages(Array.from({length:layers},(_,i)=>location.origin+'/bench/radar/'+i),new AbortController().signal);
  const ready=performance.now();images.forEach(i=>i.close());
  return {readMs:loaded.readMs,parseMs:loaded.parseMs,transferMs,packMs:loaded.packMs,unpackMs:unpacked-received,replayMs:initialized-unpacked,radarMs:ready-initialized,totalMs:ready-start,frames:data.frames.length};
 },{layers:images.length,mode});
 results.push({mode,run,...result});console.log(JSON.stringify({mode,run,...result}));await page.close();
 }
 const median = mode => results.filter(r=>r.mode===mode && r.run>0).map(r=>r.totalMs).sort((a,b)=>a-b)[1];
 console.log(JSON.stringify({medianBaselineMs:median('object'),medianActualMs:median('actual'),reductionPercent:100*(1-median('actual')/median('object'))}));
} finally {await browser.close();await server.close();}
