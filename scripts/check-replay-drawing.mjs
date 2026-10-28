import assert from 'node:assert/strict';
import { layout } from '../src/replay/draw.ts';

export async function checkReplayDrawing(page) {
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const rounds = [1, 2].map((round, i) => ({round, startTick:i * 6400, freezeEndTick:i * 6400 + 128, endTick:i * 6400 + 6200, officiallyEndedTick:(i + 1) * 6400 - 1, roster:{}}));
    const players = Array.from({length:10}, (_,i) => ({steamid:`replay-${i}`,name:`Player ${i + 1}`}));
    players[0].name = 'Player with a very long display name';
    rounds.forEach(r => { r.roster = Object.fromEntries(players.map((p,i)=>[p.steamid,i < 5 ? 'CT' : 'TERRORIST'])); });
    const rows = players.map((_,i) => [i,200+i*55,-240,50,0,100,100,i===5?19:3,i===5?5:1,2650,1,2,3,3,4,...(i===5?[5]:[])]);
    const data = {schemaVersion:7,tickRate:64,step:4,firstTick:0,lastTick:12799,players,weapons:['','AK-47','USP-S','Flashbang','Smoke Grenade','C4 Explosive'],events:[],frames:[{t:0,p:rows,g:[],d:[[90,4,400,-500,50],[91,1,600,-500,50]]},{t:12799,p:rows,g:[],d:[]}]};
    const radar = document.createElement('canvas');
    radar.width = radar.height = 1024;
    radar.getContext('2d').fillStyle = '#263440';
    radar.getContext('2d').fillRect(0, 0, 1024, 1024);
    const radarUrl = radar.toDataURL();
    window.__TAURI_INTERNALS__.convertFileSrc = path => path === 'drawing-replay'
      ? 'data:application/json,' + encodeURIComponent(JSON.stringify(data))
      : radarUrl;
    window.__TAURI_INTERNALS__.invoke = async (cmd,args) => {
      if (cmd === 'get_kills') return [false, true, undefined].map(flash => ({
        tick: 0, round: 1, attacker: {...players[0], team:'CT'}, victim: {...players[5], team:'TERRORIST'},
        assister: flash === undefined ? undefined : {...players[1], team:'CT'},
        weapon:'ak47', headshot:flash === false, assistedFlash:flash ?? false,
      }));
      if (cmd === 'get_replay') return {path:'drawing-replay',bytes:100};
      if (cmd === 'get_map_assets') return {posX:0,posY:0,scale:1,layers:['Upper','Lower'].map((name,i)=>({name,path:name,image:name,altitudeMin:i*100,altitudeMax:(i+1)*100}))};
      const result = await original(cmd,args);
      if (cmd === 'get_demo') result.parsed.rounds = rounds;
      return result;
    };
  });
  await page.locator('.demo-item').first().click();
  await page.getByRole('tab').filter({hasText:'2D'}).click();
  const canvas = page.locator('.replay-box canvas');
  await canvas.waitFor();
  const button = name => page.getByRole('button',{name,exact:true});
  await page.locator('.player-loadout').first().waitFor();
  const feed = page.locator('.replay-feed > div');
  assert.equal(await feed.count(), 3);
  for (const i of [0, 1]) assert.match(await feed.nth(i).innerText(), /\+ Player 2/, 'Damage and flash assists show the assisting player');
  assert.ok(!(await feed.nth(2).innerText()).includes('+'), 'Solo kills have no assist separator');
  const headshot = feed.nth(0).getByRole('img', {name:'Headshot', exact:true});
  assert.equal(await headshot.getAttribute('src'), '/weapons/headshot.svg');
  await headshot.evaluate(img => img.decode());
  assert.equal(await feed.nth(1).getByRole('img', {name:'Headshot', exact:true}).count(), 0, 'Non-headshots have no headshot icon');
  assert.equal(await page.locator('.player-loadout').count(),10);
  assert.equal(await page.locator('.player-row').first().locator('img[alt="Flashbang"]').count(),2,'Both held flashbangs remain visible');
  await page.waitForFunction(()=>Array.from(document.querySelectorAll('.player-loadout img')).every(img=>img.complete && img.naturalWidth>0));
  assert.ok(await page.locator('.player-loadout .utility').evaluateAll(icons => icons.every(icon => icon.getBoundingClientRect().height === 14)), 'Utility icons share a visible height');
  assert.equal(await page.locator('.player-equipment').first().innerText(),'$2650','Weapon names no longer crowd money');
  assert.equal(await page.locator('.player-loadout img[alt="C4 Explosive"]').count(),1,'C4 appears once in the central equipment row');
  assert.equal(await page.locator('.player-name').filter({hasText:'C4'}).count(),0,'No duplicate C4 badge beside the name');
  assert.ok(!(await page.locator('.team-panel').allTextContents()).join('').includes('C4 Explosive'));
  const slotPositions = await page.locator('.player-loadout').evaluateAll(rows => rows.map(row => Array.from(row.children, slot => Math.round(slot.getBoundingClientRect().x))));
  assert.ok(slotPositions.every(positions => positions.join() === slotPositions[0].join()),'Equipment columns stay aligned across both teams');
  const originalViewport = page.viewportSize();
  for (const width of [900,1360]) {
    await page.setViewportSize({width,height:940});
    for (const size of [14,16,18]) {
      const fontOverride = await page.addStyleTag({content:`.radix-themes { --app-font-caption:${size}px; }`});
      assert.ok(await page.locator('.player-meta').evaluateAll(rows => rows.every(row => {
        const loadout = row.querySelector('.player-loadout').getBoundingClientRect();
        const money = row.querySelector('.player-equipment').getBoundingClientRect();
        const hp = row.querySelector('.player-vitals').getBoundingClientRect();
        return Array.from(row.querySelectorAll('.equipment-icon')).every(icon => {
          const rect = icon.getBoundingClientRect();
          return rect.left >= money.right + 3 && rect.left >= loadout.left && rect.right <= loadout.right + 1 && rect.right <= hp.left - 3 && rect.top >= loadout.top && rect.bottom <= loadout.bottom + 1;
        });
      })),`Equipment stays within its column at ${width}px / ${size}px text`);
      assert.ok(await page.locator('.player-name').evaluateAll(rows => rows.every(row => {
        const name = row.querySelector('strong').getBoundingClientRect();
        const kda = row.querySelector('.player-kda').getBoundingClientRect();
        return name.right <= kda.left && kda.right <= row.getBoundingClientRect().right + 1;
      })), 'Name and K/D/A stay on the first row without overlap');
      const heights = await page.locator('.player-row').evaluateAll(rows=>rows.map(row=>row.getBoundingClientRect().height));
      assert.ok(Math.max(...heights)-Math.min(...heights)<1,'All ten players have equal row heights');
      await fontOverride.evaluate(el=>el.remove());
    }
  }
  await page.setViewportSize(originalViewport);
  await page.waitForTimeout(50);
  const groundVisible = await canvas.evaluate(el => el.toDataURL());
  await button('Options').click();
  const groundSwitch = page.getByRole('switch', {name:'Ground utility', exact:true});
  const weaponSwitch = page.getByRole('switch', {name:'Ground weapons', exact:true});
  assert.equal(await groundSwitch.getAttribute('aria-checked'),'true','Ground utility defaults to on');
  assert.equal(await weaponSwitch.getAttribute('aria-checked'),'true','Ground weapons default to on');
  await groundSwitch.click();
  await page.waitForFunction(before => document.querySelector('.replay-box canvas').toDataURL() !== before, groundVisible);
  const weaponsOnly = await canvas.evaluate(el => el.toDataURL());
  await weaponSwitch.click();
  await page.waitForFunction(before => document.querySelector('.replay-box canvas').toDataURL() !== before, weaponsOnly);
  const neither = await canvas.evaluate(el => el.toDataURL());
  await groundSwitch.click();
  await page.waitForFunction(before => document.querySelector('.replay-box canvas').toDataURL() !== before, neither);
  const utilityOnly = await canvas.evaluate(el => el.toDataURL());
  assert.equal(new Set([groundVisible, weaponsOnly, neither, utilityOnly]).size, 4, 'Ground weapon and utility visibility are independent');
  await weaponSwitch.click();
  await page.waitForFunction(before => document.querySelector('.replay-box canvas').toDataURL() === before, groundVisible);
  await page.keyboard.press('Escape');
  await groundSwitch.waitFor({state:'hidden'});
  if (process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:`${process.env.UI_SCREENSHOT_DIR}/replay-equipment.png`});
  const drawLine = async (layer=0) => {
    const box = await canvas.boundingBox();
    const lay = layout(box.width,box.height,2,{zoom:1,panX:0,panY:0});
    const [x,y] = lay.origins[layer];
    await page.mouse.move(box.x+x+lay.side*0.3,box.y+y+lay.side*0.4);
    await page.mouse.down();
    await page.mouse.move(box.x+x+lay.side*0.65,box.y+y+lay.side*0.6,{steps:12});
    await page.mouse.up();
  };
  const pinkPixels = () => canvas.evaluate(el => {
    const data = el.getContext('2d').getImageData(0,0,el.width,el.height).data;
    let count=0;
    for(let i=0;i<data.length;i+=4) if(data[i]===255 && data[i+1]===56 && data[i+2]===184) count++;
    return count;
  });
  await button('Play').click();
  await button('Annotate').click();
  assert.equal(await button('Play').count(),1,'Drawing pauses playback');
  assert.equal(await page.locator('.replay-drawing-color').count(),8);
  await button('Violet').focus();
  await page.keyboard.press('Space');
  assert.equal(await button('Violet').getAttribute('aria-pressed'),'true','Color buttons support keyboard activation');
  assert.equal(await button('Annotate').getAttribute('aria-pressed'),'true');
  await button('Magenta').click();
  await drawLine();
  await page.waitForFunction(()=>!Array.from(document.querySelectorAll('button')).find(b=>b.getAttribute('aria-label')==='Undo')?.disabled);
  assert.ok(await pinkPixels()>0,'Pen is painted on the map');
  await button('Arrow').click();
  await button('Red').click();
  await drawLine(1);
  await page.waitForTimeout(50);
  assert.ok(await canvas.evaluate(el => {
    const data=el.getContext('2d').getImageData(0,0,el.width,el.height).data;
    return data.some((v,i)=>i%4===0 && v===255 && data[i+1]===69 && data[i+2]===69);
  }),'Arrow uses the selected red color');
  await button('Undo').click();
  assert.ok(await pinkPixels()>0,'Undo keeps the previous stroke');
  for (const shape of ['Ellipse','Rectangle']) {
    await button(shape).click();
    await drawLine(1);
    assert.equal(await button(shape).getAttribute('aria-pressed'),'true');
    await button('Undo').click();
  }
  await page.keyboard.press('Escape');
  assert.equal(await button('Annotate').getAttribute('aria-pressed'),'false');
  assert.equal(await page.locator('.replay-drawing-toolbar button').count(),1,'Collapsed toolbar shows only the palette');
  assert.ok(await pinkPixels()>0,'Escape preserves annotations');
  await button('Play').click();
  await button('Pause').click();
  assert.ok(await pinkPixels()>0,'Playback preserves annotations in the same round');
  await button('Annotate').click();
  await button('Clear').click();
  await page.waitForTimeout(50);
  assert.equal(await pinkPixels(),0);
  await button('Pen').click();
  await button('Magenta').click();
  await drawLine(1);
  await page.locator('.round-pill').nth(1).click();
  assert.equal(await button('Undo').isDisabled(),true,'Round changes clear history');
  await page.waitForTimeout(50);
  assert.equal(await pinkPixels(),0,'Round changes clear the drawing');
  const cancelBox=await canvas.boundingBox();
  await page.mouse.move(cancelBox.x+cancelBox.width*0.25,cancelBox.y+cancelBox.height*0.5);
  await page.mouse.down();
  await page.mouse.move(cancelBox.x+cancelBox.width*0.3,cancelBox.y+cancelBox.height*0.5);
  await canvas.dispatchEvent('pointercancel');
  await page.mouse.up();
  assert.equal(await button('Undo').isDisabled(),true,'Cancelled strokes do not enter history');
  const box = await canvas.boundingBox();
  await page.mouse.move(box.x+box.width*0.25,box.y+box.height*0.5);
  await page.mouse.down();
  await page.mouse.move(box.x+box.width+20,box.y+box.height*0.5,{steps:10});
  await page.mouse.up();
  assert.equal(await button('Undo').isDisabled(),false,'Pointer capture finishes a stroke outside the map');
  if(process.env.UI_SCREENSHOT_DIR) await page.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/replay-drawing.png'});
  await page.setViewportSize({width:900,height:940});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'Drawing toolbar wraps on narrow windows');
  await page.setViewportSize({width:1360,height:940});
  await page.locator('.demo-item').nth(1).click();
  await page.getByRole('tab').filter({hasText:'2D'}).click();
  await button('Annotate').waitFor();
  assert.equal(await button('Undo').count(),0,'Switching demos starts with no annotations');
  await page.getByRole('tab').filter({hasText:'Players'}).click();
  await page.evaluate(() => {
    const request=window.requestAnimationFrame.bind(window), cancel=window.cancelAnimationFrame.bind(window);
    const add=window.addEventListener.bind(window), remove=window.removeEventListener.bind(window);
    const frames=new Set(), keys=new Set();
    window.requestAnimationFrame=callback=>{
      const id=request(time=>{frames.delete(id);callback(time);});
      frames.add(id);
      return id;
    };
    window.cancelAnimationFrame=id=>{frames.delete(id);cancel(id);};
    window.addEventListener=(type,listener,options)=>{if(type==='keydown')keys.add(listener);add(type,listener,options);};
    window.removeEventListener=(type,listener,options)=>{if(type==='keydown')keys.delete(listener);remove(type,listener,options);};
    const NativeWorker = window.Worker;
    let workers = 0;
    const bitmaps = [];
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args); workers++;
        this.addEventListener('message', ({data}) => { if (data.images) bitmaps.push(...data.images); });
      }
      terminate() { if (!this.stopped) { this.stopped = true; workers--; } super.terminate(); }
    };
    window.replayResources=()=>({frames:frames.size,keys:keys.size,workers,bitmaps:bitmaps.filter(image=>image.width>0).length});
    window.restoreReplayTracking=()=>{
      window.Worker=NativeWorker;
      window.requestAnimationFrame=request;window.cancelAnimationFrame=cancel;
      window.addEventListener=add;window.removeEventListener=remove;
      delete window.replayResources;delete window.restoreReplayTracking;
    };
  });
  const cdp=await page.context().newCDPSession(page);
  const heaps=[];
  try {
    for(let cycle=0;cycle<6;cycle++) {
      await page.getByRole('tab').filter({hasText:'2D'}).click();
      await button('Annotate').click();
      await drawLine();
      await button('Clear').click();
      await drawLine();
      await page.getByRole('tab').filter({hasText:'Players'}).click();
      await canvas.waitFor({state:'detached'});
      await page.waitForFunction(()=>{const r=window.replayResources();return r.frames===0 && r.keys===0;});
      assert.deepEqual(await page.evaluate(()=>window.replayResources()),{frames:0,keys:0,workers:0,bitmaps:0},'Leaving 2D releases animation callbacks, key listeners, workers and bitmaps');
      await cdp.send('HeapProfiler.collectGarbage');
      heaps.push((await cdp.send('Runtime.getHeapUsage')).usedSize);
    }
    console.log('Replay cleanup: 6 drawing/unmount cycles, no remaining RAF, key listeners, radar workers or bitmaps; post-GC heap bytes:',heaps.join(', '));
  } finally {
    await page.evaluate(()=>window.restoreReplayTracking());
    await cdp.detach();
  }
  console.log('Replay drawing checks passed: eight colors, pen/arrow/ellipse/rectangle, undo, clear, playback, round reset, pointer capture.');
}
