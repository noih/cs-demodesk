import { readFile, readdir, mkdir, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';
import { preview } from 'vite';

// Run after npm run build. Reads local caches without changing application data.
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const source = process.argv[2] || path.join(process.env.LOCALAPPDATA, 'dev.noih.demodesk/demodesk-data/parsed');
const output = path.resolve('out/store-screenshots');
const locales = ['en', 'zh-TW', 'zh-CN', 'ja', 'ko', 'ru'];
const matches = [];
for (const file of (await readdir(source)).filter(f => /^[^.]+\.json$/.test(f))) {
  const data = JSON.parse(await readFile(path.join(source, file), 'utf8'));
  if (data.stats?.length === 10 && data.roundSummaries?.length) {
    let sourceStat;
    try { sourceStat = await stat(data.info.path); } catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    data.screenshotBytes = sourceStat.size;
    data.screenshotCreatedMs = sourceStat.birthtimeMs || sourceStat.mtimeMs;
    data.screenshotMtimeMs = sourceStat.mtimeMs;
    data.info.serverName = 'Community match';
    matches.push(data);
  }
}
assert(matches.length, 'No complete parsed matches found');
const aliases = ['Falcon', 'Nova', 'Echo', 'Orion', 'Atlas', 'Comet', 'Lynx', 'Raven', 'Vega', 'Frost'];
function anonymize(data) {
  const identities = new Map(data.stats.map((p,i)=>[p.steamid,{steamid:String(i+1),name:aliases[i]}]));
  function clean(value) {
    if (typeof value === 'string') {
      if (/^[A-Za-z]:[\\/]/.test(value) || value.startsWith('\\\\')) return 'demo.dem';
      return value.replace(/7656119\d{10}/g, id=>identities.get(id)?.steamid ?? '0');
    }
    if (Array.isArray(value)) return value.map(clean);
    if (value && typeof value === 'object') {
      const result = Object.fromEntries(Object.entries(value).map(([k,v])=>[identities.get(k)?.steamid ?? k,clean(v)]));
      if ('steamid' in value && 'name' in value) Object.assign(result,identities.get(value.steamid) ?? {steamid:'0',name:'Anonymous'});
      if (typeof value.title === 'string' && value.player) {
        const prefix = value.player.name + ' — ';
        assert(value.title.startsWith(prefix), 'Unexpected highlight title format');
        result.title = result.player.name + ' — ' + value.title.slice(prefix.length);
      }
      return result;
    }
    return value;
  }
  return clean(data);
}
const probe = anonymize({stats:[{steamid:'test-player',name:'a'}],info:{mapName:'de_ancient'},kills:[{weapon:'ak47'}],highlights:[{player:{steamid:'test-player',name:'a'},title:'a — 3K'}]});
assert.equal(probe.info.mapName,'de_ancient');
assert.equal(probe.kills[0].weapon,'ak47');
assert.equal(probe.stats[0].name,'Falcon');
assert.equal(probe.highlights[0].title,'Falcon — 3K');

const parsed = matches.map(anonymize);
// Give matches stable anonymous names; the app applies its normal date sorting.
parsed.sort((a, b) => b.highlights.length - a.highlights.length);
const demos = parsed.map((d, i) => ({id:String(i), name:`Match-${String(i+1).padStart(2,'0')}.dem`, path:`D:/Demos/Match-${i+1}.dem`, bytes:d.screenshotBytes,
  createdMs:d.screenshotCreatedMs,mtimeMs:d.screenshotMtimeMs,status:'parsed',mapName:d.info.mapName,parsedAt:d.parsedAt,
  summary:{rounds:d.roundSummaries.length,kills:d.kills.length,highlights:d.highlights.length,scoreA:d.score.A,scoreB:d.score.B,players:d.stats.map(p=>p.name)}}));
const server = await preview({preview:{host:'127.0.0.1',port:0}});
const browser = await chromium.launch({channel:'chrome',headless:true});
const appVersion = JSON.parse(await readFile('package.json','utf8')).version;
const report = [];
try {
  for (const locale of locales) {
    const strings = JSON.parse(await readFile(new URL('../src/i18n/locales/'+locale+'.json',import.meta.url),'utf8'));
    const directory = path.join(output, locale);
    await mkdir(directory, {recursive:true});
    const page = await browser.newPage({viewport:{width:1920,height:1080},deviceScaleFactor:1,locale});
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(({parsed,demos,locale,appVersion}) => {
      localStorage.setItem('demodesk.appearance','dark');
      if (window.name === 'light' || window.name === 'dark') {
        const getItem = Storage.prototype.getItem;
        Storage.prototype.getItem = function(key) { return key === 'demodesk.appearance' ? window.name : getItem.call(this,key); };
      }
      const status = {ok:true,missingRenderTools:[],problems:[],dataDir:'D:/DemoDesk',version:appVersion};
      window.__TAURI_INTERNALS__ = {transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
        if(cmd==='get_startup_error')return null;
        if(cmd==='get_status')return status;
        if(cmd==='check_for_updates')return {status:'packaged'};
        if(cmd==='get_settings')return {settings:{language:locale},doctor:{ok:true,problems:[],paths:{}},setup:{running:false,log:[]}};
        if(cmd==='list_demos')return demos;
        if(cmd==='list_jobs')return [];
        if(cmd==='get_demo')return {meta:demos.find(d=>d.id===args.id),parsed:parsed[Number(args.id)]};
        if(cmd==='get_kills')return parsed[Number(args.id)].kills;
        if(cmd.startsWith('plugin:event|'))return 1;
        throw Error('Unexpected screenshot command: '+cmd);
      }};
      window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
    }, {parsed,demos,locale,appVersion});
    await page.goto(server.resolvedUrls.local[0]);
    await page.locator('.demo-item').first().click();
    const tabs=page.locator('.demo-tabs [role=tab]');
    await tabs.first().waitFor();
    await page.evaluate(()=>document.fonts.ready);
    async function capture(name) {
      await page.mouse.move(1910,1070);
      await page.waitForTimeout(1100);
      assert.deepEqual(errors, [], 'Browser errors');
      const file=path.join(directory,name+'.png');
      const buffer=await page.screenshot({path:file});
      assert.equal(buffer.readUInt32BE(16),1920);
      assert.equal(buffer.readUInt32BE(20),1080);
      assert(buffer.length<50*1024*1024);
      report.push({locale,file:path.relative(output,file),width:1920,height:1080,bytes:buffer.length});
    }
    await tabs.nth(0).click(); await capture('01-player-statistics');
    await tabs.nth(1).click(); await capture('02-round-trends');
    await page.getByRole('heading',{name:strings.recoil.title,exact:true}).evaluate(el=>{const card=el.closest('.rt-Card');const scroller=el.closest('.tab-body');scroller.scrollTop+=card.getBoundingClientRect().top-scroller.getBoundingClientRect().top;}); await capture('03-combat-analysis');
    await tabs.nth(2).click(); await page.locator('.tab-body').evaluate(el=>el.scrollTop=0); await capture('04-highlights');
    if (locale === 'en') {
      await tabs.nth(0).click();
      await page.getByRole('button',{name:'Switch to light mode',exact:true}).click();
      await page.waitForTimeout(1000);
      assert.equal(await tabs.first().evaluate(el=>getComputedStyle(el).backgroundColor), 'rgb(236, 239, 240)', 'Light active tab keeps its light background');
      await page.mouse.move(1910,1070);
      await mkdir(path.join(output,'_sources'),{recursive:true});
      await page.screenshot({path:path.join(output,'_sources/light.png')});
      await page.setContent(`<style>html,body{margin:0;overflow:hidden}iframe{position:absolute;inset:0;width:1920px;height:1080px;border:0}.light{clip-path:polygon(60% 0,100% 0,100% 100%,40% 100%)}.divider{position:absolute;inset:0;background:#eab83d;clip-path:polygon(59.9% 0,60.1% 0,40.1% 100%,39.9% 100%);pointer-events:none}</style><iframe name="dark" src="${server.resolvedUrls.local[0]}"></iframe><iframe class="light" name="light" src="${server.resolvedUrls.local[0]}"></iframe><div class="divider"></div>`);
      for (const name of ['dark','light']) {
        const frame = page.frameLocator(`iframe[name="${name}"]`);
        await frame.locator('.demo-item').first().evaluate(el=>el.click());
        await frame.locator('.demo-tabs [role=tab]').first().evaluate(el=>el.click());
        await frame.locator('.rt-TableRoot').first().waitFor();
      }
      await page.waitForTimeout(1200);
      await mkdir(path.join(output,'promotional'),{recursive:true});
      await capture('../promotional/05-dark-light-comparison-en');
    }
    await page.close();
    console.log(locale+' complete');
  }
  await writeFile(path.join(output,'manifest.json'),JSON.stringify(report,null,2)+'\n');
  console.log(`${report.length} PNG files validated in ${output}`);
} finally { await browser.close(); await server.close(); }
