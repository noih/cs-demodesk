import assert from 'node:assert/strict';

export async function checkEditingEffects(page, dialog) {
  const effects = dialog.locator('.editing-effects');
  assert.equal(await effects.getByRole('checkbox').count(), 0, 'Do not promise edits before analysis');
  await page.evaluate(() => {
    window.editingRestore = {missing:window.missingTools,invoke:window.__TAURI_INTERNALS__.invoke};
    window.editingCalls = [];
    window.missingTools = false;
    window.__TAURI_INTERNALS__.invoke = async (cmd,args) => {
      if (['analyze_editing','cached_editing_analysis','start_render'].includes(cmd)) window.editingCalls.push({cmd,args});
      if (cmd === 'cached_editing_analysis') return args.options.shortCombatClips ? null : window.editingCache ?? null;
      if (cmd === 'analyze_editing') {
        if (args.force) window.editingCache=null;
        if (window.editingFail) throw new Error('Analysis test failure');
        if (window.editingEmpty) return window.editingCache={id:'empty',candidates:[],warnings:[],cached:false};
        const h=args.highlightIds[0];
        return window.editingCache={id:'analysis-1',cached:!args.force,warnings:[],candidates:[
          {id:'intro',highlightId:h,kind:'opening',tick:1000,target:'',recommended:true,addedSeconds:1.8,conflicts:['push']},
          ...[1,2,3].map(i=>({id:'kill-'+i,highlightId:h,kind:'combat',tick:1000+i*128,target:'Enemy '+i,recommended:i===1,addedSeconds:2,conflicts:[]})),
          {id:'push',highlightId:h,kind:'push',tick:1000,target:'',recommended:false,addedSeconds:3,conflicts:['intro']},
        ]};
      }
      if (cmd === 'start_render') {window.editingOptions=args.options;throw new Error('Editing export probe');}
      return window.editingRestore.invoke(cmd,args);
    };
  });
  const refresh = page.locator('.header-tools .bi-arrow-clockwise').locator('..');
  await refresh.evaluate(el=>el.click());
  await effects.getByRole('button',{name:'Analyze scenes',exact:true}).click();
  await effects.getByRole('checkbox').first().waitFor();
  const replays=effects.getByRole('checkbox',{name:/Combat perspectives/});
  assert.equal(await replays.count(),3);
  assert.equal(await replays.nth(0).isChecked(),true);
  assert.equal(await replays.nth(1).isChecked(),false);
  assert.equal(await replays.nth(2).isChecked(),false);
  // Clicking the entire option row works, not just the checkbox.
  await replays.nth(1).locator('xpath=ancestor::label').click();
  await replays.nth(2).click();
  await effects.getByRole('checkbox',{name:/Team push/}).click();
  assert.equal(await effects.getByRole('checkbox',{name:/Opening camera/}).isChecked(),false,'Conflicting intro is replaced');
  const groupHeader=effects.locator('.editing-highlight-header').first();
  assert.equal(await groupHeader.getAttribute('aria-expanded'),'true');
  await groupHeader.click();
  assert.equal(await effects.getByRole('checkbox').count(),0,'Collapsed highlights hide their options');
  assert.match(await groupHeader.innerText(),/4 \/ 5/,'Selection count stays visible when collapsed');
  await groupHeader.press('Enter');
  assert.equal(await replays.nth(2).isChecked(),true,'Collapsing preserves selections');
  const viewport=page.viewportSize();
  for (const width of [600,900,1360]) {
    await page.setViewportSize({width,height:940});
    assert.ok(await effects.evaluate(el=>[el,...el.querySelectorAll('.editing-highlight,.editing-option-row')].every(node=>node.scrollWidth<=node.clientWidth+1)),`Editing fits ${width}px`);
  }
  if (process.env.UI_SCREENSHOT_DIR) await effects.screenshot({path:process.env.UI_SCREENSHOT_DIR+'/editing-effects.png'});
  await page.setViewportSize(viewport);
  const submit=dialog.getByRole('button',{name:'Export',exact:true});
  await submit.click();
  await dialog.getByText('Editing export probe',{exact:true}).waitFor();
  const options=await page.evaluate(()=>window.editingOptions);
  assert.equal(options.editingAnalysisId,'analysis-1');
  assert.deepEqual(options.editingSelections,['kill-1','kill-2','kill-3','push']);
  assert.equal(options.combatPerspectives,true);
  assert.equal(options.utilityTeamwork,false);
  assert.equal(options.openingCamera,false);
  await effects.getByRole('button',{name:'Analyze again',exact:true}).click();
  await effects.getByRole('checkbox').first().waitFor();
  assert.equal(await page.evaluate(()=>window.editingCalls.filter(c=>c.cmd==='analyze_editing').at(-1).args.force),true);
  const analysesBefore=await page.evaluate(()=>window.editingCalls.filter(c=>c.cmd==='analyze_editing').length);
  await dialog.getByRole('button',{name:'Close',exact:true}).click();
  await dialog.waitFor({state:'detached'});
  await page.getByRole('button',{name:/^Export/}).click();
  await effects.getByRole('checkbox').first().waitFor();
  assert.equal(await page.evaluate(()=>window.editingCalls.filter(c=>c.cmd==='analyze_editing').length),analysesBefore,'Reopening reads cache without analyzing');
  assert.equal(await effects.getByRole('button',{name:'Clear analysis',exact:true}).count(),0);
  assert.equal(await effects.getByText('Using cached analysis',{exact:true}).count(),0);
  assert.equal(await effects.getByText(/Recommendations are preselected|Estimated|enemy replay|Demo /).count(),0);
  assert.equal(await effects.getByRole('status').count(),1,'Only one duration/quality notice');
  for(const checkbox of await effects.getByRole('checkbox').all()) {
    if(await checkbox.isChecked()) await checkbox.click();
  }
  await submit.click();
  await page.waitForFunction(()=>window.editingOptions.editingAnalysisId===undefined);
  assert.equal(await page.evaluate(()=>window.editingOptions.combatPerspectives),false);
  await page.evaluate(()=>{window.editingFail=true;});
  await effects.getByRole('button',{name:'Analyze again',exact:true}).click();
  await effects.getByRole('alert').filter({hasText:'Analysis test failure'}).waitFor();
  await page.evaluate(()=>{window.editingFail=false;window.editingEmpty=true;});
  await effects.getByRole('button',{name:'Analyze scenes',exact:true}).click();
  await effects.getByText('No available edits for these clips. Export normally.').waitFor();
  assert.equal(await effects.getByRole('checkbox').count(),0);
  await page.evaluate(()=>{window.editingEmpty=false;});
  await effects.getByRole('button',{name:'Analyze again',exact:true}).click();
  await effects.getByRole('checkbox').first().waitFor();
  const modes=dialog.getByRole('radiogroup',{name:'Combat clips'});
  // Changing the trim window invalidates the visible selection immediately.
  if(await modes.count()) {
    await modes.getByRole('radio',{name:'Short',exact:true}).click();
    await effects.getByRole('button',{name:'Analyze scenes',exact:true}).waitFor();
    await modes.getByRole('radio',{name:'Normal',exact:true}).click();
    await effects.getByRole('checkbox').first().waitFor();
  }
  await page.evaluate(()=>{
    window.missingTools=window.editingRestore.missing;
    window.__TAURI_INTERNALS__.invoke=window.editingRestore.invoke;
    delete window.editingRestore;
  });
  await refresh.evaluate(el=>el.click());
}
