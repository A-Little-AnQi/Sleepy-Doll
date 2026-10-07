import { createHash,createHmac } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { readFile,writeFile,mkdir } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { existsSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { run } from './client-protection.mjs';

const version=process.env.REPLACE_VERSION;
const expected=process.env.REPLACE_EXPECTED_SHA256;
if(version!=='0.1.0'||!/^[a-f0-9]{64}$/.test(expected??''))throw Error('Replacement must name 0.1.0 and its verified original SHA-256');
const account=process.env.CLOUDFLARE_ACCOUNT_ID,bucket=process.env.R2_BUCKET,access=process.env.R2_ACCESS_KEY_ID,secret=process.env.R2_SECRET_ACCESS_KEY;
if(!account||!bucket||!access||!secret||!process.env.CLOUDFLARE_API_TOKEN||!process.env.GH_TOKEN)throw Error('Missing replacement credentials');
const hash=v=>createHash('sha256').update(v).digest('hex');
const hmac=(key,v)=>createHmac('sha256',key).update(v).digest();
const host=`${account}.r2.cloudflarestorage.com`;
async function s3(method,key,body=Buffer.alloc(0),extra={},query={}) {
  const path='/'+[bucket,...key.split('/')].map(encodeURIComponent).join('/');
  const timestamp=new Date().toISOString().replace(/[:-]|\.\d{3}/g,'');const date=timestamp.slice(0,8);
  const headers={host,'x-amz-date':timestamp,'x-amz-content-sha256':hash(body),...extra};
  const names=Object.keys(headers).sort(),signed=names.join(';'),scope=`${date}/auto/s3/aws4_request`;
  const encode=v=>encodeURIComponent(v).replace(/[!'()*]/g,c=>'%'+c.charCodeAt(0).toString(16).toUpperCase());
  const qs=Object.entries(query).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${encode(k)}=${encode(v)}`).join('&');
  const canonical=[method,path,qs,names.map(n=>`${n}:${String(headers[n]).trim()}\n`).join(''),signed,hash(body)].join('\n');
  const keyBytes=hmac(hmac(hmac(hmac('AWS4'+secret,date),'auto'),'s3'),'aws4_request');
  headers.Authorization=`AWS4-HMAC-SHA256 Credential=${access}/${scope}, SignedHeaders=${signed}, Signature=${createHmac('sha256',keyBytes).update(`AWS4-HMAC-SHA256\n${timestamp}\n${scope}\n${hash(canonical)}`).digest('hex')}`;
  const response=await fetch(`https://${host}${path}${qs?'?'+qs:''}`,{method,headers,...(method==='PUT'?{body}:{}),signal:AbortSignal.timeout(120000)});
  if(!response.ok)throw Error(`R2 ${method} ${key}: HTTP ${response.status}`);
  return response;
}
const name=`Sleepy-Doll-${version}-setup.exe`,key=`releases/${version}/${name}`,channelKey='channels/stable.json',archiveKey=`releases/${version}/release.json`;
const previous=await (await s3('GET',channelKey)).json();
if(previous.version!==version||previous.channel!=='stable'||previous.sha256!==expected)throw Error('Current stable release changed; refusing replacement');
const original=Buffer.from(await (await s3('GET',key)).arrayBuffer());
if(hash(original)!==expected)throw Error('Original R2 package does not match stable manifest');
const previousArchive=Buffer.from(await (await s3('GET',archiveKey)).arrayBuffer());
const previousChecksum=Buffer.from(await (await s3('GET',key+'.sha256')).arrayBuffer());
const replacement=await readFile(`dist/${name}`),digest=hash(replacement);
if(digest===expected||replacement.length>256*1024*1024)throw Error('Invalid replacement package');
const jsReport=JSON.parse(await readFile('target/.tmp/client-protection/javascript-report.json','utf8'));
if(!jsReport.some(c=>c.protected&&c.before!==c.after)||jsReport.some(c=>c.excluded&&c.before!==c.after))throw Error('Client/installer protection validation failed');
const nativeReport=JSON.parse(await readFile('target/.tmp/client-protection/native-report.json','utf8'));
if(!nativeReport.length||nativeReport.some(c=>c.plainSha256===c.encodedSha256))throw Error('Native protection report invalid');
const manifest={...previous,size:replacement.length,sha256:digest,publishedAt:new Date().toISOString()};
const meta=(type,sha)=>({'content-type':type,'cache-control':'no-store','x-amz-storage-class':'STANDARD',...(sha?{'x-amz-meta-sha256':sha,'content-disposition':`attachment; filename="${name}"`}:{})});
function gateway(revision,paused) {
  run(process.platform==='win32'?'npm.cmd':'npm',['exec','--yes','--package=wrangler@4.147.0','--','wrangler','deploy','--config','cloudflare/wrangler.jsonc','--keep-vars','--var',`CLIENT_DOWNLOAD_REVISION:${revision}`,'--var',`DOWNLOAD_REPLACE_VERSION:${paused?version:'none'}`]);
}
const channelBytes=Buffer.from(JSON.stringify(manifest));
const budgetPath=existsSync('installer/r2-budget.mjs')?'installer/r2-budget.mjs':'scripts/r2-budget.mjs';
const {parseListing,checkStorageBudget}=await import(pathToFileURL(resolve(budgetPath)));
const objects=[];let token;const tokens=new Set();
do {
  const page=parseListing(await (await s3('GET','',Buffer.alloc(0),{}, {'list-type':'2','max-keys':'1000',...(token?{'continuation-token':token}:{})})).text());
  objects.push(...page.objects);token=page.next;
  if(objects.length>10000||(token&&tokens.has(token)))throw Error('Unsafe R2 inventory');
  if(token)tokens.add(token);
} while(token);
checkStorageBudget(objects,[{key,size:replacement.length},{key:key+'.sha256',size:Buffer.byteLength(`${digest}  ${name}\n`)},{key:archiveKey,size:channelBytes.length},{key:channelKey,size:channelBytes.length}]);
let paused=false,gatewayAttempted=false,mutated=false;
try {
  gatewayAttempted=true;gateway(digest,true);paused=true;mutated=true;
  await s3('PUT',key,replacement,meta('application/octet-stream',digest));
  if(hash(Buffer.from(await (await s3('GET',key)).arrayBuffer()))!==digest)throw Error('Uploaded replacement hash mismatch');
  await s3('PUT',key+'.sha256',Buffer.from(`${digest}  ${name}\n`),meta('text/plain'));
  await s3('PUT',archiveKey,channelBytes,meta('application/json'));
  await writeFile(`dist/${name}.sha256`,`${digest}  ${name}\n`);
  await writeFile(`dist/Sleepy-Doll-${version}-release.json`,JSON.stringify(manifest,null,2)+'\n');
  run('gh',['release','upload',`v${version}`,`dist/${name}`,`dist/${name}.sha256`,`dist/Sleepy-Doll-${version}-release.json`,'--clobber']);
  await s3('PUT',channelKey,channelBytes,meta('application/json'));
  gateway(digest,false);paused=false;
  const url=new URL(manifest.url);url.pathname=url.pathname.replace(/\/([^/]+)$/,'/'+digest+'/$1');
  let verified=false;
  for(let attempt=0;attempt<12;attempt++) {
    const publicResponse=await fetch(url,{signal:AbortSignal.timeout(60000)});
    if(publicResponse.ok) {
      if(hash(Buffer.from(await publicResponse.arrayBuffer()))!==digest)throw Error('Public replacement hash mismatch');
      verified=true;break;
    }
    const body=(await publicResponse.text()).slice(0,200);
    console.log(`Public gateway attempt ${attempt+1}: HTTP ${publicResponse.status} ${body}`);
    if(![503,502,404].includes(publicResponse.status))throw Error(`Public replacement verification failed: HTTP ${publicResponse.status}`);
    await delay(8000);
  }
  if(!verified)throw Error('Public gateway did not finish deployment propagation');
  console.log(JSON.stringify({version,channel:'stable',size:replacement.length,sha256:digest,publicVerified:true}));
} catch(error) {
  if(!mutated) { if(gatewayAttempted)gateway(expected,false); throw error; }
  console.error('Replacement failed; restoring verified original release');
  await s3('PUT',key,original,meta('application/octet-stream',expected));
  await s3('PUT',key+'.sha256',previousChecksum,meta('text/plain'));
  await s3('PUT',archiveKey,previousArchive,meta('application/json'));
  await s3('PUT',channelKey,Buffer.from(JSON.stringify(previous)),meta('application/json'));
  const rollback=resolve('target/.tmp/client-protection/rollback');await mkdir(rollback,{recursive:true});
  await writeFile(join(rollback,name),original);await writeFile(join(rollback,name+'.sha256'),previousChecksum);
  await writeFile(join(rollback,`Sleepy-Doll-${version}-release.json`),previousArchive);
  run('gh',['release','upload',`v${version}`,join(rollback,name),join(rollback,name+'.sha256'),join(rollback,`Sleepy-Doll-${version}-release.json`),'--clobber']);
  gateway(expected,false);paused=false;
  throw error;
} finally {
  if(paused) console.error('Gateway still paused for this version; recovery must finish before download resumes');
}
