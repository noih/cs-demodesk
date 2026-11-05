import assert from 'node:assert/strict';

export async function checkEditingEffects(page, dialog) {
  const effects = dialog.locator('.editing-effects');
  assert.equal(await effects.getAttribute('open'), null);
  await effects.locator('summary').click();
  const switches = effects.getByRole('switch');
  assert.equal(await switches.count(), 4);
  for (const toggle of await switches.all()) assert.equal(await toggle.isChecked(), false);
  const opening = effects.getByRole('switch', {name:'Opening camera', exact:true});
  const combat = effects.getByRole('switch', {name:'Combat perspectives', exact:true});
  const teamwork = effects.getByRole('switch', {name:'Utility teamwork', exact:true});
  const push = effects.getByRole('switch', {name:'Team push', exact:true});
  await push.click();
  assert.equal(await opening.isChecked(),false);
  await teamwork.click();
  assert.equal(await combat.isChecked(), false, 'Teamwork does not enable enemy perspectives');
  assert.equal(await opening.isChecked(), false, 'Teamwork does not enable openings');
  assert.equal(await combat.isDisabled(), false);
  await combat.click();
  assert.equal(await opening.isDisabled(), false);
  await opening.click();
  await teamwork.focus();
  const viewport = page.viewportSize();
  for (const width of [600, 900, 1360]) {
    await page.setViewportSize({width, height:940});
    assert.ok(await effects.evaluate(el => [el, ...el.querySelectorAll('.editing-effect-card')].every(node => node.scrollWidth <= node.clientWidth + 1)), `Effects fit at ${width}px`);
  }
  if (process.env.UI_SCREENSHOT_DIR) await effects.screenshot({path:process.env.UI_SCREENSHOT_DIR + '/editing-effects.png'});
  await page.setViewportSize(viewport);
  // Capture a real export submission without queuing a native render in this UI test.
  await page.evaluate(() => {
    window.utilityTestRestore = {missing:window.missingTools,invoke:window.__TAURI_INTERNALS__.invoke};
    window.missingTools = false;
    window.__TAURI_INTERNALS__.invoke = async (cmd,args) => {
      if (cmd === 'start_render') {window.utilityTestOptions=args.options;throw new Error('Utility export probe');}
      return window.utilityTestRestore.invoke(cmd,args);
    };
  });
  const refresh = page.locator('.header-tools .bi-arrow-clockwise').locator('..');
  await refresh.evaluate(el=>el.click());
  const submit = dialog.getByRole('button',{name:'Export',exact:true});
  await page.waitForFunction(()=>!document.querySelector('[role=dialog] button.rt-Button:last-child')?.disabled);
  await submit.click();
  await dialog.getByText('Utility export probe',{exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>window.utilityTestOptions.utilityTeamwork),true,'Export includes utility teamwork');
  assert.equal(await page.evaluate(()=>window.utilityTestOptions.openingCamera),true,'Export includes opening camera');
  assert.equal(await page.evaluate(()=>window.utilityTestOptions.combatPerspectives),true,'Export includes combat perspectives');
  assert.equal(await page.evaluate(()=>window.utilityTestOptions.teamPush),true,'Export includes team push');
  await push.click();
  await combat.click();
  await opening.click();
  await teamwork.click();
  assert.equal(await teamwork.isChecked(), false);
  await submit.click();
  await page.waitForFunction(()=>window.utilityTestOptions.utilityTeamwork===false && window.utilityTestOptions.openingCamera===false && window.utilityTestOptions.combatPerspectives===false && window.utilityTestOptions.teamPush===false);
  await page.evaluate(() => {
    window.missingTools=window.utilityTestRestore.missing;
    window.__TAURI_INTERNALS__.invoke=window.utilityTestRestore.invoke;
    delete window.utilityTestRestore;
  });
  await refresh.evaluate(el=>el.click());
  for (const toggle of await switches.all()) assert.equal(await toggle.isChecked(), false);
  await effects.locator('summary').click();
}
