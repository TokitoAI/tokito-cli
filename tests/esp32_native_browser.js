// Actual native ESP32 community-reference prototype, including missing parts.
async(page,expectedBodies=19)=>{
  const assert=(ok,message)=>{if(!ok)throw Error(message);};
  await page.waitForFunction(()=>document.querySelector('#view').dataset.rendered==='true');
  const stats=await page.evaluate(()=>({components:tokitoInspection.componentCount,triangles:tokitoInspection.triangleCount,draws:tokitoInspection.drawCount,error:document.querySelector('#error').textContent}));
  assert(stats.components===26 && stats.triangles>1000 && !stats.error,'Native board did not render');
  assert(await page.locator('#population-audit').count()===1,'BOM panel missing');
  for(const cls of ['resistor','capacitor'])assert((await page.locator('[data-part-class="'+cls+'"]').innerText()).replace(/\s+/g,' ').trim()===cls+' 6 6','Native source population mismatch: '+cls);
  for(const ref of ['R1','R2','R3','R4','R5','R6','C1','C2','C3','C4','C5','C6','D1','Q1','Q2','U1','U2','J2','J3']){
    const selected=await page.evaluate(ref=>tokitoInspection.select(ref),ref);
    assert(selected.modelAvailable && selected.reference===ref,'Missing selectable source body '+ref);
  }
  if(expectedBodies>=21)for(const ref of ['SW1','SW2']){
    const selected=await page.evaluate(ref=>tokitoInspection.select(ref),ref);
    assert(selected.modelAvailable && selected.reference===ref,'Missing source switch '+ref);
  }
  if(expectedBodies>=23)for(const ref of ['J1','D2']){
    const selected=await page.evaluate(ref=>tokitoInspection.select(ref),ref);
    assert(selected.modelAvailable && selected.reference===ref,'Missing source USB/LED '+ref);
  }
  const centralModule=await page.evaluate(()=>tokitoInspection.select('U1'));
  assert(centralModule.modelAvailable && centralModule.reference==='U1','Central ESP32 module is not selectable');
  await page.setViewportSize({width:1600,height:1000});
  const stage=page.locator('#stage');
  const selectionPixels=()=>page.evaluate(()=>{const c=document.querySelector('#selection'),data=c.getContext('2d').getImageData(0,0,c.width,c.height).data;let n=0;for(let i=3;i<data.length;i+=4)if(data[i])n++;return n;});
  const capture=()=>page.evaluate(()=>tokitoInspection.snapshotImage());
  await page.locator('#reset').click();const initial=await stage.screenshot(),initialView=await capture();
  const baselineColors=await page.evaluate(async url=>{const im=new Image();im.src=url;await im.decode();const c=document.createElement('canvas');c.width=im.width;c.height=im.height;const ctx=c.getContext('2d');ctx.drawImage(im,0,0);const d=ctx.getImageData(0,0,c.width,c.height).data,colors=new Set();for(let i=0;i<d.length;i+=4)colors.add(d[i]+','+d[i+1]+','+d[i+2]);return colors.size;},initialView);
  assert(baselineColors>32,'Baseline snapshot contains no rendered geometry');
  await page.locator('#search').fill('U2');await page.getByRole('button',{name:/^U2/}).click();
  assert(!initial.equals(await stage.screenshot()),'Regulator highlight did not change pixels');
  assert(await selectionPixels()>0,'Regulator selection outline was not drawn');
  assert(initialView===await capture(),'Selection recolored or changed original regulator materials');
  await page.evaluate(()=>document.querySelector('aside').scrollTop=0);
  const prototypePath=new URL(page.url()).pathname.replace(/^\//,'').replace(/\/index\.html$/,'');
  const output='target/prototypes/'+prototypePath;
  await page.screenshot({path:output+'/native-selected-regulator.png'});
  await page.locator('#reset').click();
  assert(initial.equals(await stage.screenshot()),'Source colours not restored after selection reset');
  assert(await selectionPixels()===0,'Reset did not clear the selection outline');
  await page.locator('#search').fill('U1');await page.getByRole('button',{name:/^U1/}).click();
  assert(!initial.equals(await stage.screenshot()),'Central module highlight did not change pixels');
  assert(await selectionPixels()>0,'Central module selection outline was not drawn');
  assert(initialView===await capture(),'Central module selection changed its source material colors');
  await page.screenshot({path:output+'/native-selected-central-module.png'});
  await page.locator('#reset').click();
  assert(await selectionPixels()===0,'Reset did not clear central module selection');
  await page.evaluate(()=>document.querySelector('aside').scrollTop=0);
  await page.screenshot({path:output+'/native-original-materials.png'});
  await page.evaluate(()=>tokitoInspection.select('U3'));
  assert((await page.locator('#status').innerText()).includes('no model available'),'CP2102 refusal not visible');
  const html=await page.content();await page.context().setOffline(true);await page.setContent(html);
  await page.waitForFunction(()=>document.querySelector('#view').dataset.rendered==='true');
  await page.evaluate(()=>tokitoInspection.select('R1'));
  assert(await page.evaluate(()=>tokitoInspection.selected())==='R1','Offline resistor selection failed');
  await page.setViewportSize({width:3840,height:2160});await page.screenshot({path:output+'/native-selected-resistor-4k.png'});
  await page.setViewportSize({width:1600,height:1000});await page.context().setOffline(false);
  assert(await selectionPixels()>0,'Selected resize lost its outline');
  assert(await page.evaluate(()=>{const a=document.querySelector('#view'),b=document.querySelector('#selection');return a.width===b.width&&a.height===b.height;}),'Selection/view bitmap sizes disagree after resize');
  const overlayCapture=()=>page.locator('#selection').evaluate(c=>c.toDataURL());
  const view=page.locator('#view'),box=await view.boundingBox(),beforeOrbit=await capture(),outlineBeforeOrbit=await overlayCapture();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+80,box.y+box.height/2+35,{steps:8});await page.mouse.up();
  assert(beforeOrbit!==await capture() && outlineBeforeOrbit!==await overlayCapture() && await selectionPixels()>0,'Orbit did not redraw selected geometry and outline');
  const beforeZoom=await capture(),outlineBeforeZoom=await overlayCapture();await page.mouse.wheel(0,-180);await page.waitForTimeout(100);
  assert(beforeZoom!==await capture() && outlineBeforeZoom!==await overlayCapture() && await selectionPixels()>0,'Zoom did not redraw selected geometry and outline');
  const lost=await page.evaluate(async()=>{document.querySelector('#view').getContext('webgl2').getExtension('WEBGL_lose_context').loseContext();await new Promise(r=>setTimeout(r,100));return document.querySelector('#view').dataset.rendered;});
  assert(lost==='false' && await selectionPixels()===0,'Context loss retained stale selection geometry');
  await page.evaluate(()=>tokitoInspection.select('U1'));
  assert(await selectionPixels()===0,'Selection redrew stale geometry after context loss');
  return {passed:true,stats,selectableBodies:expectedBodies,photographedBoardMatchVerified:false,checks:['reference BOM R6/C6 coverage',expectedBodies+' independent selectors including central U1 module','nonblank synchronous snapshots preserve original GLB material pixels','selection reset clears the overlay','missing CP2102 refusal','offline resistor selection','3840x2160 screenshot','orbit/zoom change selection overlay','resized bitmap dimensions match','context-loss clears and suppresses selection']};
}
