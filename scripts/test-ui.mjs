import { checkReplayDrawing } from './check-replay-drawing.mjs';
import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { preview } from 'vite';
import { initialAppearance, THEMES } from '../src/themes.ts';
import { projectReference } from '../src/recoil.ts';
const calibration = JSON.parse(await readFile(new URL('../src/data/recoil-reference.json', import.meta.url), 'utf8'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
assert.equal(initialAppearance('light', true), 'light');
assert.equal(initialAppearance('invalid', true), 'dark');
assert.equal(initialAppearance(null, false), 'light');
assert.equal(Object.keys(THEMES.dark).join(), Object.keys(THEMES.light).join());
const server = await preview({ preview: { port: 0, host: '127.0.0.1' } });
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1360, height: 940 } });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('demodesk.appearance', 'dark');
    const demos = Array.from({ length: 1000 }, (_, i) => ({ id: 'demo-' + i, name: 'match_' + i + '.dem', path: 'E:/replays/' + i + '.dem', bytes: 214800000, createdMs: new Date(2026, 8, 8, 17, 0).getTime() - i * 60000, mtimeMs: new Date(2026, 8, 8, 17, i % 60).getTime(), status: 'parsed', mapName: i % 2 ? 'Inferno' : 'Mirage', summary: { rounds: 22, kills: 100, highlights: 1, scoreA: 13, scoreB: 9, players: ['Player'] } }));
    const options = { width: 1920, height: 1080, fps: 60, codec: 'h264', hud: true, crosshair: true, radar: true, killFeed: true, viewmodel: true, tracers: true, maxSizeMb: 20, trueView: true };
    const jobTime = Date.now();
    const jobs = Array.from({ length: 100 }, (_, i) => ({ id: 'job-' + i, demoId: i === 1 ? 'demo-999' : 'demo-0', highlightIds: ['highlight-1'], options, status: i === 0 ? 'running' : i === 1 ? 'queued' : 'done', stage: i === 0 ? 'recording 1/2' : '', progress: i === 0 ? 0.35 : undefined, createdAt: new Date(jobTime - 45000).toISOString(), startedAt: i === 1 ? undefined : new Date(jobTime - 40000).toISOString(), finishedAt: i > 1 ? new Date(jobTime - 10000).toISOString() : undefined, outputs: i < 2 ? [] : Array.from({length:i===2?3:1},(_,j)=>({ file: 'E:/clips/' + i + '-' + j + '.mp4', bytes: 18400000, title: 'Round 08', highlightId: 'highlight-1', isFinal: j===2 })), errorCode:i===2?'app-closed':undefined,error:i===2?'backend diagnostic changed':undefined,log: ['recording'] }));
    const testPlayerNames = JSON.parse(localStorage.getItem('test.playerNames') || 'null');
    const player = {steamid:'1',name:testPlayerNames?.[0] ?? 'Player',team:'A',kills:20,deaths:10,assists:3,openingKills:4,openingDeaths:2,flashAssists:1,roundsPlayed:22,roundsSurvived:12,kast:77.3,tradeKills:2,tradedDeaths:1,heDamage:20,fireDamage:10,opponents:{'2':3},aim:{all:{shots:100,hits:25,headHits:5,headEligibleHits:20,firstShots:10,firstHits:4,sprayShots:30,sprayHits:9}},activity:{shots:100,flashes:2,smokes:3,hes:2,fires:1,enemiesFlashed:3,teammatesFlashed:1,enemyBlindSeconds:7},clutches:[{round:8,side:'CT',versus:2,kills:2,outcome:'won'}],headshots:10,headshotPct:50,kd:2,multiKills:{'2k':2,'3k':1,'4k':0,'5k':0},clutchesWon:1,damage:2000,utilityDamage:30,friendlyDamage:0,adr:90,highlights:1,bestScore:8};
    const ray = ([x,y], i) => ({ tick: 100 + i * 6, origin: [0,0,64], viewYaw: Math.atan2(x,1000) * 180 / Math.PI, viewPitch: Math.atan2(y,Math.hypot(1000,x)) * 180 / Math.PI });
    player.recoil = {ak47:[
      {round:1,startTick:100,shots:[[0,0],[-40,40],[0,80],[40,100]].map(ray)},
      {round:2,startTick:200,shots:[[0,0],[4000,40],[40,80]].map((p,i)=>({...ray(p,i),tick:200+i*6,origin:[i*20,-i*10,64-i*5]}))},
    ]};
    const status = { missingRenderTools:[],ok:true,problems:[],dataDir:'E:/data',activeRender:'job-0',version:'test' };
    window.testCalls = [];
    window.scoreHistory=JSON.parse(localStorage.getItem("test.scoreHistory") || "null");
    window.analysisJobs=[];
    window.analysisStep=(job,step)=>{Object.assign(job,{step,revision:job.revision+1});window.emitTestEvent({type:'analysis-job-changed',job:{...job}});};
    const callbacks = new Map();
    const listeners = new Map();
    let callbackId = 0;
    window.testListenerCount = () => listeners.size;
    window.emitTestEvent = payload => { for (const [id, handler] of listeners) callbacks.get(handler)?.({event:'demodesk://event',id,payload}); };
    window.__TAURI_INTERNALS__ = { transformCallback: fn => { callbacks.set(++callbackId, fn); return callbackId; }, unregisterCallback: id => callbacks.delete(id), convertFileSrc: () => 'data:video/mp4;base64,', invoke: async (cmd, args) => {
      window.testCalls.push({cmd,args});
      if(cmd==='get_startup_error')return null;
      if(cmd==='get_map_assets' && window.missingTools)throw new Error('Source 2 Viewer CLI not installed');
      if(cmd==='get_replay' || cmd==='get_map_assets')return new Promise(()=>{});
      if(cmd==='get_kills')return [];
      if(cmd==='parse_demo')return new Promise(resolve=>window.releaseParse=resolve);
      if(cmd==='browse_directory')return args.path ? 'E:/tools' : 'E:/Desktop';
      if(cmd==='plugin:dialog|open')return null;
      if(cmd==='run_setup')return true;
      if(cmd==='check_for_updates'){if(window.holdUpdate)await new Promise(resolve=>window.releaseUpdate=resolve);if(window.failUpdate)throw new Error('offline');return window.updateStatus ?? {status:'available',version:'1.0.10'};}
      if(cmd==='open_url'){window.openedUrl=args.url;return;}
      if(cmd==='open_path'){window.openedPath=args.path;return;}
      if(cmd==='get_status')return window.missingTools ? {...status,ok:false,missingRenderTools:window.missingRenderTools ?? ['HLAE','ffmpeg']} : status;
      if(cmd==='get_settings')return { settings:{language:'en',replayFolders:[],scanGameReplays:true},doctor:{ok:true,problems:[],paths:window.toolPaths || {}},detected:{},setup:{running:false,log:[]},dataDir:'E:/data',defaultDataDir:'E:/data',parsedBytes:0,anomalyBytes:0,clipsBytes:0,radarBytes:0 };
      if(cmd==='list_demos'){if(window.holdRefresh)await new Promise(resolve=>window.releaseRefresh=resolve);return demos;}
      if(cmd==='delete_job') { window.queueJobs=(window.queueJobs ?? jobs).filter(job=>job.id!==args.id); return; }
      if(cmd==='list_jobs')return window.queueJobs ?? (window.showQueuedOnCurrentDemo ? jobs.map(j=>j.status==='queued'?{...j,demoId:'demo-0'}:j) : jobs);
      if(cmd==='analysis_clips'&&window.holdClipPreview)await new Promise(resolve=>(window.releaseClipPreviews??=[]).push(resolve));
      if(cmd==='analysis_clips')return args.selection.ruleIds.map(ruleId=>({ruleId,title:'Opponent — '+ruleId,demoFingerprint:'source-current',highlights:[{id:'clip-'+ruleId,player:{steamid:args.selection.playerId,name:'Opponent'},round:1,startTick:0,endTick:320,anchorTick:64,score:0,tags:[ruleId],title:'Opponent — '+ruleId,kills:[],breakdown:{}}]}));
      if(cmd==='start_analysis_render'){
        if(window.failExport)throw Error('Cannot queue export');
        const exports=args.selection.ruleIds.map((ruleId,index)=>({id:'analysis-video-'+index,demoId:args.demoId,highlightIds:['clip-'+ruleId],analysisClips:{ruleId,title:'Opponent — '+ruleId,demoFingerprint:'source-current',highlights:[{id:'clip-'+ruleId,title:'Opponent — '+ruleId}]},options:{...args.options,merge:true},status:'queued',createdAt:new Date().toISOString(),outputs:[],log:[]}));
        window.queueJobs=[...(window.queueJobs??jobs),...exports];
        exports.forEach(job=>window.emitTestEvent({type:'job-changed',job}));return exports;
      }
      if(cmd==='analysis_jobs')return window.analysisJobs.map(job=>({...job}));
      if(cmd==='clear_match_anomaly') { for(const key of Object.keys(window.scoreHistory ?? {})) if(key.startsWith(args.id+':')) delete window.scoreHistory[key]; window.clearMatchCalls=(window.clearMatchCalls ?? 0)+1; return; }
      if(cmd==='scoring_history')return Object.fromEntries(Object.entries(window.scoreHistory??{}).filter(([key])=>key.startsWith(args.id+':')).map(([key,records])=>[key.slice(args.id.length+1),records]));
      if(cmd==='score_player')throw Error('Per-player scoring must not be invoked');
      if(cmd==='score_match') {
        window.scoreCalls=(window.scoreCalls ?? 0)+1;
        if(window.failScore)throw Error('Source demo unavailable');
        const active=window.analysisJobs.find(job=>job.demoId===args.id&&['running','queued'].includes(job.status));
        if(active)return {...active};
        const job={id:'analysis-'+window.scoreCalls,demoId:args.id,sequence:window.scoreCalls,revision:0,status:'queued',step:null,error:null,createdAt:new Date().toISOString(),startedAt:null,finishedAt:null};
        window.analysisJobs.push(job);
        const emit=()=>window.emitTestEvent({type:'analysis-job-changed',job:{...job}});
        emit();
        const complete=()=>{
        job.status='running';job.startedAt=new Date().toISOString();window.analysisStep(job,1);
        window.releaseScore=()=>{
        if(window.failWorker){Object.assign(job,{status:'error',error:'Cannot read source demo',finishedAt:new Date().toISOString(),revision:job.revision+1});emit();return;}

        const history=window.scoreHistory ??= {};
        const players={};
        for(const playerId of ['1','2']) {
          const key=args.id+':'+playerId;
          const records=history[key] ??= [];
          const index=records[0] ? Number(records[0].id.split('-').at(-1))+1 : 0;
          const occurrence={id:'event-'+playerId,round:1,startTick:64+index*64,endTick:128+index*64,targetId:'1',sourceIds:['raw-a','raw-b'],measurements:[{name:'speed',value:3.123456,unit:'deg/s',threshold:null},{name:'speed',value:4.123456,unit:'deg/s',threshold:null}]};
          const makeCheck = (id,name) => ({definition:{id,version:'test-2',name,description:'Test behavior',category:'aim',parameters:{}},state:playerId==='1'?'passed':'findings',reason:'Observed behavior',reasonCode:'experimentalMeasurements',evaluatedSamples:64,summary:[{name:'shots',value:64,unit:'',threshold:null},{name:'hitRate',value:0.875,unit:'ratio',threshold:null}],occurrences:playerId==='1'?[]:[occurrence],diagnostics:null});
          const contextCheck = (id,metric) => ({...makeCheck(id,id),state:playerId==='1'?'unavailable':'findings',reasonCode:'shotPathsMissing',summary:playerId==='1'?[]:[{name:metric,value:1,unit:'shots',threshold:null},{name:'unclassifiedShots',value:63,unit:'shots',threshold:null}],occurrences:playerId==='1'?[]:[{...occurrence,id:'context-'+id,measurements:[{name:metric,value:1,unit:'shots',threshold:null}]}]});
          records.length=0;
          records.unshift({schemaVersion:2,id:'assessment-'+index,createdAt:new Date(1750000000000+index*1000).toISOString(),demoId:args.id,demoFingerprint:'source-current',playerId,tickRate:64,rulesetVersion:'test-2',state:playerId==='1'?'passed':'findings',checks:[{...contextCheck('smoke-hit-rate','smokeHits'),state:playerId==='1'?'passed':'findings',summary:[{name:'smokeHitRate',value:playerId==='1'?0:28,unit:'percent'},{name:'estimatedSmokeHits',value:playerId==='1'?0:14,unit:'shots'},{name:'smokeShots',value:50,unit:'shots'}]},contextCheck('penetration-hit-rate','penetrationHits'),makeCheck('aim-snap','Instant acquisition'),makeCheck('aim-linear-acquisition','Rapid straight turn'),{...makeCheck('globally-empty','Empty behavior'),state:'passed',occurrences:[],summary:[]},{definition:{id:'view-angle-oscillation',version:'test-2',name:'View-angle oscillation',description:'View behavior',category:'view',parameters:{}},state:'unavailable',reason:'Missing view data',reasonCode:'viewMissing',evaluatedSamples:0,summary:[],occurrences:[],diagnostics:null}]});
          players[playerId]=records;
        }
        Object.assign(job,{status:'done',step:3,finishedAt:new Date().toISOString(),revision:job.revision+1});emit();
        };
        if(!window.holdScore)window.releaseScore();
        };
        window.startScore=complete;
        if(!window.holdQueued)complete();
        return {...job};
      }
      if(cmd==='get_demo' && window.emptyParsed)return {meta:demos.find(d=>d.id===args.id)};
      if(cmd==='get_demo' && window.failDemo)throw Error('Cannot read demo');
      if(cmd==='get_demo' && window.holdDemo)await new Promise(resolve=>window.releaseDemo=resolve);
      if(cmd==='get_demo')return { meta:demos.find(d=>d.id===args.id),parsed:{recoilReference:{ak47:[{x:0,y:0,samples:4},{x:-1,y:-1,samples:4},{x:-2,y:-3,samples:4},{x:-2,y:-4,samples:2}]},info:{mapName:demos.find(d=>d.id===args.id).mapName,tickRate:64,players:[{steamid:'1',name:'Player',teamNumber:3},{steamid:'2',name:'Opponent',teamNumber:2}]},parsedAt:'2026-09-08',score:{A:13,B:9},rounds:[],roundSummaries:[{round:1,winner:'A',killsA:5,killsB:2,players:{'1':{kills:5,deaths:2,damage:450,awp:1,flashed:2,cash:800},'2':{kills:2,deaths:5,damage:220,awp:0,flashed:0,cash:300}}}],stats:[player,{...player,steamid:'2',name:testPlayerNames?.[1] ?? 'Player',team:'B',recoil:{},opponents:{'1':1}}],highlights:[{id:'highlight-1',player:{steamid:'1',name:'Player'},round:8,startTick:640,endTick:1280,score:8,tags:['3k'],title:'Player — 3 kills · R8',kills:[],breakdown:{}}]}};
      if(cmd==='plugin:event|listen') { if(window.holdListener)await new Promise(resolve => window.releaseListener = resolve); await new Promise(resolve => setTimeout(resolve, 10)); listeners.set(args.handler, args.handler); return args.handler; }
      if(cmd==='plugin:event|unlisten') { listeners.delete(args.eventId); return; }
      throw Error('Unexpected command '+cmd);
    }};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener:()=>{} };
  });
  await page.goto(server.resolvedUrls.local[0]);
  await page.locator('.demo-item').first().waitFor();
  const workerFile = (await readdir(new URL('../dist/assets/', import.meta.url))).find(name => name.startsWith('load.worker-') && name.endsWith('.js'));
  assert.ok(workerFile, 'Replay JSON worker is emitted');
  const workerResult = await page.evaluate(async workerUrl => {
    const read = url => new Promise((resolve, reject) => {
      const worker = new Worker(workerUrl, {type:'module'});
      worker.onmessage = e => { worker.terminate(); resolve(e.data); };
      worker.onerror = e => { worker.terminate(); reject(new Error(e.message)); };
      worker.postMessage(url);
    });
    const payload = {frames:Array.from({length:50000}, (_,i)=>({t:i,p:[[1,2,3,4,5,6,7,8,9,10]]}))};
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload)], {type:'application/json'}));
    let paints = 0;
    let running = true;
    const paint = () => { if (running) { paints++; requestAnimationFrame(paint); } };
    requestAnimationFrame(paint);
    try {
      const result = await read(url);
      const invalid = await read('data:application/json,not-json');
      const missing = await read('http://127.0.0.1:1/unavailable');
      return {frames:result.data.frames.length,last:result.data.frames.at(-1).t,paints,invalid:typeof invalid.error,missing:typeof missing.error};
    } finally { running = false; URL.revokeObjectURL(url); }
  }, '/assets/'+workerFile);
  assert.equal(workerResult.frames,50000);
  assert.equal(workerResult.last,49999);
  assert.ok(workerResult.paints > 0, 'UI paints while replay worker loads');
  assert.equal(workerResult.invalid,'string');
  assert.equal(workerResult.missing,'string');
  await page.evaluate(() => document.fonts.ready);
  assert.ok(await page.evaluate(() => document.fonts.check('15px bootstrap-icons')), 'Bootstrap icon font is loaded');
  assert.ok(await page.locator('.bi-funnel').count(), 'Filter uses Bootstrap Icons');
  assert.ok(await page.locator('.demo-item').count() < 40, 'Demo DOM bounded by viewport');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='get_demo').length),0,'No background demo parsing reads');
  await page.evaluate(()=>window.holdDemo=true);
  await page.locator('.demo-item').first().click();
  await page.locator('.main [aria-busy="true"]').waitFor();
  assert.equal(await page.locator('.demo-heading').count(), 0, 'No unparsed header while loading');
  assert.equal(await page.locator('.main button').count(), 0, 'No premature parse action while loading');
  await page.waitForFunction(()=>typeof window.releaseDemo==='function');
  await page.evaluate(()=>{window.holdDemo=false;window.releaseDemo()});
  await page.locator('.demo-tabs').waitFor();
  assert.equal(await page.getByRole('tab').first().innerText(),'Players');
  assert.equal(await page.getByRole('tab').first().getAttribute('data-state'),'active','Opening a demo selects player statistics');
  assert.equal(await page.locator('.main [aria-busy="true"]').count(), 0, 'Loading ends after demo data arrives');
  await page.getByRole('tab').filter({hasText:'Videos'}).click();
  const videoTab = page.getByRole('tab').filter({hasText:'Videos'});
  assert.equal(await videoTab.locator('.rt-TabsTriggerInner .app-spinner').count(), 1);
  assert.doesNotMatch(await videoTab.innerText(), /running/);
  assert.ok(await videoTab.evaluate(el => {
    const outer = el.getBoundingClientRect();
    return [...el.querySelector('.rt-TabsTriggerInner').children].every(child => {
      const rect = child.getBoundingClientRect();
      return rect.left >= outer.left && rect.right <= outer.right;
    });
  }), 'Video count and activity indicator stay inside the tab');
  await page.evaluate(()=>{window.showQueuedOnCurrentDemo=true;});
  await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
  await videoTab.locator('.rt-TabsTriggerInner .bi-hourglass-split').waitFor();
  assert.equal(await videoTab.locator('.rt-TabsTriggerInner .app-spinner').count(),1);
  const viewport=page.viewportSize();
  await page.setViewportSize({width:900,height:940});
  await videoTab.scrollIntoViewIfNeeded();
  assert.ok(await videoTab.evaluate(el=>{const outer=el.getBoundingClientRect();return [...el.querySelector('.rt-TabsTriggerInner').children].filter(child=>child.getBoundingClientRect().width>0).every(child=>{const r=child.getBoundingClientRect();return r.left>=outer.left && r.right<=outer.right;});}), 'Visible icon fits the compact video tab');
  await page.setViewportSize(viewport);
  await page.evaluate(()=>{window.showQueuedOnCurrentDemo=false;});
  await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
  await videoTab.locator('.rt-TabsTriggerInner .bi-hourglass-split').waitFor({state:'detached'});
  await page.locator('.notification-viewport').getByRole('button',{name:'Close',exact:true}).click();
  await page.locator('.job-card').first().waitFor();
  await page.getByText('Job interrupted because the app was closed',{exact:true}).waitFor();
  assert.equal(await page.getByText('backend diagnostic changed',{exact:true}).count(),0,'Error code selects translation regardless of backend diagnostic wording');
  const progressBar = page.getByRole('progressbar', {name:'Overall progress (estimated)'});
  assert.equal(await progressBar.getAttribute('aria-valuenow'), '35');
  assert.ok(!(await page.locator('.job-card').first().innerText()).includes('Elapsed'), 'Running timer has no redundant prefix');
  const elapsedBefore = await page.locator('.job-card').first().innerText();
  await page.waitForTimeout(1100);
  assert.notEqual(await page.locator('.job-card').first().innerText(), elapsedBefore, 'Elapsed timer advances without progress events');
  await page.evaluate(async () => {
    const jobs = await window.__TAURI_INTERNALS__.invoke('list_jobs');
    window.emitTestEvent({type:'job-changed', job:{...jobs[0],stage:'encoding 2/2: fitting',progress:0.82}});
  });
  await page.waitForFunction(() => document.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow') === '82');
  assert.ok((await page.locator('.job-card').first().innerText()).includes('Encoding 2/2 · Fitting file size'));
  assert.ok(await page.locator('.job-card').count()<15,'Job DOM bounded by viewport');
  // Width changes invalidate off-screen heights; mounted cards must stay measured
  // even when progress updates do not change their physical height.
  for (const width of [1200, 1360]) {
    await page.setViewportSize({width, height:940});
    await page.waitForTimeout(100);
    const overlaps = await page.evaluate(async () => {
      const [job] = await window.__TAURI_INTERNALS__.invoke('list_jobs');
      const failures = [];
      for (let step = 0; step < 12; step++) {
        window.emitTestEvent({type:'job-changed',job:{...job,progress:0.4+step/100}});
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const cards = [...document.querySelectorAll('.job-card')].map(el => el.getBoundingClientRect());
        for (let i=1;i<cards.length;i++) {
          if (cards[i].top < cards[i-1].bottom + 10) failures.push({step,gap:cards[i].top-cards[i-1].bottom});
        }
      }
      return failures;
    });
    assert.deepEqual(overlaps, [], 'Progress updates preserve measured card heights after resizing');
  }

  const reparseButton = page.locator('.demo-heading button').filter({ has: page.locator('.bi-arrow-clockwise') });
  await reparseButton.click();
  const pendingParse = page.locator('.demo-heading button[aria-busy="true"]');
  await pendingParse.locator('.app-spinner').waitFor();
  assert.ok(await pendingParse.isDisabled(), 'Reparse stays visible and blocks repeat clicks');
  await page.waitForFunction(()=>typeof window.releaseParse==='function');
  await page.evaluate(()=>window.releaseParse());
  await pendingParse.waitFor({state:'detached'});
  for (const [name, expected] of [['Small',[20,18,16,14]],['Large',[24,22,20,18]],['Medium',[22,20,18,16]]]) {
    await page.getByRole('button',{name:'Text size',exact:true}).click();
    await page.getByRole('radiogroup',{name:'Text size'}).getByRole('radio',{name,exact:true}).click();
    await page.getByRole('radiogroup',{name:'Text size'}).waitFor({state:'detached'});
    const sizes = await page.locator('.app-root').evaluate(el => {
      const css = getComputedStyle(el);
      return ['title','subtitle','body','caption'].map(role=>parseInt(css.getPropertyValue('--app-font-'+role)));
    });
    assert.deepEqual(sizes,expected);
    assert.equal(await page.locator('.demo-heading .rt-Heading').evaluate(el=>parseFloat(getComputedStyle(el).fontSize)),expected[0]);
    assert.equal(await page.locator('.demo-heading .rt-Text').first().evaluate(el=>parseFloat(getComputedStyle(el).fontSize)),expected[3]);

    assert.equal(await page.evaluate(()=>localStorage.getItem('demodesk.fontSize')),name.toLowerCase());
    assert.ok(await page.locator('.demo-item').first().evaluate(el=>el.scrollHeight<=el.clientHeight),'Demo row accommodates text');
  }
  await page.keyboard.press('Escape');
  assert.ok(await page.getByRole('tab').evaluateAll(tabs=>tabs.every(tab=>{
    const style=getComputedStyle(tab);
    return style.flexGrow==='1' && tab.scrollWidth<=tab.clientWidth+1;
  })), 'Tabs adapt to font size and share available space without clipping');
  const plainCursors = await page.locator('button:not(:disabled):not([data-disabled]):not([aria-disabled="true"])').evaluateAll(buttons => buttons
    .filter(button => getComputedStyle(button).cursor !== 'pointer')
    .map(button => button.getAttribute('aria-label') || button.textContent.trim()));
  assert.deepEqual(plainCursors, [], 'Enabled buttons and tabs indicate clickability');

  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-dark.png'});
  await page.getByRole('button',{name:'Switch to light mode'}).click();
  assert.match(await page.locator('.radix-themes').first().getAttribute('class'),/light/);
  assert.equal(await page.evaluate(()=>localStorage.getItem('demodesk.appearance')), 'light');
  await videoTab.click();
  assert.equal(await videoTab.locator('.rt-TabsTriggerInner .app-spinner').count(), 1);
  await page.waitForTimeout(200);
  assert.equal(await videoTab.evaluate(el=>getComputedStyle(el).backgroundColor), 'rgb(236, 239, 240)', 'Running videos retain the light active tab background');
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-light.png'});
  const about = page.getByRole('button', { name: 'About: Version 1.0.10 available', exact: true });
  assert.equal(await about.innerText(), '', 'About is an icon-only button');
  const aboutBounds = await about.boundingBox();
  const settingsBounds = await page.getByRole('button', { name: 'Settings', exact: true }).boundingBox();
  assert.ok(settingsBounds.x + settingsBounds.width + 7 <= aboutBounds.x, 'About and Settings hit areas stay separated');
  await page.evaluate(() => window.holdUpdate = true);
  await about.click();
  await page.getByRole('dialog').waitFor();
  const checkingAbout = page.getByRole('button', {name:'About: Checking for updates…',exact:true,includeHidden:true});
  await checkingAbout.locator('.app-spinner').waitFor();
  assert.equal(await checkingAbout.locator('.app-spinner').evaluate(el=>getComputedStyle(el).animationName), 'app-spinner-rotate');
  const checkingNotice = page.getByRole('dialog').getByRole('status').filter({hasText:'Checking for updates…'});
  await checkingNotice.waitFor();
  assert.equal(await checkingNotice.locator('.app-spinner').count(), 1);
  await page.evaluate(() => { window.holdUpdate = false; window.releaseUpdate(); });
  await page.getByRole('button', {name:'GitHub: Version 1.0.10 available',exact:true}).waitFor();
  await checkingNotice.waitFor({state:'detached'});
  const githubLink = page.getByRole('dialog').getByRole('button', {name:/^GitHub/});
  assert.match(await githubLink.innerText(), /v1\.0\.10/, 'GitHub link displays the available version');
  assert.equal(await githubLink.getAttribute('data-accent-color'), 'green', 'GitHub link highlights an available update');
  const storeLink = page.getByRole('dialog').getByRole('button', {name:'Microsoft Store',exact:true});
  assert.equal(await storeLink.getAttribute('data-accent-color'), 'gray', 'GitHub updates do not highlight Microsoft Store');
  const sourceBoxes = await page.getByRole('dialog').evaluate(el => {
    const rect = name => { const r = [...el.querySelectorAll('button')].find(b => b.textContent.trim().startsWith(name)).getBoundingClientRect(); return { y:r.y, height:r.height }; };
    return {github:rect('GitHub'),store:rect('Microsoft Store')};
  });
  assert.equal(sourceBoxes.github.y, sourceBoxes.store.y, 'Distribution links share a row');
  assert.equal(sourceBoxes.github.height, sourceBoxes.store.height, 'Distribution links have equal prominence');
  await storeLink.click();
  assert.equal(await page.evaluate(() => window.openedUrl), 'https://apps.microsoft.com/detail/9N5G4VXSDGS5');
  await page.getByRole('button', {name:'GitHub: Version 1.0.10 available',exact:true}).click();
  assert.equal(await page.evaluate(() => window.openedUrl), 'https://github.com/noih/cs-demodesk/releases/latest');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  const updateCount = () => page.evaluate(() => window.testCalls.filter(c => c.cmd === 'check_for_updates').length);
  const initialChecks = await updateCount();
  await page.evaluate(() => { window.failUpdate = true; window.holdUpdate = true; });
  await about.click();
  await checkingNotice.waitFor();
  await page.evaluate(() => { window.holdUpdate = false; window.releaseUpdate(); });
  await page.getByText('Unable to check for updates. Reopen About to try again.', {exact:true}).waitFor();
  await checkingNotice.waitFor({state:'detached'});
  await page.waitForFunction(n => window.testCalls.filter(c => c.cmd === 'check_for_updates').length === n + 1, initialChecks);
  await page.getByRole('dialog').getByRole('button', {name:'Close',exact:true}).click();
  assert.equal(await updateCount(), initialChecks + 1, 'Closing About does not check again');
  await page.evaluate(() => window.failUpdate = false);
  await page.getByRole('button', {name:'About',exact:true}).click();
  await page.getByRole('button', {name:'GitHub: Version 1.0.10 available',exact:true}).waitFor();
  assert.equal(await updateCount(), initialChecks + 2, 'Reopening About retries after failure');
  await page.getByRole('dialog').getByRole('button', {name:'Close',exact:true}).click();
  for (const status of ['current', 'packaged']) {
    await page.evaluate(status => window.updateStatus = {status}, status);
    await page.getByRole('button', {name:/^About/}).click();
    const github = page.getByRole('dialog').getByRole('button', {name:'GitHub',exact:true});
    await github.waitFor();
    assert.equal(await github.getAttribute('data-accent-color'), 'gray');
    await github.click();
    assert.equal(await page.evaluate(() => window.openedUrl), 'https://github.com/noih/cs-demodesk');
    await page.getByRole('dialog').getByRole('button', {name:'Close',exact:true}).click();
  }
  await page.getByRole('tab').filter({hasText:'Players'}).click();
  const tables = page.getByRole('table');
  assert.equal(await tables.count(), 2);
  assert.equal(await tables.first().getByRole('columnheader', {name:'Opening kills',exact:true}).count(), 1);
  const cells = tables.first().locator('tbody tr').first().getByRole('cell');
  assert.deepEqual(await cells.allTextContents(), ['20 / 10 / 3','2.00','90.0','50%','77.3%','4','2','2 / 1 / 0 / 0','1']);
  assert.match(await cells.nth(4).getAttribute('style'), /77.3%/, 'KAST bar uses the percentage');
  assert.match(await cells.nth(2).getAttribute('style'), /100%/, 'ADR bar uses the match maximum');
  for (const width of [1360, 900]) {
    await page.setViewportSize({width,height:940});
    const top = await tables.first().boundingBox();
    const bottom = await tables.last().boundingBox();
    assert.ok(bottom.y >= top.y + top.height, 'Teams stack at every width');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Table overflow stays inside the page');
    if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/players-'+width+'.png'});
  }
  for (const category of ['Aim','Activity','Utility','Opening duels','Trades','Clutches','Duels']) {
    await page.getByRole('button',{name:category,exact:true}).click();
    assert.equal(await tables.count(),2);
    assert.ok(!(await tables.allTextContents()).join().includes('NaN'));
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/stats-'+category.replaceAll(' ','-')+'.png'});
  }
  await page.getByRole('button',{name:'Aim',exact:true}).click();
  assert.ok((await tables.first().textContent()).includes('25.0%'));
  await page.getByRole('button',{name:'AWP',exact:true}).click();
  assert.ok((await tables.first().textContent()).includes('—'));
  assert.ok(!(await tables.first().textContent()).includes('NaN'));
  await page.getByRole('button',{name:'Duels',exact:true}).click();
  const duelCells = tables.first().locator('tbody tr').first().getByRole('cell');
  assert.equal(await duelCells.nth(1).innerText(), '3 (75.0%)');
  assert.match(await duelCells.nth(1).getAttribute('style'), /75%/);
  assert.match(await duelCells.nth(0).getAttribute('style'), / 0%/);
  await page.getByRole('button',{name:'Utility',exact:true}).click();
  assert.equal(await tables.first().getByRole('columnheader',{name:'Damage / HE',exact:true}).count(),1);
  assert.ok((await tables.first().textContent()).includes('10.00'));
  await page.setViewportSize({width:1360,height:940});
  await page.getByRole('tab').filter({hasText:'2D'}).click();
  const replaySpinner = page.locator('.app-spinner').filter({visible:true}).last();
  await replaySpinner.waitFor();
  for(const reducedMotion of ['reduce','no-preference']) {
    await page.emulateMedia({reducedMotion});
    const before = await replaySpinner.evaluate(el=>({name:getComputedStyle(el).animationName,transform:getComputedStyle(el).transform,duration:getComputedStyle(el).animationDuration}));
    assert.equal(before.name,'app-spinner-rotate');
    assert.equal(before.duration,reducedMotion==='reduce'?'1.8s':'0.9s');
    await page.waitForTimeout(120);
    assert.notEqual(await replaySpinner.evaluate(el=>getComputedStyle(el).transform),before.transform,'Loading spinner advances while 2D remains pending');
  }
  await page.getByRole('tab').filter({hasText:'Charts'}).click();
  await page.locator('canvas').first().waitFor();
  const playerLegends = page.locator('.chart-legend').first().locator('button');
  assert.deepEqual(await playerLegends.allTextContents(), ['Player', 'Player']);
  await playerLegends.first().click();
  assert.equal(await playerLegends.first().getAttribute('aria-pressed'), 'false');
  assert.equal(await playerLegends.nth(1).getAttribute('aria-pressed'), 'true');
  await playerLegends.first().click();
  for (const name of ['Deaths','Damage','AWP kills','Enemies flashed','Score difference','Team cash']) {
    await page.getByRole('button',{name,exact:true}).first().click();
    assert.equal(await page.getByRole('button',{name,exact:true}).first().getAttribute('aria-pressed'),'true');
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  }
  if(process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/trends.png'});
  const trendCanvas = page.locator('.chart canvas').first();
  await trendCanvas.scrollIntoViewIfNeeded();
  await page.evaluate(() => {
    window.timelineWheelPrevented = undefined;
    window.addEventListener('wheel', event => { window.timelineWheelPrevented = event.defaultPrevented; }, { once: true });
  });
  await trendCanvas.hover();
  await page.mouse.wheel(0, 100);
  await page.waitForFunction(() => window.timelineWheelPrevented !== undefined);
  assert.equal(await page.evaluate(() => window.timelineWheelPrevented), false, 'Timeline wheel allows page scrolling');
  await page.mouse.move(0, 0);
  const recoil = page.getByTestId('recoil-chart');
  await recoil.getByText('2 bursts',{exact:true}).waitFor();
  assert.equal(await recoil.getByText('No qualifying bursts',{exact:true}).count(),2);
  await recoil.scrollIntoViewIfNeeded();
  const recoilPlayers = recoil.getByRole('button', { name: '━ Player', exact: true });
  await recoilPlayers.first().click();
  assert.deepEqual(await recoilPlayers.evaluateAll(buttons => buttons.map(button => button.getAttribute('aria-pressed'))), ['false', 'false', 'false']);
  await recoilPlayers.last().click();
  assert.deepEqual(await recoilPlayers.evaluateAll(buttons => buttons.map(button => button.getAttribute('aria-pressed'))), ['true', 'true', 'true']);
  const zoomIn = recoil.getByRole('button',{name:'Zoom in',exact:true});
  const zoomOut = recoil.getByRole('button',{name:'Zoom out',exact:true});
  const resetZoom = recoil.getByRole('button',{name:'Reset zoom',exact:true});
  assert.ok(!(await zoomOut.isDisabled()));
  const checkPlot = async (zoom, playerPoint = false) => {
    // Check a late reference point even for 3-shot bursts, plus known player positions.
    await page.waitForFunction(({ zoom, accent, weapons, playerPoint }) => {
      const canvas = document.querySelector('[data-testid="recoil-chart"] canvas');
      const plots = [weapons.ak47,weapons.m4a1,weapons.m4a1_silencer].map(points => {
        const xs = points.map(p=>p.x), ys = points.map(p=>p.y);
        return { points, cx: (Math.min(0,...xs)+Math.max(0,...xs))/2, cy: (Math.min(0,...ys)+Math.max(0,...ys))/2 };
      });
      const halfSpan = Math.max(10,...plots.flatMap(p=>p.points.map(v=>Math.max(Math.abs(v.x-p.cx),Math.abs(v.y-p.cy))*1.2))) / (zoom*.8*0.84375);
      const {cx,cy,points} = plots[0];
      const inset = Math.ceil(parseFloat(getComputedStyle(canvas).getPropertyValue('--app-font-caption')) * 3 + 8);
      const point = playerPoint ? { x: -40, y: -100 } : points.reduce((best,p)=>Math.hypot(p.x-cx,p.y-cy)<Math.hypot(best.x-cx,best.y-cy)?p:best);
      return [[point.x, point.y]].every(([px, py]) => {
        const x = (inset + (px - cx + halfSpan) / (2 * halfSpan) * (canvas.clientWidth - inset - 20)) * canvas.width / canvas.clientWidth;
        const y = (20 + (halfSpan - (py - cy)) / (2 * halfSpan) * (canvas.clientHeight - inset - 20)) * canvas.height / canvas.clientHeight;
        const pixels = canvas.getContext('2d').getImageData(Math.round(x)-3, Math.round(y)-3, 7, 7).data;
        return Array.from({length:49},(_,n)=>n*4).some(offset=>accent.every((value,i)=>Math.abs(pixels[offset+i]-value)<5));
      });
    }, { zoom, playerPoint, weapons: Object.fromEntries(Object.entries(calibration.weapons).map(([id,points])=>[id,projectReference(points)])), accent: (playerPoint ? THEMES.light.players.split(',')[7] : THEMES.light.accent).match(/[a-f0-9]{2}/gi).map(v => parseInt(v, 16)) });
  };
  await checkPlot(1);
  const burstView = recoil.getByRole('combobox', { name: 'AK-47 Burst view' });
  const previousBurst = recoil.getByRole('button', { name: 'AK-47 Previous burst', exact: true });
  const nextBurst = recoil.getByRole('button', { name: 'AK-47 Next burst', exact: true });
  await recoilPlayers.first().click();
  const fixedReferenceImage = await recoil.locator('canvas').first().evaluate(canvas=>canvas.toDataURL());
  for (const button of [nextBurst,nextBurst,previousBurst,previousBurst]) {
    await button.click();
    await page.waitForTimeout(100);
    assert.equal(await recoil.locator('canvas').first().evaluate(canvas=>canvas.toDataURL()),fixedReferenceImage,'Full reference and axes stay pixel-identical across bursts with different origins');
  }
  await recoilPlayers.first().click();
  await zoomIn.click();
  assert.equal(await resetZoom.innerText(), '120%');
  assert.ok(await previousBurst.isDisabled());
  await nextBurst.click();
  assert.match(await burstView.innerText(), /Burst 1/);
  await checkPlot(1.2);
  await nextBurst.click();
  assert.match(await burstView.innerText(), /Burst 2/);
  await checkPlot(1.2); // The extreme second-shot outlier must not rescale the reference.
  assert.ok(await nextBurst.isDisabled());
  await previousBurst.click();
  await previousBurst.click();
  assert.equal(await burstView.innerText(), 'Average');
  const selectorBox = await burstView.boundingBox();
  const previousBox = await previousBurst.boundingBox(), nextBox = await nextBurst.boundingBox();
  assert.ok(selectorBox.width > 200 && Math.abs(selectorBox.x - previousBox.x - previousBox.width - 4) < 2 && Math.abs(nextBox.x - selectorBox.x - selectorBox.width - 4) < 2, 'Selector fills the space between navigation buttons');
  assert.ok(await recoil.getByRole('button', { name: 'M4A4 Next burst', exact: true }).isDisabled());
  const meanImage = await recoil.locator('canvas').first().evaluate(canvas => canvas.toDataURL());
  await burstView.click();
  await page.getByRole('option', { name: 'Burst 2 · R2 · tick 200 · 3 shots', exact: true }).click();
  await page.waitForFunction(before => document.querySelector('[data-testid="recoil-chart"] canvas').toDataURL() !== before, meanImage);
  assert.match(await burstView.innerText(), /Burst 2/);
  await checkPlot(1.2); // The extreme second-shot outlier must not rescale the reference.
  await burstView.click();
  await page.getByRole('option', { name: 'Average', exact: true }).click();
  await checkPlot(1.2, true);
  assert.equal(await resetZoom.innerText(), '120%', 'Burst arrows and selector preserve zoom');
  await resetZoom.click();
  await checkPlot(1, true);
  const playShots = recoil.getByRole('button', { name: 'Play AK-47', exact: true });
  const playPosition = await playShots.boundingBox(), chartPosition = await recoil.locator('.recoil-plot').first().boundingBox();
  const plotInset = await recoil.locator('canvas').first().evaluate(canvas => Math.ceil(parseFloat(getComputedStyle(canvas).getPropertyValue('--app-font-caption')) * 3 + 8));
  assert.ok(Math.abs(playPosition.x + playPosition.width - (chartPosition.x + chartPosition.width - 28)) < 2 && Math.abs(playPosition.y + playPosition.height - (chartPosition.y + chartPosition.height - plotInset - 8)) < 2, 'Playback is inside the plotting area at the bottom right');
  assert.equal(playPosition.width,30);
  assert.equal(playPosition.height,30);
  const rightGap = chartPosition.x + chartPosition.width - 20 - playPosition.x - playPosition.width;
  const bottomGap = chartPosition.y + chartPosition.height - plotInset - playPosition.y - playPosition.height;
  assert.ok(Math.abs(rightGap-8)<1 && Math.abs(bottomGap-8)<1, 'Playback has equal 8px right and bottom gaps');
  const iconPosition = await playShots.locator('svg').boundingBox();
  assert.ok(Math.abs(iconPosition.x + iconPosition.width/2 - playPosition.x - playPosition.width/2) < 1 && Math.abs(iconPosition.y + iconPosition.height/2 - playPosition.y - playPosition.height/2) < 1, 'Playback SVG is centred in both axes');
  assert.equal(await playShots.innerText(), '', 'Playback uses only the familiar icon');
  assert.ok(await recoil.getByRole('button', { name: 'Play M4A4', exact: true }).isDisabled());
  assert.ok(await recoil.getByRole('button', { name: 'Play M4A1-S', exact: true }).isDisabled());
  const clockStart = new Date();
  await page.clock.install({time: clockStart});
  await page.clock.pauseAt(new Date(clockStart.getTime() + 1000));
  await page.evaluate(() => {
    const start = window.setInterval.bind(window), clear = window.clearInterval.bind(window);
    window.shotIntervals = new Set();
    window.setInterval = (callback, delay, ...args) => {
      const id = start(callback, delay, ...args);
      if (delay === 50) window.shotIntervals.add(id);
      return id;
    };
    window.clearInterval = id => { window.shotIntervals.delete(id); clear(id); };
  });
  const recoilCanvases = () => recoil.locator('.recoil-plot canvas').evaluateAll(canvases => canvases.map(canvas => canvas.toDataURL()));
  const completeShots = await recoilCanvases();
  await playShots.click();
  const pauseShots = recoil.getByRole('button', { name: 'Pause AK-47', exact: true });
  await pauseShots.waitFor();
  await page.clock.runFor(20);
  const firstShot = await recoilCanvases();
  assert.ok(firstShot[0] !== completeShots[0], 'Playback reveals a partial player trajectory');
  assert.ok(firstShot.slice(1).every((image,i) => image === completeShots[i+1]), 'Other weapon panels do not change');
  await checkPlot(1); // The complete fixed reference remains visible during playback.
  // The second shot is an off-screen outlier; check the third visible shot's pulse.
  await page.clock.runFor(200);
  const pulsingShot = await recoilCanvases();
  await page.clock.runFor(50);
  const settledShot = await recoilCanvases();
  assert.ok(settledShot[0] !== pulsingShot[0], 'The newly fired point settles to its normal size');
  await pauseShots.click();
  await playShots.waitFor();
  assert.equal(await page.evaluate(() => window.shotIntervals.size), 0, 'Pause clears the interval');
  const pausedShots = await recoilCanvases();
  await page.clock.runFor(500);
  assert.ok((await recoilCanvases()).every((image,i) => image === pausedShots[i]), 'Pause freezes the trajectory');
  await playShots.click();
  await page.clock.fastForward(1000);
  await page.clock.runFor(20);
  await playShots.waitFor();
  await checkPlot(1, true); // The final player shot is visible again.
  assert.equal(await page.evaluate(() => window.shotIntervals.size), 0, 'Completion clears the interval');
  await playShots.click();
  await pauseShots.waitFor();
  await page.clock.runFor(1000);
  await playShots.waitFor();
  await playShots.click();
  await pauseShots.waitFor();
  await page.getByRole('tab', { name: 'Players', exact: true }).click();
  assert.equal(await page.evaluate(() => window.shotIntervals.size), 0, 'Unmount clears the interval');
  await page.getByRole('tab', { name: 'Charts', exact: true }).click();
  await playShots.waitFor();
  await page.clock.resume();
  const controlHeights = await recoil.locator('.recoil-controls .rt-Button,.recoil-controls .rt-IconButton,.recoil-controls .rt-SelectTrigger').evaluateAll(elements=>elements.map(el=>el.getBoundingClientRect().height));
  assert.ok(Math.max(...controlHeights)-Math.min(...controlHeights)<1,'Recoil controls share a height');
  const plot = recoil.locator('.recoil-plot').first();
  await plot.hover();
  await page.mouse.wheel(0,-100);
  await page.waitForFunction(()=>document.querySelector('[aria-label="Reset zoom"]').textContent==='120%');
  await resetZoom.click();
  await plot.scrollIntoViewIfNeeded();
  const box = await plot.boundingBox();
  const beforePan = await recoil.locator('.recoil-plot canvas').evaluateAll(canvases=>canvases.map(canvas=>canvas.toDataURL()));
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);
  await page.mouse.down();
  await page.mouse.move(box.x+box.width/2+40,box.y+box.height/2+25,{steps:4});
  await page.mouse.up();
  await page.waitForFunction(before=>Array.from(document.querySelectorAll('.recoil-plot canvas')).every((canvas,i)=>canvas.toDataURL()!==before[i]),beforePan);
  await resetZoom.click();
  await checkPlot(1);
  await zoomIn.click();
  assert.equal(await resetZoom.innerText(),'120%');
  await checkPlot(1.2);
  await zoomOut.click();
  assert.equal(await resetZoom.innerText(),'100%');
  await checkPlot(1);
  for(let step=0;step<8;step++) await zoomIn.click();
  assert.equal(await resetZoom.innerText(),'250%');
  await checkPlot(2.5);
  assert.ok(await zoomIn.isDisabled());
  await plot.hover();
  await page.mouse.wheel(0,-100);
  await page.waitForTimeout(100);
  assert.equal(await resetZoom.innerText(),'250%','Wheel cannot exceed 250%');
  for(let step=0;step<13;step++) await zoomOut.click();
  assert.equal(await resetZoom.innerText(),'5%');
  await checkPlot(0.05);
  assert.ok(await zoomOut.isDisabled());
  await plot.hover();
  await page.mouse.wheel(0,100);
  await page.waitForTimeout(100);
  assert.equal(await resetZoom.innerText(),'5%','Wheel cannot go below 5%');
  await page.mouse.wheel(0,-100);
  await page.waitForFunction(()=>document.querySelector('[aria-label="Reset zoom"]').textContent==='20%');
  await plot.hover();
  await page.mouse.wheel(0,100);
  await page.waitForFunction(()=>document.querySelector('[aria-label="Reset zoom"]').textContent==='5%');
  assert.ok(await zoomOut.isDisabled());
  await resetZoom.click();
  assert.equal(await resetZoom.innerText(),'100%');
  await checkPlot(1);
  await zoomIn.click();
  for(const width of [1360,900]) {
    await page.setViewportSize({width,height:940});
    await recoil.scrollIntoViewIfNeeded();
    const bounds = await recoil.locator('canvas').first().boundingBox();
    assert.ok(bounds.width>100 && Math.abs(bounds.width-bounds.height)<2,'Recoil plot uses equal angle scales');
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
    if(process.env.UI_SCREENSHOT_DIR) await recoil.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/recoil-'+width+'.png'});
  }
  await playShots.click();
  await recoil.getByRole('combobox',{name:'Spray player'}).click();
  await page.getByRole('option',{name:'Player',exact:true}).nth(1).click();
  assert.equal(await recoil.getByText('No qualifying bursts',{exact:true}).count(),3);
  assert.equal(await page.evaluate(() => window.shotIntervals.size), 0, 'Changing player clears the interval');
  assert.equal(await resetZoom.innerText(),'120%','Changing player preserves zoom');
  assert.ok(!(await resetZoom.isDisabled()),'Reference remains available without player bursts');
  await recoil.getByText('━ Compensation reference',{exact:true}).first().waitFor();
  await page.setViewportSize({width:1360,height:940});
  await page.getByRole('button',{name:'Switch to dark mode'}).click();
  await page.getByRole('button',{name:'Filter demos'}).click();
  const dateOverflow = await page.getByRole('button', {name:'From',exact:true}).evaluate(button => {
    button.style.maxWidth = '95px';
    const text = button.querySelector('[title]');
    text.textContent = '起始日期';
    const before = button.getBoundingClientRect();
    const style = getComputedStyle(text);
    const fits = text.getBoundingClientRect().bottom <= before.bottom && text.getBoundingClientRect().right <= before.right;
    const result = {fits, whiteSpace:style.whiteSpace, overflow:style.overflow, ellipsis:style.textOverflow};
    text.textContent = 'From';
    button.style.maxWidth = '';
    return result;
  });
  assert.deepEqual(dateOverflow, {fits:true,whiteSpace:'nowrap',overflow:'hidden',ellipsis:'ellipsis'}, 'Narrow date labels stay on one line inside the button');
  for (const label of ['From', 'To']) {
    const fits = await page.getByRole('button', {name:label,exact:true}).evaluate(button => {
      const text = button.querySelector('[title]');
      const original = text.textContent;
      text.textContent = '2026/09/10';
      const fits = text.scrollWidth <= text.clientWidth;
      text.textContent = original;
      return fits;
    });
    assert.ok(fits, 'Date filter expands to display a complete date');
  }
  await page.getByRole('textbox').fill('   ');
  assert.equal(await page.locator('.filter-trigger').getAttribute('data-filtered'), 'false');
  await page.getByRole('textbox').fill('no matches');
  assert.equal(await page.locator('.filter-trigger').getAttribute('data-filtered'), 'true');
  assert.equal(await page.locator('.filter-trigger .bi-funnel-fill').count(), 1);
  await page.keyboard.press('Escape');
  await page.locator('.queue-trigger').click();

  assert.equal(await page.locator('.queue-trigger').count(), 1, 'One shared queue entry');
  assert.match(await page.getByRole('dialog').getByRole('tab').first().innerText(), /^Video export/);
  assert.equal(await page.getByRole('dialog').getByRole('tab').first().getAttribute('data-state'), 'active');
  await page.getByRole('dialog').getByRole('tab', {name:/^Anomaly analysis/}).click();
  await page.getByRole('dialog').getByText('No jobs', {exact:true}).waitFor();
  await page.getByRole('dialog').getByRole('tab', {name:/^Video export/}).click();
  await page.locator('.queue-clips').getByText('Player — 3 kills · R8').first().waitFor();
  await page.getByRole('button',{name:'Open demo',exact:true}).last().click();
  await page.locator('.demo-item.active').waitFor();
  assert.equal(await page.locator('.filter-trigger').getAttribute('data-filtered'), 'false', 'Queue navigation clears filter indicator');
  assert.match(await page.locator('.demo-heading').innerText(),/Inferno/);
  assert.equal(await page.getByRole('tab').filter({hasText:'Videos'}).getAttribute('data-state'),'active');
  assert.ok(await page.locator('.demo-item').count()<40);
  const headerButtonBounds = () => page.locator('.app-header button').evaluateAll(buttons => buttons.map(button => {
    const { x, y, width, height } = button.getBoundingClientRect();
    return { x, y, width, height };
  }));
  const detailHeaderBounds = await headerButtonBounds();
  assert.ok(detailHeaderBounds.every(b => b.height === 32), 'All header buttons share a 32px height');
  assert.ok(await page.locator('.app-header .rt-IconButton').evaluateAll(buttons => buttons.every(b => b.getBoundingClientRect().width === 32)), 'Header icon buttons are square');
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByText('Language',{exact:true}).waitFor();
  const settingsPage = page.locator('.settings-page');
  assert.equal(await settingsPage.getByText('Anomaly data', {exact:true}).count(), 1);
  await settingsPage.getByText('Anomaly data', {exact:true}).locator('xpath=following-sibling::button[1]').click();
  assert.equal(await page.evaluate(()=>window.openedPath.replaceAll('\\','/')), 'E:/data/analysis');
  const directoryInput = settingsPage.getByLabel('Data directory', {exact:true});
  const originalDark = (await page.locator('.radix-themes').first().getAttribute('class')).split(' ').includes('dark');
  for (const mode of ['light','dark']) {
    if (!(await page.locator('.radix-themes').first().getAttribute('class')).split(' ').includes(mode)) {
      await page.getByRole('button', {name:mode === 'dark' ? 'Switch to dark mode' : 'Switch to light mode'}).click();
    }
    await directoryInput.fill('E:/custom-data');
    const colors = await directoryInput.evaluate(el => ({text:getComputedStyle(el).color,placeholder:getComputedStyle(el,'::placeholder').color}));
    assert.notEqual(colors.text, colors.placeholder, 'Custom and default paths have distinct colors in ' + mode);
    await directoryInput.fill('');
  }
  if (!originalDark) await page.getByRole('button', {name:'Switch to light mode'}).click();

  assert.equal(await settingsPage.getByRole('heading', {name:'Check results',exact:true}).count(), 0, 'Tool checks are merged into the path fields');
  assert.equal(await settingsPage.getByLabel('Steam install folder', {exact:true}).count(), 1);
  assert.equal(await settingsPage.getByLabel('CS2 install folder', {exact:true}).count(), 1);
  assert.equal(await settingsPage.getByRole('button', {name:'Download: Steam',exact:true}).count(), 0);
  assert.equal(await settingsPage.getByRole('button', {name:'Download: CS2',exact:true}).count(), 0);

  const toolPositions = await settingsPage.evaluate(el => {
    const buttons = [...el.querySelectorAll('button')];
    return ['HLAE','FFmpeg','Source 2 Viewer CLI'].map(name => buttons.find(b => b.getAttribute('aria-label') === 'Download: ' + name).getBoundingClientRect().x);
  });
  assert.ok(toolPositions.every(x => x === toolPositions[0]), 'Download buttons align across tools');

  for (const label of ['HLAE.exe', 'ffmpeg.exe', 'Source 2 Viewer CLI']) {
    assert.equal(await settingsPage.getByRole('button', {name:'Download source: ' + label,exact:true}).count(), 1);
  }

  assert.equal(await page.getByLabel('HLAE.exe', {exact:true}).getAttribute('placeholder'), 'Not detected');
  assert.equal(await page.getByLabel('ffmpeg.exe', {exact:true}).getAttribute('placeholder'), 'Not detected');
  assert.equal(await page.getByLabel('Source 2 Viewer CLI', {exact:true}).count(), 1);
  await page.getByText('Tools are not fully installed. Some features will be limited.', {exact:true}).waitFor();
  const warningAlignment = await page.getByText('Tools are not fully installed. Some features will be limited.', {exact:true}).locator('..').evaluate(el => {
    const icon = el.querySelector('.rt-CalloutIcon').getBoundingClientRect();
    const text = el.querySelector('.rt-CalloutText').getBoundingClientRect();
    return Math.abs(icon.y + icon.height / 2 - text.y - text.height / 2);
  });
  assert.ok(warningAlignment < 1, 'Callout icon and text are vertically centered');
  const toolLayout = await settingsPage.evaluate(el => {
    const recheck = [...el.querySelectorAll('button')].find(b=>b.textContent.trim()==='Check again');
    const card = recheck.closest('.rt-Card').getBoundingClientRect();
    const button = recheck.getBoundingClientRect();
    const warning = recheck.closest('.rt-Card').querySelector('.rt-CalloutRoot').getBoundingClientRect();
    const firstTool = recheck.closest('.rt-Card').querySelector('.tool-field').getBoundingClientRect();
    return {center:Math.abs(button.x+button.width/2-card.x-card.width/2),warningBottom:warning.bottom,toolTop:firstTool.top};
  });
  assert.ok(toolLayout.center < 1 && toolLayout.warningBottom <= toolLayout.toolTop, 'Warning is above tools and recheck is centered');



  assert.equal(await settingsPage.locator('.rt-Badge').count(), 0, 'Settings statuses are plain text');
  assert.equal(await settingsPage.locator('button.rt-variant-ghost, button.rt-variant-soft').count(), 0, 'Settings actions use clear outlined or solid controls');
  const toolWarning = settingsPage.getByText('Tools are not fully installed. Some features will be limited.', {exact:true});
  for (const missing of ['hlaeExe', 'hlaeDll', 'ffmpegExe', 'vrfExe', null]) {
    await page.evaluate(missing => {
      window.toolPaths = {hlaeExe:'E:/tools/HLAE.exe',hlaeDll:'E:/tools/AfxHookSource2.dll',ffmpegExe:'E:/tools/ffmpeg.exe',vrfExe:'E:/tools/Source2Viewer-CLI.exe'};
      if (missing) delete window.toolPaths[missing];
    }, missing);
    await settingsPage.getByRole('button', {name:'Check again',exact:true}).click();
    await toolWarning.waitFor({state:missing ? 'visible' : 'hidden'});
    await page.locator('.notification-viewport .app-toast').getByRole('button', {name:'Close',exact:true}).click();
  }
  const actionHeights = await settingsPage.locator('.rt-Button, .rt-IconButton').evaluateAll(buttons => buttons.map(b => b.getBoundingClientRect().height));
  assert.ok(actionHeights.every(height => height >= 32), 'Settings action controls accommodate the selected text size');
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/settings-redesign.png'});
  assert.deepEqual(await headerButtonBounds(), detailHeaderBounds, 'Header button positions and sizes remain stable on Settings');
  await page.setViewportSize({width:960,height:720});
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.locator('.demo-tabs').waitFor();
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'No window-level horizontal overflow');
  for (const mode of ['dark', 'light']) {
    const root = page.locator('.radix-themes').first();
    if (!(await root.getAttribute('class')).split(' ').includes(mode)) {
      await page.getByRole('button', { name: mode === 'dark' ? 'Switch to dark mode' : 'Switch to light mode' }).click();
    }
    await page.mouse.move(500, 20);
    await page.evaluate(() => document.activeElement?.blur());
    await page.getByRole('button', { name: mode === 'dark' ? 'Switch to light mode' : 'Switch to dark mode' }).hover();
    const tooltip = page.locator('.rt-TooltipContent');
    await tooltip.waitFor();
    const tip = await tooltip.evaluate(el => {
      const style = getComputedStyle(el);
      const text = el.querySelector('.rt-TooltipText');
      const rect = text.getBoundingClientRect();
      const probe = document.createElement('span');
      probe.style.color = 'var(--app-raised)';
      el.append(probe);
      const expected = getComputedStyle(probe).color;
      probe.style.color = 'var(--app-text)';
      const expectedForeground = getComputedStyle(probe).color;
      probe.remove();
      return {
        background: style.backgroundColor, expected, expectedForeground,
        foreground: getComputedStyle(text).color,
        arrow: getComputedStyle(el.querySelector('.rt-TooltipArrow')).fill,
        onTop: el.contains(document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)),
      };
    });
    assert.equal(tip.background, tip.expected, 'Tooltip retains its contrasting background');
    assert.equal(tip.arrow, tip.background, 'Tooltip arrow matches its surface');
    assert.equal(tip.foreground, tip.expectedForeground, 'Tooltip text uses the theme foreground');
    assert.ok(tip.onTop, 'Tooltip text is not covered or clipped');
    await page.mouse.move(500, 500);
    await page.keyboard.press('Escape');
    await tooltip.waitFor({ state: 'detached' });
    const expected = THEMES[mode].panel;
    const checkPanel = async locator => {
      const result = await locator.evaluate(el => {
        const style = getComputedStyle(el);
        return { token: style.getPropertyValue('--app-panel').trim(), background: style.backgroundColor, blur: style.backdropFilter };
      });
      assert.equal(result.token, expected, 'Portal inherits current theme tokens');
      assert.equal(result.blur, 'none', 'No frosted glass effect');
      assert.notEqual(result.background, 'rgba(0, 0, 0, 0)', 'Portal background must be opaque');
    };
    await page.locator('.demo-heading').getByRole('button', { name: 'More', exact: true }).click();
    await checkPanel(page.getByRole('menu'));
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Filter demos' }).click();
    await checkPanel(page.locator('.rt-PopoverContent'));
    await page.keyboard.press('Escape');
    await page.locator('.queue-trigger').click();
    await checkPanel(page.locator('.rt-DialogContent'));
    await page.keyboard.press('Escape');
    await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
    const notice = page.locator('.notification-viewport .app-toast');
    await notice.getByText('Updated',{exact:true}).waitFor();
    const feedback = await notice.evaluate(el=>{
      const text=el.querySelector('.rt-CalloutText').getBoundingClientRect();
      const close=el.querySelector('button').getBoundingClientRect();
      const box=el.getBoundingClientRect();
      const header=document.querySelector('.app-header').getBoundingClientRect();
      const probe=document.createElement('span');
      probe.style.color='var(--app-panel)';el.append(probe);
      const expected=getComputedStyle(probe).color;probe.remove();
      return { background:getComputedStyle(el).backgroundColor, expected,
        gap:close.x-text.right, aligned:Math.abs((text.y+text.height/2)-(close.y+close.height/2))<1,
        inset:text.x-box.x, bottom:innerHeight-box.bottom, belowHeader:box.y>header.bottom };
    });
    assert.equal(feedback.background,feedback.expected,'Status notification has an opaque theme surface');
    assert.ok(feedback.aligned && feedback.gap>=16 && feedback.inset>=12,'Status text and close button have separate, aligned space');
    assert.equal(feedback.bottom,24);
    assert.ok(feedback.belowHeader,'Status does not cover the toolbar');
    if(process.env.UI_SCREENSHOT_DIR) await notice.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/refresh-'+mode+'.png'});
    await notice.getByRole('button',{name:'Close',exact:true}).click();
    await notice.waitFor({state:'detached'});
  }
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.evaluate(()=>window.holdRefresh=true);
  await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
  const spinner=page.locator('.header-tools .app-spinner');
  await spinner.waitFor();
  const initial=await spinner.evaluate(e=>getComputedStyle(e).transform);
  await page.waitForTimeout(180);
  const animated=await spinner.evaluate(e=>({transform:getComputedStyle(e).transform,opacity:getComputedStyle(e).opacity}));
  assert.notEqual(initial,animated.transform,'Spinner rotates');
  assert.equal(animated.opacity,'1','Spinner does not pulse');
  await page.emulateMedia({reducedMotion:'reduce'});
  assert.equal(await spinner.evaluate(e=>getComputedStyle(e).animationName),'app-spinner-rotate','Reduced motion retains slow loading feedback');
  await page.waitForFunction(()=>typeof window.releaseRefresh==='function');
  await page.evaluate(()=>{window.holdRefresh=false;window.releaseRefresh()});
  await spinner.waitFor({state:'detached'});
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.evaluate(()=>{window.missingTools=true;window.toolPaths={steamDir:'E:/Steam',cs2Exe:'E:/CS2/game/bin/win64/cs2.exe',cs2Dir:'E:/CS2'};});
  await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
  await page.locator('.header-tools [aria-busy="true"]').waitFor();
  await page.locator('.header-tools [aria-busy="true"]').waitFor({state:'detached'});
  assert.equal(await page.locator('.notification-viewport [role="alert"]').count(),0,'Missing tools do not show a global notice');
  await page.getByRole('tab').filter({hasText:'Players'}).click();
  await page.getByRole('tab').filter({hasText:'2D'}).click();
  await page.setViewportSize({width:1360,height:600});
  await page.getByRole('button',{name:'Missing Source 2 Viewer. Go to Settings to download.'}).click();
  const download = page.getByRole('button', {name:'Download: Source 2 Viewer CLI',exact:true});
  await download.waitFor();
  await page.locator('.driver-popover').waitFor();
  assert.equal(await page.locator('.driver-active-element.tool-field').count(), 1, 'Single-tool guidance highlights the entire tool field');
  assert.equal(await page.locator('.driver-active-element').getByRole('button', {name:'Download: Source 2 Viewer CLI',exact:true}).count(), 1);
  assert.equal(await page.locator('.driver-popover-title').innerText(), 'Download: Source 2 Viewer CLI');
  assert.deepEqual(await download.evaluate(el => [getComputedStyle(el).animationName, getComputedStyle(el).animationIterationCount]), ['tools-highlight-pulse', 'infinite']);
  for (const reducedMotion of ['no-preference', 'reduce']) {
    await page.emulateMedia({reducedMotion});
    const initialColor = await download.evaluate(el => getComputedStyle(el).backgroundColor);
    await page.waitForFunction(initial => {
      const button = document.querySelector('.driver-active-element [data-guide-missing="true"]');
      return button && getComputedStyle(button).backgroundColor !== initial;
    }, initialColor);
    assert.equal(await download.evaluate(el => getComputedStyle(el).animationIterationCount), 'infinite');
  }
  await page.emulateMedia({reducedMotion:'no-preference'});
  const arrowPosition = await page.locator('.driver-popover').evaluate(popover => {
    const arrow = popover.querySelector('.driver-popover-arrow');
    const target = document.querySelector('.driver-active-element').getBoundingClientRect();
    const tip = arrow.getBoundingClientRect();
    return { side: arrow.className, target: {x:target.x,y:target.y}, tip: {x:tip.x,y:tip.y}, aligned: tip.top <= target.bottom && tip.bottom >= target.top };
  });
  assert.ok(arrowPosition.aligned, JSON.stringify(arrowPosition));
  assert.ok(arrowPosition.side.includes('driver-popover-arrow-side-left'), 'Arrow points right toward the download button');
  const arrowStyle = await page.locator('.driver-popover-arrow').evaluate(el => {
    const style = getComputedStyle(el);
    return [style.borderTopColor, style.borderRightColor, style.borderBottomColor, style.borderLeftColor];
  });
  assert.deepEqual(arrowStyle, ['rgba(0, 0, 0, 0)', 'rgba(0, 0, 0, 0)', 'rgba(0, 0, 0, 0)', 'rgb(255, 255, 255)'], 'Arrow triangle faces right, not down');
  await page.locator('.driver-popover').getByRole('button', {name:'Close',exact:true}).click();
  await page.locator('.driver-popover').waitFor({state:'detached'});
  await page.setViewportSize({width:1360,height:940});
  assert.ok(await download.evaluate(el => {
    const r = el.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= innerHeight;
  }), 'Download action is scrolled into view');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length), 0, 'Guidance does not start downloads');
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.waitForFunction(() => window.testListenerCount() === 1);
  await page.evaluate(() => { window.holdListener = true; });
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('button', {name:'Download: Source 2 Viewer CLI',exact:true}).waitFor();
  assert.equal(await page.locator('.driver-popover').count(), 0, 'Ordinary settings navigation does not replay guidance');
  assert.equal(await page.locator('.demo-item.active, .demo-item[aria-pressed="true"]').count(), 0, 'Settings does not mark a demo as active');
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.evaluate(() => { window.holdListener = false; window.releaseListener(); });
  await page.waitForFunction(() => window.testCalls.filter(c => c.cmd === 'plugin:event|unlisten').length === window.testCalls.filter(c => c.cmd === 'plugin:event|listen').length - 1);
  assert.equal(await page.evaluate(() => window.testListenerCount()), 1, 'A listener resolved after settings unmount is removed');
  await page.getByRole('tab').filter({hasText:'Highlights'}).click();
  await page.getByRole('button',{name:'Select all',exact:true}).click();
  await page.getByRole('button',{name:/^Export/}).click();
  const exportDialog = page.getByRole('dialog').filter({has:page.getByRole('button',{name:'Export',exact:true})});
  await exportDialog.waitFor();
  assert.equal(await exportDialog.getByRole("radio", {name:"90", exact:true}).count(), 0, "90 FPS is not offered for new exports");
  assert.equal(await exportDialog.getByRole("radio", {name:"60", exact:true}).getAttribute("aria-checked"), "true", "New exports default to 60 FPS");
  const hideGame = exportDialog.getByRole('switch',{name:'Hide game in background'});
  await hideGame.waitFor();
  assert.equal(await hideGame.isChecked(),true,'Game is hidden by default');
  await hideGame.click();
  assert.equal(await hideGame.isChecked(),false);
  await hideGame.click();
  assert.ok(await exportDialog.getByRole('button',{name:'Export',exact:true}).isDisabled());
  assert.notEqual(await exportDialog.getByRole('button',{name:'Export',exact:true}).evaluate(el=>getComputedStyle(el).cursor),'pointer','Disabled actions do not advertise clickability');
  for (const tools of [['Steam','CS2'], ['HLAE'], ['ffmpeg'], ['HLAE','ffmpeg']]) {
    await page.evaluate(tools => { window.missingRenderTools = tools; }, tools);
    await page.locator('.header-tools .bi-arrow-clockwise').locator('..').evaluate(el => el.click());
    await exportDialog.getByText('Missing ' + tools.join(', ') + '. Go to Settings to download.', {exact:true}).waitFor();
  }
  await exportDialog.locator('.environment-notice').click();
  await exportDialog.waitFor({state:'detached'});
  const renderDownload = page.getByRole('button', {name:'Download: HLAE',exact:true});
  await renderDownload.waitFor();
  await page.locator('.driver-popover').waitFor();
  const group = page.locator('.driver-active-element');
  assert.equal(await group.getByRole('button',{name:'Download: HLAE',exact:true}).count(),1);
  assert.equal(await group.getByRole('button',{name:'Download: FFmpeg',exact:true}).count(),1);
  assert.equal(await group.getByRole('button',{name:'Download: Source 2 Viewer CLI',exact:true}).count(),0, 'Export guidance excludes the replay tool');
  assert.equal(await group.getByRole('button',{name:'Check again',exact:true}).count(),0, 'Export guidance excludes unrelated settings actions');
  assert.equal(await page.locator('.driver-popover-title').innerText(), 'Download: HLAE, FFmpeg');
  const groupArrow = await page.locator('.driver-popover-arrow').evaluate(arrow => {
    // Simulate a downward arrow while the popover is positioned on the left.
    arrow.className = 'driver-popover-arrow driver-popover-arrow-side-top';
    arrow.style.left = '200px';
    const box = arrow.parentElement.getBoundingClientRect();
    const tip = arrow.getBoundingClientRect();
    const style = getComputedStyle(arrow);
    return { rightEdge: Math.abs(tip.left - box.right) < 1, centered: Math.abs((tip.top + tip.bottom - box.top - box.bottom) / 2) < 1, colors: [style.borderTopColor, style.borderRightColor, style.borderBottomColor, style.borderLeftColor] };
  });
  assert.deepEqual(groupArrow, {rightEdge:true, centered:true, colors:['rgba(0, 0, 0, 0)', 'rgba(0, 0, 0, 0)', 'rgba(0, 0, 0, 0)', 'rgb(255, 255, 255)']}, 'Group arrow remains on the right despite an incorrect arrow side');
  assert.deepEqual(await group.locator('[data-guide-missing="true"]').evaluateAll(els => els.map(el => getComputedStyle(el).animationIterationCount)), ['infinite', 'infinite']);
  assert.equal(await page.locator('.driver-popover-description').innerText(), 'These tools are required to export videos.');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length),0,'Guidance does not start downloads');
  await renderDownload.click();
  await page.locator('.driver-popover').waitFor({state:'detached'});
  await page.setViewportSize({width:1360,height:940});
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length),1,'Highlighted download remains clickable');
  await page.evaluate(() => window.emitTestEvent({type:'setup-log',line:'10% of 185.4 MB'}));
  await page.getByRole('dialog').getByText('10% of 185.4 MB',{exact:true}).waitFor();
  assert.equal(await page.getByRole('dialog').getByText('10% of 185.4 MB',{exact:true}).count(), 1, 'One backend progress event produces one log line');
  assert.equal(await page.getByRole('button', {name:'Download: HLAE',exact:true,includeHidden:true}).evaluate(el => getComputedStyle(el).animationName), 'none', 'Downloading closes guidance and stops pulsing');
  await page.keyboard.press('Escape');
  await page.evaluate(() => { window.toolPaths = {steamDir:'E:/Steam',cs2Exe:'E:/CS2/game/bin/win64/cs2.exe',hlaeExe:'E:/tools/HLAE.exe',hlaeDll:'E:/tools/AfxHookSource2.dll'}; });
  await page.getByRole('button', {name:'Check again',exact:true}).click();
  await page.locator('.tool-field').filter({has:page.getByLabel('HLAE.exe', {exact:true})}).getByText('Ready', {exact:true}).waitFor();
  assert.equal(await page.locator('.driver-popover').count(), 0, 'Tool updates do not restart guidance');


  const refreshList = async () => {
    await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
    await page.locator('.header-tools [aria-busy="true"]').waitFor();
    await page.locator('.header-tools [aria-busy="true"]').waitFor({state:'detached'});
  };
  await page.locator('.demo-scroll').evaluate(el=>{el.scrollTop=0});
  await page.waitForTimeout(100);
  await refreshList();
  assert.equal(await page.locator('.demo-scroll').evaluate(el=>el.scrollTop),0,'Refresh does not replay an old queue navigation');

  await page.evaluate(async () => {
    const jobs=await window.__TAURI_INTERNALS__.invoke('list_jobs');
    window.queueJobs=Array.from({length:30},(_,i)=>({...jobs[0],id:'queue-'+i,demoId:'demo-0',status:'queued',outputs:[]}));
  });
  await refreshList();
  const demoReads=()=>page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='get_demo').length);
  const closeQueue=async()=>{await page.keyboard.press('Escape');await page.locator('.queue-list').waitFor({state:'detached'});};
  let before=await demoReads();
  await page.locator('.queue-trigger').click();
  await page.locator('.queue-clips').first().getByText('Player — 3 kills · R8').waitFor();
  assert.equal(await demoReads()-before,1,'Visible jobs of one demo share one request');
  await page.locator('.queue-list').evaluate(el=>{el.scrollTop=el.scrollHeight});
  await page.locator('.queue-clips').last().getByText('Player — 3 kills · R8').waitFor();
  assert.equal(await demoReads()-before,1,'Scrolling same-demo jobs reuses titles');
  await closeQueue();

  await page.evaluate(()=>{window.queueJobs=window.queueJobs.map((j,i)=>({...j,demoId:i<15?'demo-0':'demo-999'}));});
  await refreshList();
  before=await demoReads();
  await page.locator('.queue-trigger').click();
  await page.locator('.queue-clips').first().getByText('Player — 3 kills · R8').waitFor();
  assert.equal(await demoReads()-before,1,'Off-screen demos are not fetched');
  await page.locator('.queue-list').evaluate(el=>{el.scrollTop=el.scrollHeight});
  await page.locator('.queue-clips').last().getByText('Player — 3 kills · R8').waitFor();
  assert.equal(await demoReads()-before,2,'Entering viewport fetches the other demo once');
  await closeQueue();

  await page.evaluate(()=>window.emptyParsed=true);
  await page.locator('.queue-trigger').click();
  await page.locator('.queue-clips').first().waitFor();
  await page.locator('.queue-clips').first().locator('.app-spinner').waitFor({state:'detached'});
  assert.equal(await page.locator('.queue-clips').first().locator('li').innerText(),'highlight-1','Missing parse result uses clip ID without an endless spinner');
  await closeQueue();
  await page.evaluate(()=>{window.emptyParsed=false;window.failDemo=true;});
  await page.locator('.queue-trigger').click();
  await page.locator('.queue-clips').first().getByText('Cannot read demo').waitFor();
  assert.equal(await page.locator('.queue-clips').first().locator('.app-spinner').count(),0,'Read failures finish loading');
  await closeQueue();
  await page.evaluate(()=>window.failDemo=false);
  await page.locator('.queue-trigger').click();
  await page.locator('.queue-clips').first().getByText('Player — 3 kills · R8').waitFor();
  await closeQueue();
  if (!await page.locator('.settings-page').count()) await page.getByRole('button', {name:'Settings',exact:true}).click();
  for (const [tool, label] of [['hlae','HLAE'],['ffmpeg','FFmpeg'],['vrf','Source 2 Viewer CLI']]) {
    await page.getByRole('button', {name:'Download: ' + label,exact:true}).click();
    const dialog = page.getByRole('dialog');
    await dialog.waitFor();
    assert.deepEqual(await page.evaluate(() => window.testCalls.filter(c => c.cmd === 'run_setup').at(-1).args), {tool,force:true});
    await dialog.getByRole('button', {name:'Close',exact:true}).click();
  }
  const hlaeField = page.locator('.tool-field').filter({has:page.getByLabel('HLAE.exe',{exact:true})});
  await hlaeField.getByRole('button',{name:'Browse...',exact:true}).click();
  assert.equal(await page.evaluate(() => window.testCalls.filter(c=>c.cmd==='browse_directory').at(-1).args.path), 'E:/tools/HLAE.exe');
  assert.equal(await page.evaluate(() => window.testCalls.filter(c=>c.cmd==='plugin:dialog|open').at(-1).args.options.defaultPath), 'E:/tools');
  await page.getByLabel('HLAE.exe',{exact:true}).fill('E:/custom/HLAE.exe');
  await hlaeField.getByRole('button',{name:'Browse...',exact:true}).click();
  assert.equal(await page.evaluate(() => window.testCalls.filter(c=>c.cmd==='browse_directory').at(-1).args.path), 'E:/custom/HLAE.exe');
  await checkReplayDrawing(page);
  await page.evaluate(() => localStorage.setItem('test.playerNames', JSON.stringify(['P', "Synthetic player with an intentionally very long display name"])));
  await page.reload();
  await page.locator('.demo-item').first().click();
  await page.getByRole('tab', { name: 'Charts', exact: true }).click();
  const shortNameSelector = page.getByRole('combobox', { name: 'Spray player' });
  await shortNameSelector.waitFor();
  const selectorLayout = () => page.locator('.bounded-select').evaluateAll(nodes => nodes.map(node => {
    const r = node.getBoundingClientRect(); return { x:r.x,y:r.y-node.closest('.rt-Card').getBoundingClientRect().y,width:r.width,height:r.height };
  }));
  const shortLayout = await selectorLayout();
  assert.equal(shortLayout.length,3);
  assert.ok(Math.abs(shortLayout[0].width-200)<1, 'Recoil player selector retains its fixed width');
  const checkRadarSelectors = async () => {
    const layout = await page.locator('.bounded-select').nth(1).evaluate(node => {
      const row=node.parentElement, a=node.getBoundingClientRect(), b=node.nextElementSibling.getBoundingClientRect();
      const bounds=row.getBoundingClientRect(), heading=row.previousElementSibling.getBoundingClientRect();
      return {equal:Math.abs(a.width-b.width)<1,sameRow:Math.abs(a.y-b.y)<1,below:a.y>=heading.bottom,
        fills:Math.abs(a.x-bounds.x)<1 && Math.abs(b.right-bounds.right)<1};
    });
    assert.deepEqual(layout,{equal:true,sameRow:true,below:true,fills:true},'Radar selectors share a full-width row below the heading');
  };
  await checkRadarSelectors();
  await shortNameSelector.click();
  await page.getByRole('option', { name: "Synthetic player with an intentionally very long display name", exact: true }).click();
  assert.deepEqual(await selectorLayout(),shortLayout,'Changing name length cannot move or resize selectors');
  const clippedName = await shortNameSelector.locator('.rt-SelectTriggerInner').evaluate(node => ({ clipped:node.scrollWidth>node.clientWidth,overflow:getComputedStyle(node).textOverflow }));
  assert.deepEqual(clippedName,{clipped:true,overflow:'ellipsis'});
  assert.equal(await shortNameSelector.getAttribute('title'), "Synthetic player with an intentionally very long display name");
  await page.setViewportSize({width:900,height:940});
  await checkRadarSelectors();
  assert.ok((await selectorLayout()).every(r=>r.x>=0 && r.x+r.width<=900),'Player selectors fit within narrow windows');
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await page.evaluate(()=>localStorage.removeItem('test.playerNames'));
  await page.getByRole('tab',{name:'Match anomalies',exact:true}).click();
  const scorePanel=page.getByRole('tabpanel');
  const scoreButton=scorePanel.locator('button[aria-busy]');
  await scoreButton.waitFor();
  assert.equal(await scorePanel.getByText(/upper right|Use Analyze/).count(),0,'No analysis instructions');
  assert.equal(await page.getByRole('button',{name:'Analyze',exact:true}).count(),1,'Analysis action only appears inside the anomaly tab');
  assert.equal(await page.evaluate(()=>window.scoreCalls ?? 0),0,'Opening demo and scoring tab never starts scoring');
  const scoreRow = id => scorePanel.locator('[data-player-id="'+id+'"]');
  const behaviorCount = (id,rule='aim-snap') => scoreRow(id).locator('[data-rule-count="'+rule+'"]');
  assert.equal(await scorePanel.getByRole('combobox',{name:'Player',exact:true}).count(),0,'No player selector hides the roster');
  assert.equal(await scorePanel.getByRole('table').count(),0,'No empty roster table before analysis');
  assert.equal(await scorePanel.getByRole('button',{name:'Details',exact:true}).count(),0,'No disabled details buttons without data');
  const scoringListenerBaseline=await page.evaluate(()=>window.testListenerCount());
  await page.evaluate(()=>{window.holdScore=true;window.holdQueued=true;});
  await scoreButton.click();
  await scorePanel.getByRole('status').filter({hasText:'Waiting'}).waitFor();
  await page.locator('.queue-trigger').click();
  await page.getByRole('dialog').getByRole('tab', {name:/^Anomaly analysis/}).click();
  await page.getByRole('dialog').getByText(/Waiting.*1/).waitFor();
  await page.getByRole('dialog').getByRole('button',{name:'Close',exact:true}).click();
  await page.evaluate(()=>{window.holdQueued=false;window.startScore();});
  assert.equal(await scoreButton.isDisabled(),true,'Duplicate match requests are disabled');
  await scorePanel.getByRole('status').filter({hasText:'Step 1 / 3'}).waitFor();
  await page.evaluate(()=>window.emitTestEvent({type:'analysis-job-changed',job:{...window.analysisJobs[0],id:'other-job',demoId:'another-demo',step:3}}));
  assert.ok((await scorePanel.getByRole('status').innerText()).includes('Step 1 / 3'),'Other demos do not change progress');
  await page.evaluate(()=>window.analysisStep(window.analysisJobs[0],2));
  await scorePanel.getByRole('status').filter({hasText:'Step 2 / 3'}).waitFor();
  const activeAnalysisId=await page.evaluate(()=>window.analysisJobs[0].id);
  await page.locator('.demo-item').nth(1).click();
  await page.locator('.queue-trigger').click();
  await page.getByRole('dialog').getByRole('tab', {name:/^Anomaly analysis/}).click();
  await page.getByRole('dialog').locator('[data-analysis-job="'+activeAnalysisId+'"]').getByRole('button',{name:'Open demo',exact:true}).click();
  await scorePanel.getByRole('status').filter({hasText:'Step 2 / 3'}).waitFor();
  assert.equal(await scoreButton.isDisabled(),true,'Returning to a running demo preserves its job');

  assert.equal(await scorePanel.locator('ol').count(),0,'Progress has no redundant step list');
  assert.equal(await scorePanel.getByText(/Elapsed:/).count(),0,'Progress has no elapsed timer');
  await page.getByRole('tab',{name:'Players',exact:true}).click();
  await page.getByRole('tab',{name:'Match anomalies',exact:true}).click();
  await scorePanel.getByRole('status').filter({hasText:'Step 2 / 3'}).waitFor();
  await page.evaluate(()=>window.analysisStep(window.analysisJobs[0],3));
  await scorePanel.getByRole('status').filter({hasText:'Step 3 / 3'}).waitFor();
  await page.evaluate(()=>{window.holdScore=false;window.releaseScore();});
  await page.waitForFunction(count=>window.testListenerCount()===count,scoringListenerBaseline);
  await scoreRow('1').waitFor();
  assert.equal(await scoreRow('1').locator('[data-rule-count]').count(),0,'No empty behavior labels');
  await behaviorCount('2').filter({hasText:/^1$/}).waitFor();
  assert.equal(await behaviorCount('2','aim-linear-acquisition').innerText(),'1','Independent aligned rule columns');
  assert.equal(await behaviorCount('1','aim-linear-acquisition').count(),0,'Unobserved behavior is omitted per player');
  assert.equal(await scorePanel.getByRole('columnheader').count(),3,'Only player, observed behaviors, and details');
  assert.equal(await scorePanel.getByRole('columnheader',{name:'Empty behavior',exact:true}).count(),0,'Globally empty rules are omitted');
  assert.equal(await scorePanel.locator('[data-rule-count="view-angle-oscillation"]').count(),0,'Unavailable rule is not an event column');
  assert.equal(await scorePanel.getByText('No occurrences',{exact:true}).count(),0,'No repetitive empty-result list');
  assert.equal(await scorePanel.getByText(/Counts show observed behavior|Analysis history|Source demo not verified|different demo content/).count(),0,'Main matrix has no redundant explanation or source warnings');
  assert.equal(await page.getByRole('dialog').count(),0,'Details are initially closed');
  assert.equal(await page.evaluate(()=>window.scoreCalls),1,'One request evaluates the whole match');
  await page.setViewportSize({width:1360,height:940});
  if(process.env.UI_SCREENSHOT_DIR) await scorePanel.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/scoring-matrix.png'});
  const detailsButton = id => scoreRow(id).getByRole('button',{name:'Details',exact:true});
  assert.equal(await behaviorCount('2','smoke-hit-rate').innerText(),'1','Confirmed path hits remain positive counts');
  await detailsButton('2').click();
  const analysisDialog=page.getByRole('dialog');
  await analysisDialog.getByRole('heading',{name:'Opponent · Analysis details',exact:true}).waitFor();
  assert.equal(await analysisDialog.locator('select').count(),0,'No analysis history select');
  assert.equal(await analysisDialog.getByText(/Counts show observed behavior|Analysis history|Source demo not verified|different demo content/).count(),0,'No explanation, history label, or source warnings');
  assert.equal(await analysisDialog.locator('[data-smoke-estimate]').innerText(),'Smoke hit rate (estimated): 28% · 14／50 shots','Estimate displays its own numerator and denominator');
  const smokeDetails=analysisDialog.locator('[data-check-id="smoke-hit-rate"]');
  assert.equal(await analysisDialog.locator('[data-check-id][open]').count(),0,'Every behavior starts collapsed');
  assert.equal(await analysisDialog.locator('[data-occurrence-id]:visible').count(),0,'Evidence is hidden until expanded');
  await smokeDetails.locator(':scope > summary').click();
  assert.equal(await smokeDetails.getByText(/cannot yet be calculated|paths of missed shots/).count(),0,'No unimplemented-feature notice');
  assert.equal(await smokeDetails.getByText(/ratio|%/).count(),0,'Confirmed smoke hits never fabricate an unknown percentage');
  if(process.env.UI_SCREENSHOT_DIR) await analysisDialog.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/scoring-dialog.png'});

  for (const summary of await analysisDialog.locator('[data-check-id] > summary').all()) { if (!await summary.evaluate(el=>el.parentElement.open)) await summary.click(); }
  await analysisDialog.getByText(/Ticks: 64–128/).first().waitFor();
  assert.equal(await analysisDialog.locator('[data-check-id] [data-occurrence-id]').first().evaluate(el=>el.tagName),'TR','Occurrences use compact table rows');
  const evidenceScroll=analysisDialog.locator('[data-check-id] > div').first();
  assert.equal(await evidenceScroll.evaluate(el=>getComputedStyle(el).maxHeight),'none','Evidence has no nested vertical scroll area');
  assert.equal(await analysisDialog.locator('[data-check-id] details').count(),0,'No nested source expanders');
  assert.ok(await analysisDialog.locator('[data-summary-id]').first().evaluate(el=>Boolean(el.compareDocumentPosition(document.querySelector('[data-check-id]')) & Node.DOCUMENT_POSITION_FOLLOWING)),'Summary precedes behavior details');
  await analysisDialog.getByText('Acquisition speed: 3.1235 deg/s',{exact:true}).first().waitFor();
  await analysisDialog.getByText('Acquisition speed: 4.1235 deg/s',{exact:true}).first().waitFor();
  await analysisDialog.getByText('Shots: 64',{exact:true}).first().waitFor();
  assert.equal(await analysisDialog.getByRole('heading',{name:'Data status',exact:true}).count(),0,'No unavailable-data list');
  assert.equal(await analysisDialog.getByText('Missing view data',{exact:true}).count(),0,'Unavailable rules are omitted');
  assert.equal(await analysisDialog.locator('[data-occurrence-id]').count(),4,'Original evidence stays separate per rule');
  assert.equal(await page.evaluate(()=>window.scoreCalls),1,'Opening dialog never recalculates');
  await page.keyboard.press('Escape');
  await analysisDialog.waitFor({state:'hidden'});
  assert.equal(await detailsButton('2').evaluate(el=>el===document.activeElement),true,'Escape restores trigger focus');
  await detailsButton('1').click();
  assert.equal(await analysisDialog.locator('[data-smoke-estimate]').innerText(),'Smoke hit rate (estimated): 0% · 0／50 shots','Eligible misses show a valid zero rate');
  assert.ok(await analysisDialog.locator('[data-summary-id]:visible').count()>0,'Summary is visible without expanding');
  await analysisDialog.getByText('Shots: 64',{exact:true}).first().waitFor();
  assert.equal(await analysisDialog.locator('[data-occurrence-id]').count(),0,'Player with no observations still has measured summary');
  await analysisDialog.getByRole('button',{name:'Close',exact:true}).click();
  await analysisDialog.waitFor({state:'hidden'});
  assert.equal(await scoreRow('1').getByRole('button',{name:'Export',exact:true}).count(),0,'No export action without occurrences');
  await scoreRow('2').getByRole('button',{name:'Export',exact:true}).click();
  const behaviorExport=page.getByRole('dialog');
  assert.equal(await behaviorExport.locator('button[aria-pressed]').count(),4,'Only observed rules can be exported');
  assert.equal(await behaviorExport.getByRole('button',{name:'Export',exact:true}).isDisabled(),true);
  await behaviorExport.evaluate(el=>Promise.all(el.getAnimations().map(animation=>animation.finished)));
  const exportBounds=await behaviorExport.boundingBox();
  await page.evaluate(()=>window.holdClipPreview=true);
  await behaviorExport.getByRole('button',{name:/Instant acquisition/}).click();
  await page.waitForFunction(()=>window.releaseClipPreviews?.length>0);
  const loadingBounds=await behaviorExport.boundingBox();
  assert.ok(Math.abs(loadingBounds.y-exportBounds.y)<1 && Math.abs(loadingBounds.height-exportBounds.height)<1,'Rule preview loading must not move or resize the dialog');
  await page.evaluate(()=>{window.holdClipPreview=false;window.releaseClipPreviews.splice(0).forEach(resolve=>resolve());});

  await behaviorExport.getByRole('button',{name:/Rapid straight turn/}).click();
  await page.waitForFunction(()=>document.querySelectorAll('[role=dialog] button[aria-pressed=true]').length===2);
  assert.equal(await behaviorExport.locator('[data-export-rule]').count(),0,'Selected rules do not add round or interval descriptions');
  await page.waitForFunction(()=>!document.querySelector('[role=dialog] [aria-busy=true]'));
  const settledBounds=await behaviorExport.boundingBox();
  assert.ok(Math.abs(settledBounds.y-exportBounds.y)<1 && Math.abs(settledBounds.height-exportBounds.height)<1,'Resolved rule preview must retain dialog position and height');
  const mergeRules=behaviorExport.getByRole('switch',{name:'Merge different rules',exact:true});
  assert.equal(await mergeRules.isChecked(),false,'Different rules stay separate by default');
  await page.evaluate(()=>window.failExport=true);
  await behaviorExport.getByRole('button',{name:'Export',exact:true}).click();
  await behaviorExport.getByRole('alert').filter({hasText:'Cannot queue export'}).waitFor();
  assert.equal(await behaviorExport.getByRole('button',{name:/Instant acquisition/}).getAttribute('aria-pressed'),'true','Failed enqueue retains selection');
  assert.equal(await page.evaluate(()=>window.testCalls.findLast(call=>call.cmd==='start_analysis_render').args.options.merge),false);
  await mergeRules.click();
  await page.evaluate(()=>window.failExport=false);
  await behaviorExport.getByRole('button',{name:'Export',exact:true}).click();
  await behaviorExport.waitFor({state:'hidden'});
  const exportRequest=await page.evaluate(()=>window.testCalls.findLast(call=>call.cmd==='start_analysis_render').args);
  assert.equal(exportRequest.selection.playerId,'2');
  assert.deepEqual(exportRequest.selection.ruleIds,['aim-snap','aim-linear-acquisition']);
  assert.equal(exportRequest.options.merge,true);
  await page.getByRole('tab',{name:'Match anomalies',exact:true}).click();
  await page.setViewportSize({width:760,height:940});
  assert.ok(await scorePanel.getByRole('table').evaluate(el=>el.scrollWidth<=el.clientWidth),'Behavior labels wrap without horizontal scrolling');
  await scoreButton.click();
  await page.waitForFunction(()=>window.scoreHistory && Object.values(window.scoreHistory).every(records=>records.length===1 && records[0].id==='assessment-1'));
  await detailsButton('2').click();
  assert.equal(await analysisDialog.locator('select').count(),0,'No history selector is exposed');
  assert.equal(await analysisDialog.locator('[data-check-id][open]').count(),0,'Reopening resets collapsed state');
  for (const summary of await analysisDialog.locator('[data-check-id] > summary').all()) await summary.click();
  await analysisDialog.getByText(/Ticks: 128–192/).first().waitFor();
  assert.equal(await analysisDialog.getByText(/Ticks: 64–128/).count(),0,'Dialog shows only the latest result');
  if(process.env.UI_SCREENSHOT_DIR) await analysisDialog.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/scoring-dialog.png'});
  await analysisDialog.getByRole('button',{name:'Close',exact:true}).click();
  await page.evaluate(()=>window.failWorker=true);
  await scoreButton.click();
  await scorePanel.getByRole('alert').filter({hasText:'Cannot read source demo'}).waitFor();
  await page.locator('.queue-trigger').click();
  await page.getByRole('dialog').getByRole('tab', {name:/^Anomaly analysis/}).click();
  const failedJob=page.getByRole('dialog').locator('[data-analysis-job]').filter({hasText:'Cannot read source demo'});
  await failedJob.waitFor();
  await page.evaluate(()=>window.failWorker=false);
  await failedJob.getByRole('button',{name:'Retry',exact:true}).click();
  await page.getByRole('dialog').locator('[data-analysis-job]').first().getByText('Completed',{exact:true}).waitFor();
  await page.getByRole('dialog').getByRole('button',{name:'Close',exact:true}).click();
  await page.evaluate(()=>window.failScore=true);
  await scoreButton.click();
  await scorePanel.getByRole('alert').filter({hasText:'Source demo unavailable'}).waitFor();
  await behaviorCount('2').filter({hasText:/^1$/}).waitFor();
  await page.getByRole('tab',{name:'Players',exact:true}).click();
  await page.getByRole('tab',{name:'Match anomalies',exact:true}).click();
  await behaviorCount('2').filter({hasText:/^1$/}).waitFor();
  await page.evaluate(()=>{localStorage.setItem('test.scoreHistory',JSON.stringify(window.scoreHistory));});
  await page.reload();
  await page.locator('.demo-item').first().click();
  await page.getByRole('tab',{name:'Match anomalies',exact:true}).click();
  await scoreRow('1').waitFor();
  assert.equal(await scoreRow('1').locator('[data-rule-count]').count(),0,'No empty behavior labels');
  await behaviorCount('2').filter({hasText:/^1$/}).waitFor();
  await scorePanel.getByRole('button',{name:'Reanalyze',exact:true}).waitFor();
  assert.equal(await scorePanel.getByText('Completed',{exact:true}).count(),0,'Saved results need no completed label');
  assert.equal(await scorePanel.getByRole('button',{name:'Delete data',exact:true}).count(),0,'Deletion is centralized in the demo menu');
  assert.equal(await scoreButton.evaluate(el=>getComputedStyle(el.parentElement).justifyContent),'flex-end','Saved actions align right');
  await page.locator('.demo-heading').getByRole('button',{name:'More',exact:true}).click();
  assert.deepEqual(await page.getByRole('menuitem').allTextContents(),['Show in Explorer','Delete videos','Delete anomaly data','Delete all analysis data','Delete demo file'],'Demo menu follows the requested order');
  assert.equal(await page.getByRole('menuitem',{name:'Delete videos',exact:true}).getAttribute('aria-disabled'),'true','Running videos cannot be deleted');
  await page.keyboard.press('Escape');
  await page.evaluate(async()=>{
    const jobs=await window.__TAURI_INTERNALS__.invoke('list_jobs');
    window.queueJobs=[jobs.find(j=>j.demoId==='demo-0'&&j.status==='done'),jobs.find(j=>j.demoId!=='demo-0')];
  });
  await refreshList();
  await page.locator('.demo-heading').getByRole('button',{name:'More',exact:true}).click();
  const historyReads=await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='scoring_history').length);
  await page.getByRole('menuitem',{name:'Delete videos',exact:true}).click();
  await page.waitForFunction(()=>window.queueJobs.length===1 && window.queueJobs[0].demoId!=='demo-0');
  await page.locator('.demo-heading').getByRole('button',{name:'More',exact:true}).click();
  await page.getByRole('menuitem',{name:'Delete anomaly data',exact:true}).locator(':scope:not([data-disabled])').waitFor();
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='scoring_history').length),historyReads,'Deleting videos does not reread anomaly data');
  assert.equal(await page.getByRole('menuitem',{name:'Delete anomaly data',exact:true}).getAttribute('aria-disabled'),null,'Saved analysis enables deletion');
  await page.getByRole('menuitem',{name:'Delete anomaly data',exact:true}).click();
  await scorePanel.getByRole('button',{name:'Analyze',exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>window.clearMatchCalls),1,'Deletes only match anomaly data');
  assert.equal(await scorePanel.getByRole('table').count(),0,'Deleted data disappears immediately');
  await page.locator('.demo-heading').getByRole('button',{name:'More',exact:true}).click();
  assert.equal(await page.getByRole('menuitem',{name:'Delete anomaly data',exact:true}).getAttribute('aria-disabled'),'true','No analysis disables deletion');
  await page.keyboard.press('Escape');
  assert.equal(await scoreButton.evaluate(el=>getComputedStyle(el.parentElement).justifyContent),'flex-start','Empty analysis action aligns left');
  await page.evaluate(()=>{localStorage.removeItem('test.scoreHistory');});
  assert.equal(await page.evaluate(()=>window.scoreCalls ?? 0),0,'Reloading never starts analysis');
  assert.equal(await page.evaluate(()=>window.analysisJobs.length),0,'Restart retains results but clears the analysis queue');
  await page.setViewportSize({width:760,height:940});
  const demoTabs=page.locator('.demo-tabs').getByRole('tab');
  assert.equal(await demoTabs.count(),6);
  assert.ok(await page.locator('.demo-tabs .demo-tab-label').evaluateAll(labels=>labels.every(label=>getComputedStyle(label).display==='none')),'Narrow tabs show icons only');
  assert.ok(await demoTabs.evaluateAll(tabs=>tabs.every(tab=>tab.getAttribute('aria-label')&&tab.title)),'Icon tabs retain accessible names and hover titles');
  assert.ok(await page.locator('.demo-tabs').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'Icon tabs fit without horizontal overflow');
  await demoTabs.first().click();
  await demoTabs.first().press('End');
  await page.waitForFunction(()=>document.querySelector('.demo-tabs [role=tab]:last-child').getAttribute('aria-selected')==='true');
  await page.setViewportSize({width:2200,height:940});
  assert.ok(await page.locator('.demo-tabs .demo-tab-label').evaluateAll(labels=>labels.every(label=>getComputedStyle(label).display!=='none')),'Wide tabs restore their labels');
  assert.deepEqual(errors,[]);
  console.log('UI checks passed: virtual lists, lazy demo reads, content-sized tabs, themes, charts, filters, queue jump, settings.');
} finally { await browser.close(); await new Promise(resolve => server.httpServer.close(resolve)); }
