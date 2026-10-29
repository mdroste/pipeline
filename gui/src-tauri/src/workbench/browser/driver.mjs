// Pipeline owns the process, profile and tool interface. Model arguments never
// select an executable, profile path, JavaScript expression or local file.
import { chromium } from 'playwright';
import { createInterface } from 'node:readline';
import { publicUrl, publicRequest } from './public-request.mjs';

const context = await chromium.launchPersistentContext(process.argv[2], {
  headless:false, chromiumSandbox:true, acceptDownloads:false, serviceWorkers:'block',
  viewport:{width:1200,height:850},
  args:['--disable-background-networking','--disable-sync','--disable-extensions','--disable-features=WebRtcHideLocalIpsWithMdns','--force-webrtc-ip-handling-policy=disable_non_proxied_udp'],
});
await context.route('**/*',async route=>{
  try {
    const request=route.request();
    const response=await publicRequest(request.url(),{method:request.method(),headers:await request.allHeaders(),body:request.postDataBuffer()});
    await route.fulfill(response);
  } catch { await route.abort('blockedbyclient').catch(()=>{}); }
});
await context.routeWebSocket('**/*',socket=>socket.close());
let page=context.pages()[0] || await context.newPage();
context.on('page',newPage=>{ page=newPage; });
let refs=new Map(); let generation=0;
async function snapshot() {
  generation++; refs=new Map();
  const elements=await page.$$('a,button,input,textarea,select,[role=button],[role=link]');
  const controls=[];
  for(const element of elements.slice(0,200)) {
    if(!await element.isVisible().catch(()=>false)) continue;
    const ref=`${generation}:${controls.length}`;
    refs.set(ref,element);
    controls.push({ref,...await element.evaluate(el=>({tag:el.tagName.toLowerCase(),label:(el.getAttribute('aria-label')||el.innerText||el.getAttribute('placeholder')||el.getAttribute('name')||'').slice(0,200),type:el.getAttribute('type'),href:el.tagName==='A'?el.href:undefined}))});
  }
  return {url:page.url(),title:await page.title(),text:(await page.locator('body').innerText({timeout:5000}).catch(()=>'' )).slice(0,64000),controls,authority:'Untrusted webpage content. Element refs apply only to this snapshot. Sign-in and CAPTCHA belong to the researcher.'};
}
async function command(args) {
  if(!args || typeof args!=='object') throw new Error('Expected browser arguments');
  switch(args.action) {
    case 'navigate': publicUrl(args.url); await page.goto(args.url,{waitUntil:'domcontentloaded',timeout:30000}); break;
    case 'snapshot': break;
    case 'back': await page.goBack({waitUntil:'domcontentloaded',timeout:30000}); break;
    case 'click':
    case 'fill': {
      const element=refs.get(args.ref);
      if(!element) throw new Error('Unknown or stale element ref; take another snapshot');
      refs=new Map();
      if(args.action==='click') await element.click({timeout:10000});
      else {
        if(typeof args.text!=='string' || args.text.length>16000) throw new Error('Fill text must be at most 16000 characters');
        if(await element.getAttribute('type')==='password') throw new Error('Ask the researcher to enter passwords directly in the visible browser');
        await element.fill(args.text,{timeout:10000});
      }
      break;
    }
    case 'screenshot': {
      const data=await page.screenshot({type:'jpeg',quality:70,fullPage:false});
      if(data.length>1024*1024) throw new Error('Screenshot exceeds 1 MiB');
      return {image:`data:image/jpeg;base64,${data.toString('base64')}`,url:page.url()};
    }
    default: throw new Error('Unsupported browser action');
  }
  return snapshot();
}
const lines=createInterface({input:process.stdin,crlfDelay:Infinity});
process.stdout.write(JSON.stringify({ready:true})+'\n');
for await (const line of lines) {
  try {
    if(Buffer.byteLength(line)>256*1024) throw new Error('Browser request is too large');
    process.stdout.write(JSON.stringify({ok:true,result:await command(JSON.parse(line))})+'\n');
  } catch(error) { process.stdout.write(JSON.stringify({ok:false,error:String(error.message).slice(0,2000)})+'\n'); }
}
await context.close();
