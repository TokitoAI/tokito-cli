// Playwright CLI: run-code --filename tests/prototype_browser.js
// Run against a freshly generated supported real dataset prototype.
async (page) => {
  const assert = (ok, message) => { if (!ok) throw new Error(message); };
  await page.waitForFunction(() => document.querySelector('#view').dataset.rendered === 'true');
  const stats = await page.evaluate(() => ({
    components: window.tokitoInspection.componentCount,
    triangles: window.tokitoInspection.triangleCount,
    vertices: window.tokitoInspection.vertexCount,
    draws: window.tokitoInspection.drawCount,
    error: document.querySelector('#error').textContent
  }));
  const ultimaker = stats.components === 574;
  const esp32 = stats.components === 19;
  const ic = esp32 ? 'U2' : ultimaker ? 'U1' : 'U11';
  const second = esp32 ? 'D1' : ultimaker ? 'C74' : 'R100';
  const missing = esp32 ? 'MH1' : ultimaker ? 'U9' : 'BH1';
  const esp32Output = 'target/prototypes/'+new URL(page.url()).pathname.split('/')[1]+'/prototype';
  const output = esp32 ? esp32Output : ultimaker ? 'target/prototypes/ultimaker-source-e2e-v2/prototype' : 'target/prototypes/nxp-revc-selectable-v3';
  assert(esp32 ? (stats.triangles === 7224 && stats.vertices === 21672 && stats.draws === 29 && !stats.error) : ultimaker ? (stats.triangles === 712527 && stats.vertices === 811884 && stats.draws === 841 && !stats.error) : (stats.components === 391 && stats.triangles === 108920 && stats.vertices === 134860 && stats.draws === 871 && !stats.error),
    'Actual dataset geometry counters differ from independent source counts');
  if(esp32){
    assert(await page.locator('#population-audit').count() === 1,'ESP32 BOM-enabled journey must show the population panel');
    assert((await page.locator('[data-part-class="resistor"]').innerText()).replace(/\s+/g,' ').trim() === 'resistor 6 0','Visible resistor deficit differs from BOM');
    assert((await page.locator('[data-part-class="capacitor"]').innerText()).replace(/\s+/g,' ').trim() === 'capacitor 6 2','Visible capacitor deficit differs from BOM');
    assert((await page.locator('#population-audit').innerText()).includes('U1, U2'),'Reference identity conflicts not visible');
  }
  await page.setViewportSize({width:1600,height:1000});
  await page.locator('#reset').click();
  const canvas = page.locator('#view');
  const stage = page.locator('#stage');
  const initial = await stage.screenshot();
  await page.locator('#search').fill(ic);
  await page.getByRole('button', {name:new RegExp('^'+ic+' \u00b7')}).click();
  assert(await page.evaluate(() => window.tokitoInspection.selected()) === ic, 'IC selection failed');
  const highlighted = await stage.screenshot();
  assert(!initial.equals(highlighted), 'IC highlight did not change actual rendered pixels');
  await page.screenshot({path:output+'/selected-ic.png'});
  await page.locator('#search').fill(second);
  await page.getByRole('button', {name:new RegExp('^'+second+' \u00b7')}).click();
  assert(await page.evaluate(() => window.tokitoInspection.selected()) === second, 'Separate component selection failed');
  const secondPixels = await stage.screenshot();
  // NXP R100 is occluded in the reset view: its identity selection is tested,
  // while U11 checks visible highlighting. Ultimaker C74 additionally checks
  // visible multi-mesh highlighting. Do not claim occluded pixels changed.
  if (ultimaker) assert(!initial.equals(secondPixels), 'Multi-mesh component highlight did not change rendered pixels');
  await page.locator('#reset').click();
  assert(await page.evaluate(() => window.tokitoInspection.selected()) === null, 'Clear selection failed');
  const restored = await stage.screenshot();
  assert(initial.equals(restored), 'Original colours/view not restored');
  await page.locator('#search').fill(missing);
  await page.getByRole('button', {name:new RegExp('^'+missing+' \u00b7')}).click();
  assert((await page.locator('#status').textContent()).includes('no model available'), 'Missing model was not explicit');
  await page.locator('#reset').click();
  const box = await canvas.boundingBox();
  let picked = null;
  for (const [rx,ry] of [[.5,.5],[.55,.55],[.6,.55],[.45,.48],[.6,.6],[.65,.62]]) {
    await page.mouse.click(box.x+box.width*rx,box.y+box.height*ry);
    picked = await page.evaluate(() => window.tokitoInspection.selected());
    if(picked) break;
  }
  assert(picked, 'Visible-surface GPU click picking failed');
  await page.locator('#reset').click();
  await page.mouse.move(box.x+box.width*.5,box.y+box.height*.5);
  await page.mouse.down();
  await page.mouse.move(box.x+box.width*.5+110,box.y+box.height*.5+60,{steps:6});
  await page.mouse.up();
  const orbited = await stage.screenshot();
  assert(!initial.equals(orbited), 'Orbit did not change rendered pixels');
  await page.mouse.wheel(0,-250);
  const zoomed = await stage.screenshot();
  assert(!orbited.equals(zoomed), 'Zoom did not change rendered pixels');
  const html = await page.content();
  await page.context().setOffline(true);
  await page.setContent(html);
  await page.waitForFunction(() => document.querySelector('#view').dataset.rendered === 'true');
  await page.locator('#search').fill(ic);
  await page.getByRole('button', {name:new RegExp('^'+ic+' \u00b7')}).click();
  assert(await page.evaluate(() => window.tokitoInspection.selected()) === ic, 'Offline fresh render/selection failed');
  await page.setViewportSize({width:3840,height:2160});
  await page.screenshot({path:output+'/selected-ic-4k.png'});
  await page.setViewportSize({width:1600,height:1000});
  await page.context().setOffline(false);
  return {passed:true,stats,picked,checks:['IC highlight pixels',esp32 ? 'separate diode identity' : ultimaker ? 'multi-mesh highlight pixels' : 'separate resistor identity','colour restoration','missing-model message','GPU picking','orbit pixels','zoom pixels','fresh offline render','3840x2160 screenshot']};
}
