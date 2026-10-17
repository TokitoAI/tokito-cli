// Playwright CLI run-code --filename tests/presentation_browser.js.
// Run on a newly generated real-board prototype; save downloads in its QA cwd.
async (page) => {
  const assert = (value, message) => { if (!value) throw Error(message); };
  const settlePaint = async () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.waitForFunction(() => document.querySelector('#view').dataset.rendered === 'true');
  await page.setViewportSize({width:1600,height:1000});
  const source = await page.evaluate(() => JSON.parse(document.querySelector('#index').textContent));
  const available = source.components.filter(c => c.modelAvailable && c.selectorNodeIndex != null);
  assert(available.length > 0, 'real board has no selectable model');
  const initial = await page.evaluate(() => tokitoInspection.snapshotImage());
  assert(await page.evaluate(() => tokitoInspection.presentationProfileId) === 'tokito.ultimaker-studio.v1', 'profile identity');
  assert(await page.locator('#lighting').inputValue() === 'ultimaker-reference', 'default reference lighting');
  const selected = available.find(c => /^U\d+$/.test(c.reference)) || available[0];
  await page.locator('#search').fill(selected.reference);
  const selectedButton = page.getByRole('button', {name: new RegExp('^'+selected.reference+' ·')});
  assert(await selectedButton.count() === 1, 'searched component missing from list');
  await selectedButton.click();
  assert(await page.evaluate(() => tokitoInspection.selected()) === selected.reference, 'list click did not select exact identity');
  const selectionPixels = await page.locator('#selection').evaluate(c => {const d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;let n=0;for(let i=3;i<d.length;i+=4)if(d[i])n++;return n;});
  assert(selectionPixels > 0, 'selected component highlight not visible');
  await page.locator('#search').fill('');
  assert(initial === await page.evaluate(() => tokitoInspection.snapshotImage()), 'selection changed rendered colours');
  const png = await page.evaluate(async () => {
    const data = tokitoInspection.snapshot4k();
    const b = Uint8Array.from(atob(data.split(',')[1].slice(0,48)), c => c.charCodeAt(0));
    const v = new DataView(b.buffer);
    const image=new Image();image.src=data;await image.decode();
    const sample=document.createElement('canvas');sample.width=128;sample.height=72;
    const ctx=sample.getContext('2d');ctx.drawImage(image,0,0,128,72);
    const pixels=ctx.getImageData(0,0,128,72).data;let nonBackground=0;
    for(let i=0;i<pixels.length;i+=4){if(Math.abs(pixels[i]-17)>4||Math.abs(pixels[i+1]-24)>4||Math.abs(pixels[i+2]-32)>4)nonBackground++;}
    return {width:v.getUint32(16),height:v.getUint32(20),bytes:data.length,nonBackground};
  });
  assert(png.width === 3840 && png.height === 2160 && png.bytes > 10000 && png.nonBackground > 100, 'not an actual populated 4K PNG');
  assert(await page.evaluate(() => tokitoInspection.selected()) === selected.reference, 'capture changed selected identity');
  assert(initial === await page.evaluate(() => tokitoInspection.snapshotImage()), '4K capture did not restore interactive resolution/camera');
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', {name:'Save 4K PNG'}).click();
  const download = await downloadPromise;
  const board = new URL(page.url()).pathname.split('/').slice(-2,-1)[0];
  await download.saveAs('presentation-'+board+'-4k.png');
  assert((await page.locator('#capture-status').innerText()).includes('3840'), 'download result invisible');
  await page.locator('#lighting').selectOption('material-studio');
  await settlePaint();
  assert(initial !== await page.evaluate(() => tokitoInspection.snapshotImage()), 'material studio did not change rendering');
  await page.locator('#lighting').selectOption('ultimaker-reference');
  await settlePaint();
  assert(initial === await page.evaluate(() => tokitoInspection.snapshotImage()), 'reference colours did not restore');
  const missing = source.components.find(c => !c.modelAvailable);
  if(missing){const missingButton=page.getByRole('button',{name:new RegExp('^'+missing.reference+' ·')});assert(await missingButton.count()===1,'missing component not listed');await missingButton.click();assert(await page.evaluate(()=>tokitoInspection.selected())===missing.reference,'missing component identity not selected');assert((await page.locator('#status').innerText()).includes('no model available'),'missing body not explicit');await selectedButton.click();}
  await page.locator('#reset').click();
  assert(initial === await page.evaluate(() => tokitoInspection.snapshotImage()), 'reset changed default geometry/camera');
  const html = await page.content();
  await page.context().setOffline(true);
  await page.setContent(html);
  await page.waitForFunction(() => document.querySelector('#view').dataset.rendered === 'true');
  await page.getByRole('button',{name:new RegExp('^'+selected.reference+' ·')}).click();
  assert(await page.evaluate(() => tokitoInspection.selected()) === selected.reference,'offline selection failed');
  const statistics = await page.evaluate(() => ({components:tokitoInspection.componentCount,triangles:tokitoInspection.triangleCount,draws:tokitoInspection.drawCount,error:document.querySelector('#error').textContent}));
  assert(!statistics.error && statistics.components === source.components.length,'renderer error or lost identities');
  await page.context().setOffline(false);
  // Explicitly simulated GPU-cap refusal, not a claim about another device.
  await page.addInitScript(() => {
    const original=WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter=function(parameter){return parameter===this.MAX_VIEWPORT_DIMS?new Int32Array([1024,1024]):original.call(this,parameter);};
  });
  await page.reload();
  await page.waitForFunction(() => document.querySelector('#view').dataset.rendered === 'true');
  await page.getByRole('button',{name:'Save 4K PNG'}).click();
  assert((await page.locator('#capture-status').innerText()).includes('exceeds this graphics device limit'),'GPU limit refusal absent');
  assert(await page.locator('#view').getAttribute('data-rendered')==='true','unsupported capture broke interactive view');
  return {passed:true,board,statistics,png,selected:selected.reference,missingTested:missing?.reference||null,checks:['reference default','selection colour invariance','actual 4K PNG','download button','capture state restoration','material mode roundtrip','exact identity',...(missing?['missing-body disclosure']:[]),'fresh offline rendering','simulated GPU-limit refusal'],geometryQualificationChanged:false};
}
