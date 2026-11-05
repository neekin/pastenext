// Drives the installed desktop client and its embedded WebView2, not a browser mock.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs/promises';
import path from 'node:path';
const exec=promisify(execFile),reportDir=process.argv[2];await fs.mkdir(reportDir,{recursive:true});
const pause=ms=>new Promise(r=>setTimeout(r,ms));
async function until(work,predicate,timeout=30000){const end=Date.now()+timeout;let value;while(Date.now()<end){try{value=await work();if(predicate(value))return value;}catch{}await pause(200);}throw Error('Timed out: '+JSON.stringify(value));}
const targets=await until(()=>fetch('http://127.0.0.1:9222/json/list').then(r=>r.json()),a=>a.some(t=>t.url.includes('studio.html')),45000);
const target=targets.find(t=>t.url.includes('studio.html'));
const socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
let sequence=0;const waiting=new Map();socket.addEventListener('message',event=>{const r=JSON.parse(event.data);if(r.id&&waiting.has(r.id)){const p=waiting.get(r.id);waiting.delete(r.id);r.error?p.reject(r.error):p.resolve(r.result);}});
async function cdp(method,params){const id=++sequence;const result=new Promise((resolve,reject)=>{waiting.set(id,{resolve,reject});setTimeout(()=>{if(waiting.delete(id))reject(Error(method+' timed out'));},30000).unref();});socket.send(JSON.stringify({id,method,params}));return result;}
async function evaluate(expression){const r=await cdp('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.text+': '+JSON.stringify(r.exceptionDetails.exception?.description));return r.result.value;}
const invoke=(name,args={})=>evaluate('window.__TAURI_INTERNALS__.invoke('+JSON.stringify(name)+','+JSON.stringify(args)+')');
const input=action=>exec('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',path.resolve('scripts/native-input-windows.ps1'),'-Action',action],{windowsHide:true});
const report={};
try{
 await invoke('get_settings');
 await input('capture');await until(()=>exec('powershell.exe',['-NoProfile','-Command',"@(Get-Process capture_selector -ErrorAction SilentlyContinue).Count"],{windowsHide:true}),r=>Number(r.stdout.trim())>0);
 await input('region');
 const image=await until(()=>invoke('media_list'),items=>items.some(a=>a.kind==='image'&&a.width===180&&a.height===120));
 report.nativeScreenshot=image.find(a=>a.kind==='image'&&a.width===180&&a.height===120);delete report.nativeScreenshot.path;
 await input('record');await pause(700);await input('screen');await pause(2400);await input('record');
 await until(()=>invoke('capture_status'),s=>!s.busy);
 const assets=await invoke('media_list'),video=assets.find(a=>a.kind==='video');if(!video)throw Error('Native recording was not saved');
 report.recording={saved:true,extension:path.extname(video.path)};
 report.videoPlayback=await until(()=>evaluate('(()=>{const v=document.querySelector("video");return v&&{ready:v.readyState,width:v.videoWidth,height:v.videoHeight,error:v.error?.message}})()'),v=>v?.ready>=2&&v.width>0,20000);
 const fixture=await evaluate(`(async()=>{const c=document.createElement('canvas');c.width=800;c.height=220;const x=c.getContext('2d');x.fillStyle='white';x.fillRect(0,0,800,220);x.fillStyle='#182036';x.font='bold 34px sans-serif';x.fillText('PasteNext Windows 12345',30,100);return window.__TAURI_INTERNALS__.invoke('media_commit_image',{dataUrl:c.toDataURL(),name:'Native Windows test'});})()`);
 try{const text=await invoke('media_ocr',{id:fixture.id});report.ocr={available:!!text,recognizedTestText:text.includes('12345')};}catch(e){report.ocr={available:false,error:String(e)};}
 const session=await invoke('transfer_start',{id:fixture.id});
 const url=session.urls[session.activeIndex], bytes=await fs.readFile(fixture.path);
 const download=await fetch(url+'download');if(!download.ok)throw Error('Native transfer download failed');report.download=Buffer.from(await download.arrayBuffer()).equals(bytes);
 const upload=await fetch(url+'upload',{method:'POST',body:bytes,headers:{'Content-Type':'image/png','Origin':new URL(url).origin}});report.upload=upload.ok;
 await invoke('transfer_stop');await pause(150);try{await fetch(url);report.revoked=false;}catch{report.revoked=true;}
 if(!report.download||!report.upload||!report.revoked)throw Error('Installed LAN transfer checks failed');
 report.bundle=await invoke('plugin:app|identifier');if(report.bundle!=='io.pastenext.preview')throw Error('Expected an isolated Preview application');
 report.ok=true;console.log(JSON.stringify(report));
}catch(e){report.ok=false;report.error=String(e);console.error(report.error);process.exitCode=1;}
finally{await fs.writeFile(path.join(reportDir,'native-client-report.json'),JSON.stringify(report,null,2));socket.close();}
