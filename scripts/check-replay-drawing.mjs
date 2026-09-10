import assert from 'node:assert/strict';
import { layout } from '../src/replay/draw.ts';

export async function checkReplayDrawing(page) {
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const rounds = [1, 2].map((round, i) => ({round, startTick:i * 6400, freezeEndTick:i * 6400 + 128, endTick:i * 6400 + 6200, officiallyEndedTick:(i + 1) * 6400 - 1, roster:{}}));
    const data = {schemaVersion:3,tickRate:64,step:4,firstTick:0,lastTick:12799,players:[],weapons:[''],events:[],frames:[{t:0,p:[],g:[]},{t:12799,p:[],g:[]}]};
    const radar = document.createElement('canvas');
    radar.width = radar.height = 1024;
    radar.getContext('2d').fillStyle = '#263440';
    radar.getContext('2d').fillRect(0, 0, 1024, 1024);
    const radarUrl = radar.toDataURL();
    window.__TAURI_INTERNALS__.convertFileSrc = path => path === 'drawing-replay'
      ? 'data:application/json,' + encodeURIComponent(JSON.stringify(data))
      : radarUrl;
    window.__TAURI_INTERNALS__.invoke = async (cmd,args) => {
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
