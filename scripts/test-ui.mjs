import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { preview } from 'vite';
import { initialAppearance, THEMES } from '../src/themes.ts';
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
    const jobs = Array.from({ length: 100 }, (_, i) => ({ id: 'job-' + i, demoId: i === 1 ? 'demo-999' : 'demo-0', highlightIds: ['highlight-1'], options, status: i === 0 ? 'running' : i === 1 ? 'queued' : 'done', stage: i === 0 ? 'recording 1/2' : '', createdAt: '2026-09-08T17:30:00Z', startedAt: '2026-09-08T17:30:00Z', finishedAt: i > 1 ? '2026-09-08T17:32:18Z' : undefined, outputs: i < 2 ? [] : Array.from({length:i===2?3:1},(_,j)=>({ file: 'E:/clips/' + i + '-' + j + '.mp4', bytes: 18400000, title: 'Round 08', highlightId: 'highlight-1', isFinal: j===2 })), log: ['recording'] }));
    const player = {steamid:'1',name:'Player',team:'A',kills:20,deaths:10,assists:3,openingKills:4,openingDeaths:2,flashAssists:1,roundsPlayed:22,roundsSurvived:12,kast:77.3,tradeKills:2,tradedDeaths:1,heDamage:20,fireDamage:10,opponents:{'2':3},aim:{all:{shots:100,hits:25,headHits:5,headEligibleHits:20,firstShots:10,firstHits:4,sprayShots:30,sprayHits:9}},activity:{shots:100,flashes:2,smokes:3,hes:2,fires:1,enemiesFlashed:3,teammatesFlashed:1,enemyBlindSeconds:7},clutches:[{round:8,side:'CT',versus:2,kills:2,outcome:'won'}],headshots:10,headshotPct:50,kd:2,multiKills:{'2k':2,'3k':1,'4k':0,'5k':0},clutchesWon:1,damage:2000,utilityDamage:30,friendlyDamage:0,adr:90,highlights:1,bestScore:8};
    player.recoil = {ak47:[{x:0,y:0,samples:2},{x:-1,y:-2,samples:2},{x:1,y:-4,samples:2},{x:2,y:-5,samples:1}]};
    const status = { missingRenderTools:[],ok:true,problems:[],dataDir:'E:/data',activeRender:'job-0',version:'test' };
    window.testCalls = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, convertFileSrc: () => 'data:video/mp4;base64,', invoke: async (cmd, args) => {
      window.testCalls.push({cmd,args});
      if(cmd==='get_startup_error')return null;
      if(cmd==='get_map_assets' && window.missingTools)throw new Error('Source 2 Viewer CLI not installed');
      if(cmd==='get_replay' || cmd==='get_map_assets')return new Promise(()=>{});
      if(cmd==='get_kills')return [];
      if(cmd==='parse_demo')return new Promise(resolve=>window.releaseParse=resolve);
      if(cmd==='browse_directory')return args.path ? 'E:/tools' : 'E:/Desktop';
      if(cmd==='plugin:dialog|open')return null;
      if(cmd==='run_setup')return true;
      if(cmd==='check_for_updates'){if(window.failUpdate)throw new Error('offline');return {status:'available',version:'1.0.10'};}
      if(cmd==='open_url'){window.openedUrl=args.url;return;}
      if(cmd==='get_status')return window.missingTools ? {...status,ok:false,missingRenderTools:window.missingRenderTools ?? ['HLAE','ffmpeg']} : status;
      if(cmd==='get_settings')return { settings:{language:'en',replayFolders:[],scanGameReplays:true},doctor:{ok:true,problems:[],paths:window.toolPaths || {}},detected:{},setup:{running:false,log:[]},dataDir:'E:/data',defaultDataDir:'E:/data',parsedBytes:0,clipsBytes:0,radarBytes:0 };
      if(cmd==='list_demos'){if(window.holdRefresh)await new Promise(resolve=>window.releaseRefresh=resolve);return demos;}
      if(cmd==='list_jobs')return window.queueJobs ?? jobs;
      if(cmd==='get_demo' && window.emptyParsed)return {meta:demos.find(d=>d.id===args.id)};
      if(cmd==='get_demo' && window.failDemo)throw Error('Cannot read demo');
      if(cmd==='get_demo' && window.holdDemo)await new Promise(resolve=>window.releaseDemo=resolve);
      if(cmd==='get_demo')return { meta:demos.find(d=>d.id===args.id),parsed:{recoilReference:{ak47:[{x:0,y:0,samples:4},{x:-1,y:-1,samples:4},{x:-2,y:-3,samples:4},{x:-2,y:-4,samples:2}]},info:{mapName:demos.find(d=>d.id===args.id).mapName,tickRate:64,players:[]},parsedAt:'2026-09-08',score:{A:13,B:9},rounds:[],roundSummaries:[{round:1,winner:'A',killsA:5,killsB:2,players:{'1':{kills:5,deaths:2,damage:450,awp:1,flashed:2,cash:800},'2':{kills:2,deaths:5,damage:220,awp:0,flashed:0,cash:300}}}],stats:[player,{...player,steamid:'2',name:'Player',team:'B',recoil:{},opponents:{'1':1}}],highlights:[{id:'highlight-1',player:{steamid:'1',name:'Player'},round:8,startTick:640,endTick:1280,score:8,tags:['3k'],title:'Player — 3 kills · R8',kills:[],breakdown:{}}]}};
      if(cmd.startsWith('plugin:event|'))return 1;
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
  await page.locator('.job-card').first().waitFor();
  assert.ok(await page.locator('.job-card').count()<15,'Job DOM bounded by viewport');
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
  const widths = await page.getByRole('tab').evaluateAll(t=>t.map(e=>e.getBoundingClientRect().width));
  assert.ok(Math.max(...widths)-Math.min(...widths)<2,'Tabs are equal width');
  const plainCursors = await page.locator('button:not(:disabled):not([data-disabled]):not([aria-disabled="true"])').evaluateAll(buttons => buttons
    .filter(button => getComputedStyle(button).cursor !== 'pointer')
    .map(button => button.getAttribute('aria-label') || button.textContent.trim()));
  assert.deepEqual(plainCursors, [], 'Enabled buttons and tabs indicate clickability');

  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-dark.png'});
  await page.getByRole('button',{name:'Switch to light mode'}).click();
  assert.match(await page.locator('.radix-themes').first().getAttribute('class'),/light/);
  assert.equal(await page.evaluate(()=>localStorage.getItem('demodesk.appearance')), 'light');
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-light.png'});
  const about = page.getByRole('button', { name: 'About: Version 1.0.10 available', exact: true });
  assert.equal(await about.innerText(), '', 'About is an icon-only button');
  const aboutBounds = await about.boundingBox();
  const settingsBounds = await page.getByRole('button', { name: 'Settings', exact: true }).boundingBox();
  assert.ok(settingsBounds.x + settingsBounds.width + 7 <= aboutBounds.x, 'About and Settings hit areas stay separated');
  await about.click();
  await page.getByRole('dialog').waitFor();
  const storeLink = page.getByRole('dialog').getByRole('button', {name:'Microsoft Store',exact:true});
  const sourceBoxes = await page.getByRole('dialog').evaluate(el => {
    const rect = name => { const r = [...el.querySelectorAll('button')].find(b => b.textContent.trim() === name).getBoundingClientRect(); return { y:r.y, height:r.height }; };
    return {github:rect('GitHub'),store:rect('Microsoft Store')};
  });
  assert.equal(sourceBoxes.github.y, sourceBoxes.store.y, 'Distribution links share a row');
  assert.equal(sourceBoxes.github.height, sourceBoxes.store.height, 'Distribution links have equal prominence');
  await storeLink.click();
  assert.equal(await page.evaluate(() => window.openedUrl), 'https://apps.microsoft.com/detail/9N5G4VXSDGS5');
  const updateNotice = page.getByText('Version 1.0.10 available', { exact: true });
  await updateNotice.waitFor();
  await updateNotice.locator('..').getByRole('button').click();
  assert.equal(await page.evaluate(() => window.openedUrl), 'https://github.com/noih/cs-demodesk/releases/latest');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  const updateCount = () => page.evaluate(() => window.testCalls.filter(c => c.cmd === 'check_for_updates').length);
  const initialChecks = await updateCount();
  await page.evaluate(() => window.failUpdate = true);
  await about.click();
  await page.waitForFunction(n => window.testCalls.filter(c => c.cmd === 'check_for_updates').length === n + 1, initialChecks);
  await page.getByRole('dialog').getByRole('button', {name:'Close',exact:true}).click();
  assert.equal(await updateCount(), initialChecks + 1, 'Closing About does not check again');
  await page.evaluate(() => window.failUpdate = false);
  await page.getByRole('button', {name:'About',exact:true}).click();
  await page.getByText('Version 1.0.10 available', {exact:true}).waitFor();
  assert.equal(await updateCount(), initialChecks + 2, 'Reopening About retries after failure');
  await page.getByRole('dialog').getByRole('button', {name:'Close',exact:true}).click();
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
  const resetZoom = recoil.getByRole('button',{name:'Reset zoom (reference)',exact:true});
  assert.ok(!(await zoomOut.isDisabled()));
  const checkPlot = async (zoom, playerPoint = false) => {
    // Check known shot positions, without depending on tooltip/label rasterization.
    await page.waitForFunction(({ zoom, accent, weapons, playerPoint }) => {
      const canvas = document.querySelector('[data-testid="recoil-chart"] canvas');
      const plots = [weapons.ak47,weapons.m4a1,weapons.m4a1_silencer].map(points => {
        const xs = points.map(p=>p.x), ys = points.map(p=>p.y);
        return { points, cx: (Math.min(0,...xs)+Math.max(0,...xs))/2, cy: (Math.min(0,...ys)+Math.max(0,...ys))/2 };
      });
      const halfSpan = Math.max(3,...plots.flatMap(p=>p.points.map(v=>Math.max(Math.abs(v.x-p.cx),Math.abs(v.y-p.cy))*1.2))) / (zoom*.8);
      const {cx,cy,points} = plots[0];
      const inset = Math.ceil(parseFloat(getComputedStyle(canvas).getPropertyValue('--app-font-caption')) * 3 + 8);
      const point = playerPoint ? { x: 2, y: -5 } : points.reduce((best,p)=>Math.hypot(p.x-cx,p.y-cy)<Math.hypot(best.x-cx,best.y-cy)?p:best);
      return [[point.x, point.y]].every(([px, py]) => {
        const x = (inset + (px - cx + halfSpan) / (2 * halfSpan) * (canvas.clientWidth - inset - 20)) * canvas.width / canvas.clientWidth;
        const y = (20 + (halfSpan - (py - cy)) / (2 * halfSpan) * (canvas.clientHeight - inset - 20)) * canvas.height / canvas.clientHeight;
        const pixels = canvas.getContext('2d').getImageData(Math.round(x)-3, Math.round(y)-3, 7, 7).data;
        return Array.from({length:49},(_,n)=>n*4).some(offset=>accent.every((value,i)=>Math.abs(pixels[offset+i]-value)<5));
      });
    }, { zoom, playerPoint, weapons: calibration.weapons, accent: (playerPoint ? THEMES.light.players.split(',')[7] : THEMES.light.accent).match(/[a-f0-9]{2}/gi).map(v => parseInt(v, 16)) });
  };
  await checkPlot(1);
  const playShots = recoil.getByRole('button', { name: 'Play AK-47', exact: true });
  assert.ok(await recoil.getByRole('button', { name: 'Play M4A4', exact: true }).isDisabled());
  assert.ok(await recoil.getByRole('button', { name: 'Play M4A1-S', exact: true }).isDisabled());
  await page.clock.install();
  await page.clock.pauseAt(new Date());
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
  await checkPlot(1); // The full fixed reference remains visible during playback.
  await page.clock.runFor(50);
  const settledShot = await recoilCanvases();
  assert.ok(settledShot[0] !== firstShot[0], 'The newly fired point settles to its normal size');
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
  await page.waitForFunction(()=>document.querySelector('[aria-label="Reset zoom (reference)"]').textContent==='125%');
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
  assert.equal(await resetZoom.innerText(),'125%');
  await checkPlot(1.25);
  await zoomOut.click();
  assert.equal(await resetZoom.innerText(),'100%');
  await checkPlot(1);
  for(let step=0;step<4;step++) await zoomIn.click();
  assert.equal(await resetZoom.innerText(),'200%');
  await checkPlot(2);
  assert.ok(await zoomIn.isDisabled());
  for(let step=0;step<7;step++) await zoomOut.click();
  assert.equal(await resetZoom.innerText(),'25%');
  await checkPlot(0.25);
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
  assert.equal(await resetZoom.innerText(),'100%','Changing player fits the new trajectories');
  assert.ok(!(await resetZoom.isDisabled()),'Reference remains available without player bursts');
  await recoil.getByText('━ Actual',{exact:true}).first().waitFor();
  await page.setViewportSize({width:1360,height:940});
  await page.getByRole('button',{name:'Switch to dark mode'}).click();
  await page.getByRole('button',{name:'Filter demos'}).click();
  await page.getByRole('textbox').fill('   ');
  assert.equal(await page.locator('.filter-trigger').getAttribute('data-filtered'), 'false');
  await page.getByRole('textbox').fill('no matches');
  assert.equal(await page.locator('.filter-trigger').getAttribute('data-filtered'), 'true');
  assert.equal(await page.locator('.filter-trigger .bi-funnel-fill').count(), 1);
  await page.keyboard.press('Escape');
  await page.locator('.queue-trigger').click();

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
  await page.getByRole('button',{name:'Missing Source 2 Viewer. Go to Settings to download.'}).click();
  const download = page.getByRole('button', {name:'Download: Source 2 Viewer CLI',exact:true});
  await download.waitFor();
  assert.deepEqual(await page.locator('.tools-highlight').evaluateAll(els => els.map(el => el.getAttribute('aria-label'))), ['Download: Source 2 Viewer CLI']);
  assert.equal(await download.evaluate(el=>getComputedStyle(el).animationName),'tools-highlight-pulse');
  assert.equal(await download.evaluate(el=>getComputedStyle(el).animationDuration),'0.5s');
  const outline = await download.evaluate(el=>getComputedStyle(el).backgroundColor);
  await page.waitForFunction(original=>getComputedStyle(document.querySelector('.tools-highlight')).backgroundColor!==original,outline);
  await page.emulateMedia({reducedMotion:'reduce'});
  assert.equal(await download.evaluate(el=>getComputedStyle(el).animationDuration),'0.5s');
  await page.emulateMedia({reducedMotion:'no-preference'});
  assert.ok(await download.evaluate(el => document.activeElement === el), 'Missing tools guidance focuses download action');
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Automatic guidance focus does not open a tooltip');
  assert.ok(await download.evaluate(el => {
    const r = el.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= innerHeight;
  }), 'Download action is scrolled into view');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length), 0, 'Guidance does not start downloads');
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('tab').filter({hasText:'Highlights'}).click();
  await page.getByRole('button',{name:'Select all',exact:true}).click();
  await page.getByRole('button',{name:/^Export/}).click();
  const exportDialog = page.getByRole('dialog');
  await exportDialog.waitFor();
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
  assert.deepEqual(await page.locator('.tools-highlight').evaluateAll(els => els.map(el => el.getAttribute('aria-label'))), ['Download: HLAE', 'Download: FFmpeg']);
  assert.ok(await renderDownload.evaluate(el=>document.activeElement===el),'Export guidance focuses the download action');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length),0,'Export guidance does not start downloads');
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Export guidance does not open a tooltip');
  await page.evaluate(() => { window.toolPaths = {steamDir:'E:/Steam',cs2Exe:'E:/CS2/game/bin/win64/cs2.exe',hlaeExe:'E:/tools/HLAE.exe',hlaeDll:'E:/tools/AfxHookSource2.dll'}; });
  await page.getByRole('button', {name:'Check again',exact:true}).click();
  await page.locator('.tool-field').filter({has:page.getByLabel('HLAE.exe', {exact:true})}).getByText('Ready', {exact:true}).waitFor();
  assert.deepEqual(await page.locator('.tools-highlight').evaluateAll(els => els.map(el => el.getAttribute('aria-label'))), ['Download: FFmpeg'], 'Ready HLAE stops highlighting immediately');


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
  assert.deepEqual(errors,[]);
  console.log('UI checks passed: virtual lists, lazy demo reads, equal tabs, themes, charts, filters, queue jump, settings.');
} finally { await browser.close(); await new Promise(resolve => server.httpServer.close(resolve)); }
