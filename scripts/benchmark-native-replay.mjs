import { createRequire } from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
let browser;
for(let attempt=0;attempt<30;attempt++){
  try { browser=await chromium.connectOverCDP(process.argv[2]||'http://127.0.0.1:9494'); break; }
  catch(error){ if(attempt===29) throw error; await new Promise(resolve=>setTimeout(resolve,500)); }
}
const page=browser.contexts()[0].pages().find(p=>!p.url().startsWith('devtools:'));
if(!page) throw new Error('No native WebView page');
if(page.url() === 'about:blank') await page.goto('http://tauri.localhost/');
await page.addInitScript(()=>{
  document.addEventListener('click',e=>{
    if(e.target.closest?.('[role="tab"]')?.textContent.includes('2D')) window.replayBench={start:performance.now(),workers:{}};
  },true);
  const NativeWorker=window.Worker;
  window.Worker=class extends NativeWorker {
    constructor(url,...args){super(url,...args);const kind=String(url).includes('load.worker')?'replay':'radar';const b=window.replayBench;
      if(b){const begin=performance.now();b.workers[kind]={startMs:begin-b.start};this.addEventListener('message',()=>{const w=b.workers[kind];w.messageMs=performance.now()-b.start;w.durationMs=performance.now()-begin;queueMicrotask(()=>w.handlerDoneMs=performance.now()-b.start);});}
    }
  };
  const draw=CanvasRenderingContext2D.prototype.drawImage;
  CanvasRenderingContext2D.prototype.drawImage=function(...args){
    const result=draw.apply(this,args);const b=window.replayBench;
    if(b && !b.drawing && this.canvas.closest?.('.replay-box')){b.drawing=true;b.firstDrawMs=performance.now()-b.start;requestAnimationFrame(()=>requestAnimationFrame(()=>b.firstFrameMs=performance.now()-b.start));}
    return result;
  };
});
let files=[];
page.on('request', request=>{
  const url=request.url();
  if(url.includes('.replay.bin')) files.push('binary');
  else if(url.includes('.replay.json')) files.push('json');
});
try {
 for(let run=0;run<Number(process.argv[4]||5);run++) {
  files=[];
  await page.reload();
  await page.locator('.demo-item').filter({hasText:process.argv[3]||'de_cache'}).first().click({timeout:60000});
  await page.getByRole('tab').filter({hasText:'2D'}).click();
  await page.waitForFunction(()=>window.replayBench?.firstFrameMs>0,{},{timeout:120000});
  console.log(JSON.stringify({run,files,...await page.evaluate(()=>window.replayBench)}));
 }
} finally { await browser.close(); }
