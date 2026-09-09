import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { preview } from 'vite';
import { initialAppearance, THEMES } from '../src/themes.ts';
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
    const demos = Array.from({ length: 1000 }, (_, i) => ({ id: 'demo-' + i, name: 'match_' + i + '.dem', path: 'E:/replays/' + i + '.dem', bytes: 214800000, mtimeMs: new Date(2026, 8, 8, 17, i % 60).getTime(), status: 'parsed', mapName: i % 2 ? 'Inferno' : 'Mirage', summary: { rounds: 22, kills: 100, highlights: 1, scoreA: 13, scoreB: 9, players: ['Player'] } }));
    const options = { width: 1920, height: 1080, fps: 60, codec: 'h264', hud: true, crosshair: true, radar: true, killFeed: true, viewmodel: true, tracers: true, maxSizeMb: 20, trueView: true };
    const jobs = Array.from({ length: 100 }, (_, i) => ({ id: 'job-' + i, demoId: i === 1 ? 'demo-999' : 'demo-0', highlightIds: ['highlight-1'], options, status: i === 0 ? 'running' : i === 1 ? 'queued' : 'done', stage: i === 0 ? 'recording 1/2' : '', createdAt: '2026-09-08T17:30:00Z', startedAt: '2026-09-08T17:30:00Z', finishedAt: i > 1 ? '2026-09-08T17:32:18Z' : undefined, outputs: i < 2 ? [] : Array.from({length:i===2?3:1},(_,j)=>({ file: 'E:/clips/' + i + '-' + j + '.mp4', bytes: 18400000, title: 'Round 08', highlightId: 'highlight-1', isFinal: j===2 })), log: ['recording'] }));
    const player = {steamid:'1',name:'Player',team:'A',kills:20,deaths:10,assists:3,headshots:10,headshotPct:50,kd:2,multiKills:{'2k':2,'3k':1,'4k':0,'5k':0},clutchesWon:1,damage:2000,utilityDamage:30,friendlyDamage:0,adr:90,highlights:1,bestScore:8};
    const status = { ok:true,problems:[],dataDir:'E:/data',activeRender:'job-0',version:'test' };
    window.testCalls = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, convertFileSrc: () => 'data:video/mp4;base64,', invoke: async (cmd, args) => {
      window.testCalls.push({cmd,args});
      if(cmd==='get_startup_error')return null;
      if(cmd==='parse_demo')return new Promise(resolve=>window.releaseParse=resolve);
      if(cmd==='get_status')return window.missingTools ? {...status,ok:false} : status;
      if(cmd==='get_settings')return { settings:{language:'en',replayFolders:[],scanGameReplays:true},doctor:{ok:true,problems:[],paths:{}},detected:{},setup:{running:false,log:[]},dataDir:'E:/data',defaultDataDir:'E:/data',parsedBytes:0,clipsBytes:0,radarBytes:0 };
      if(cmd==='list_demos'){if(window.holdRefresh)await new Promise(resolve=>window.releaseRefresh=resolve);return demos;}
      if(cmd==='list_jobs')return window.queueJobs ?? jobs;
      if(cmd==='get_demo' && window.emptyParsed)return {meta:demos.find(d=>d.id===args.id)};
      if(cmd==='get_demo' && window.failDemo)throw Error('Cannot read demo');
      if(cmd==='get_demo' && window.holdDemo)await new Promise(resolve=>window.releaseDemo=resolve);
      if(cmd==='get_demo')return { meta:demos.find(d=>d.id===args.id),parsed:{info:{mapName:demos.find(d=>d.id===args.id).mapName,tickRate:64,players:[]},parsedAt:'2026-09-08',score:{A:13,B:9},rounds:[],roundSummaries:[{round:1,winner:'A',killsA:5,killsB:2}],stats:[player,{...player,steamid:'2',name:'Opponent',team:'B'}],highlights:[{id:'highlight-1',player:{steamid:'1',name:'Player'},round:8,startTick:640,endTick:1280,score:8,tags:['3k'],title:'Player — 3 kills · R8',kills:[],breakdown:{}}]}};
      if(cmd.startsWith('plugin:event|'))return 1;
      throw Error('Unexpected command '+cmd);
    }};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener:()=>{} };
  });
  await page.goto(server.resolvedUrls.local[0]);
  await page.locator('.demo-item').first().waitFor();
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
  const widths = await page.getByRole('tab').evaluateAll(t=>t.map(e=>e.getBoundingClientRect().width));
  assert.ok(Math.max(...widths)-Math.min(...widths)<2,'Tabs are equal width');
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-dark.png'});
  await page.getByRole('button',{name:'Switch to light mode'}).click();
  assert.match(await page.locator('.radix-themes').first().getAttribute('class'),/light/);
  assert.equal(await page.evaluate(()=>localStorage.getItem('demodesk.appearance')), 'light');
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/ui-light.png'});
  const about = page.getByRole('button', { name: 'About', exact: true });
  assert.equal(await about.innerText(), '', 'About is an icon-only button');
  const aboutBounds = await about.boundingBox();
  const settingsBounds = await page.getByRole('button', { name: 'Settings', exact: true }).boundingBox();
  assert.ok(aboutBounds.x + aboutBounds.width + 7 <= settingsBounds.x, 'About and Settings hit areas stay separated');
  await about.click();
  await page.getByRole('dialog').waitFor();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('tab').filter({hasText:'Charts'}).click();
  await page.locator('canvas').first().waitFor();
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
  assert.equal(await settingsPage.locator('.rt-Badge').count(), 0, 'Settings statuses are plain text');
  assert.equal(await settingsPage.locator('.rt-variant-ghost, .rt-variant-soft').count(), 0, 'Settings actions use clear outlined or solid controls');
  const actionHeights = await settingsPage.locator('.rt-Button, .rt-IconButton').evaluateAll(buttons => buttons.map(b => b.getBoundingClientRect().height));
  assert.ok(actionHeights.every(height => height === 32), 'Settings action controls share a 32px height');
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
      probe.style.color = 'var(--app-text)';
      el.append(probe);
      const expected = getComputedStyle(probe).color;
      probe.remove();
      return {
        background: style.backgroundColor, expected,
        foreground: getComputedStyle(text).color,
        arrow: getComputedStyle(el.querySelector('.rt-TooltipArrow')).fill,
        onTop: el.contains(document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)),
      };
    });
    assert.equal(tip.background, tip.expected, 'Tooltip retains its contrasting background');
    assert.equal(tip.arrow, tip.background, 'Tooltip arrow matches its surface');
    assert.notEqual(tip.foreground, tip.background, 'Tooltip text remains readable');
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
  assert.equal(await spinner.evaluate(e=>getComputedStyle(e).animationName),'none','Reduced motion stays static');
  await page.waitForFunction(()=>typeof window.releaseRefresh==='function');
  await page.evaluate(()=>{window.holdRefresh=false;window.releaseRefresh()});
  await spinner.waitFor({state:'detached'});
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.evaluate(()=>window.missingTools=true);
  await page.locator('.header-tools .bi-arrow-clockwise').locator('..').click();
  await page.locator('.environment-notice').click();
  const download = page.locator('.tools-highlight');
  await download.waitFor();
  assert.ok(await download.evaluate(el => document.activeElement === el), 'Missing tools guidance focuses download action');
  assert.ok(await download.evaluate(el => {
    const r = el.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= innerHeight;
  }), 'Download action is scrolled into view');
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.cmd==='run_setup').length), 0, 'Guidance does not start downloads');
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
  assert.deepEqual(errors,[]);
  console.log('UI checks passed: virtual lists, lazy demo reads, equal tabs, themes, charts, filters, queue jump, settings.');
} finally { await browser.close(); await new Promise(resolve => server.httpServer.close(resolve)); }
