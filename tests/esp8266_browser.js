async(page)=>{
const assert=(ok,msg)=>{if(!ok)throw Error(msg);};
await page.waitForFunction(()=>document.querySelector('#view').dataset.rendered==='true');
const refs=await page.locator('#list button').evaluateAll(bs=>bs.map(b=>b.textContent.split(' · ')[0]));
assert(refs.length===34,'component list');
for(const ref of refs){const r=await page.evaluate(ref=>tokitoInspection.select(ref),ref);assert(r.reference===ref&&r.modelAvailable,'missing '+ref);}
await page.setViewportSize({width:1600,height:1000});await page.locator('#reset').click();
const base=await page.evaluate(()=>tokitoInspection.snapshotImage());
await page.locator('#search').fill('U1');await page.getByRole('button',{name:/^U1 ·/}).click();
assert(await page.evaluate(()=>tokitoInspection.selected())==='U1','IC UI select');
assert(base===await page.evaluate(()=>tokitoInspection.snapshotImage()),'selection altered source colors');
const pixels=await page.locator('#selection').evaluate(c=>{const d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;let n=0;for(let i=3;i<d.length;i+=4)if(d[i])n++;return n;});assert(pixels>0,'selection outline');
const box=await page.locator('#view').boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+90,box.y+box.height/2+40,{steps:8});await page.mouse.up();
assert(base!==await page.evaluate(()=>tokitoInspection.snapshotImage()),'orbit');await page.mouse.wheel(0,-150);
await page.setViewportSize({width:3840,height:2160});await page.screenshot({path:'target/prototypes/esp8266-source-assembly-e2e-v5/prototype/esp8266-selected-module-4k.png'});
await page.locator('#reset').click();assert(await page.evaluate(()=>tokitoInspection.selected())===null,'reset');
const html=await page.content();await page.context().setOffline(true);await page.setContent(html);await page.waitForFunction(()=>document.querySelector('#view').dataset.rendered==='true');
await page.evaluate(()=>tokitoInspection.select('D1'));assert(await page.evaluate(()=>tokitoInspection.selected())==='D1','offline diode');
const stats=await page.evaluate(()=>({components:tokitoInspection.componentCount,triangles:tokitoInspection.triangleCount,draws:tokitoInspection.drawCount,error:document.querySelector('#error').textContent}));assert(!stats.error&&stats.draws===35,'render');
await page.context().setOffline(false);
return {passed:true,stats,selectedAll:refs.length,offlineSelection:true,viewport:[3840,2160],physicalFitVerified:false,arRegistrationVerified:false};
}
