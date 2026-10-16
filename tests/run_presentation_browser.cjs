// Local QA only; uses an explicitly supplied installed Playwright module.
// node tests/run_presentation_browser.cjs <module> <prototype-root> <new-output-dir>
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {pathToFileURL} = require('node:url');

(async () => {
  if(process.argv.length !== 5) throw Error('Supply installed Playwright module, prototype root and new output directory');
  const {chromium} = require(path.resolve(process.argv[2]));
  const sourceRoot = path.resolve(process.argv[3]);
  const outputRoot = path.resolve(process.argv[4]);
  const testFile = path.join(__dirname,'presentation_browser.js');
  const test = eval('('+fs.readFileSync(testFile,'utf8')+')');
  const sha = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  fs.mkdirSync(outputRoot); // Refuse reused output/receipts.
  const boards = fs.readdirSync(sourceRoot,{withFileTypes:true}).filter(d=>d.isDirectory()&&fs.existsSync(path.join(sourceRoot,d.name,'index.html'))).map(d=>d.name).sort();
  if(!boards.length) throw Error('No prototype viewers');
  process.chdir(outputRoot);
  const receipt={schema:'tokito.presentation-browser-qa.v1',passed:false,testSha256:sha(testFile),boards:[],geometryQualified:false};
  for(const board of boards){
    let browser;
    try{
      browser=await chromium.launch({channel:'msedge',headless:true,args:['--disable-gpu-shader-disk-cache','--disk-cache-dir='+path.join(outputRoot,'edge-cache')]});
      const context=await browser.newContext({acceptDownloads:true});
      const page=await context.newPage();page.setDefaultTimeout(120000);
      const errors=[];page.on('pageerror',e=>errors.push(e.message));
      const html=path.join(sourceRoot,board,'index.html');
      await page.goto(pathToFileURL(html).href,{waitUntil:'load',timeout:120000});
      const result=await test(page);
      if(errors.length) throw Error('Browser exceptions: '+errors.join('; '));
      const image=path.join(outputRoot,'presentation-'+board+'-4k.png');
      const png=fs.readFileSync(image);
      if(png.readUInt32BE(16)!==3840||png.readUInt32BE(20)!==2160)throw Error('Downloaded PNG is not 4K');
      receipt.boards.push({...result,htmlSha256:sha(html),glbSha256:sha(path.join(sourceRoot,board,'active.digital-twin.glb')),pngSha256:sha(image),browserVersion:browser.version()});
      console.log(JSON.stringify(receipt.boards.at(-1)));
    }catch(e){receipt.boards.push({board,passed:false,error:String(e)});console.error(board+': '+e);break;}
    finally{if(browser)await browser.close();}
  }
  receipt.passed=receipt.boards.length===boards.length&&receipt.boards.every(r=>r.passed);
  fs.writeFileSync(path.join(outputRoot,'receipt.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
  if(!receipt.passed)process.exitCode=1;
})().catch(e=>{console.error(e);process.exitCode=1;});
