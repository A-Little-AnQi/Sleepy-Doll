import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile, copyFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

const root = process.cwd();
export const scratch = resolve(root, 'target/.tmp/client-protection');
const hash = code => createHash('sha256').update(code).digest('hex');
export function run(exe,args) {
  const result=spawnSync(exe,args,{cwd:root,stdio:'inherit',windowsHide:true,shell:process.platform==='win32' && exe.endsWith('.cmd')});
  if(result.status!==0) throw Error(`${exe} failed (${result.status})`);
}
export async function prepare() {
  process.env.DOTNET_GENERATE_ASPNET_CERTIFICATE='false';
  process.env.DOTNET_CLI_TELEMETRY_OPTOUT='1';
  await mkdir(join(scratch,'runtime'),{recursive:true});
  run(process.platform==='win32'?'npm.cmd':'npm',['install','--prefix',join(scratch,'tools'),'--no-save','--package-lock=false','--no-audit','--no-fund','javascript-obfuscator@5.9.0']);
  if(!existsSync(join(scratch,'obfuscar','obfuscar.console.exe')))
    run('dotnet',['tool','install','Obfuscar.GlobalTool','--version','2.2.50','--tool-path',join(scratch,'obfuscar')]);
}
export function clientProtectionPlugin() {
  return {name:'sleepy-client-protection',apply:'build',enforce:'post',generateBundle:async function(_options,bundle) {
    const obfuscator=createRequire(join(scratch,'tools','entry.cjs'))('javascript-obfuscator');
    const reachable=(entry)=>{const seen=new Set();const visit=name=>{if(seen.has(name))return;seen.add(name);const c=bundle[name];if(c?.type==='chunk')for(const imported of [...c.imports,...c.dynamicImports])visit(imported);};if(entry)visit(entry.fileName);return seen;};
    const chunks=Object.values(bundle).filter(c=>c.type==='chunk');
    const client=reachable(chunks.find(c=>c.isEntry && /\/web\/index\.html$/.test(c.facadeModuleId?.replaceAll('\\','/')??'')));
    const excluded=new Set(chunks.filter(c=>c.isEntry && /\/web\/(?:setup|tray)\.html$/.test(c.facadeModuleId?.replaceAll('\\','/')??'')).flatMap(c=>[...reachable(c)]));
    const report=[];
    for(const c of chunks) {
      const before=hash(c.code), protect=client.has(c.fileName)&&!excluded.has(c.fileName);
      if(protect) c.code=obfuscator.obfuscate(c.code,{
        target:'browser',seed:71001,compact:true,ignoreImports:true,
        identifierNamesGenerator:'hexadecimal',renameGlobals:false,renameProperties:false,
        stringArray:true,stringArrayEncoding:['base64'],stringArrayThreshold:1,
        stringArrayRotate:true,stringArrayShuffle:true,stringArrayIndexShift:true,
        controlFlowFlattening:true,controlFlowFlatteningThreshold:0.12,
        deadCodeInjection:false,debugProtection:false,selfDefending:false,
        disableConsoleOutput:false,sourceMap:false,advertisement:false,
      }).getObfuscatedCode();
      report.push({file:c.fileName,protected:protect,excluded:excluded.has(c.fileName),before,after:hash(c.code)});
    }
    if(!report.some(c=>c.protected&&c.before!==c.after))throw Error('No client entry was protected');
    if(report.some(c=>c.excluded&&c.before!==c.after))throw Error('Installer/tray JavaScript changed');
    await mkdir(scratch,{recursive:true});
    await writeFile(join(scratch,'javascript-report.json'),JSON.stringify(report,null,2));
    console.log(`Protected ${report.filter(c=>c.protected).length} client chunks; installer/tray dependency graph retained`);
  }};
}
export async function managed() {
  const input=resolve(root,'target/bridge'), output=join(scratch,'managed');
  await mkdir(output,{recursive:true});
  const runtimes=spawnSync('dotnet',['--list-runtimes'],{encoding:'utf8',windowsHide:true}).stdout;
  const paths=[input,...runtimes.split(/\r?\n/).map(line=>/^(\S+) (8\.\S+) \[(.+)\]$/.exec(line)).filter(Boolean).map(m=>join(m[3],m[2]))];
  const escape=value=>value.replaceAll('&','&amp;').replaceAll('"','&quot;');
  const modules=['BgiBridge.dll','BgiBridge.Recovery.dll'];
  // Anonymous objects are JSON wire contracts. Their constructor parameter names
  // must survive as well as their properties (System.Text.Json uses both).
  const xml=`<Obfuscator><Var name="InPath" value="${escape(input)}"/><Var name="OutPath" value="${escape(output)}"/><Var name="KeepPublicApi" value="true"/><Var name="HidePrivateApi" value="true"/><Var name="HideStrings" value="true"/><Var name="RenameProperties" value="false"/><Var name="KeepProperties" value="true"/><Var name="RenameFields" value="false"/><Var name="RenameEvents" value="false"/><Var name="UseUnicodeNames" value="false"/>${paths.map(p=>`<AssemblySearchPath path="${escape(p)}"/>`).join('')}${modules.map(m=>`<Module file="${escape(join(input,m))}"><SkipType name="*NativeMethods" skipMethods="true" skipFields="true"/><SkipType rx=".*AnonymousType.*" skipMethods="true" skipFields="true" skipProperties="true"/><SkipMethod type="*" name=".ctor"/></Module>`).join('')}</Obfuscator>`;
  const config=join(scratch,'obfuscar.xml');await writeFile(config,xml);
  run(join(scratch,'obfuscar','obfuscar.console.exe'),[config]);
  run('dotnet',['run','--project','tests/client-protection/ClientProtectionChecks.csproj','-c','Release','--',input,output]);
  for(const name of ['BgiBridge.Recovery.runtimeconfig.json','BgiBridge.Recovery.deps.json'])
    await copyFile(join(input,name),join(output,name));
  for(const [args,expectedExit,check] of [
    [['origin',join(scratch,'absent-host.exe')],0,v=>v.ok===true&&v.result.state==='unknown'],
    [['status','[]'],0,v=>v.ok===true],
    [['invalid-request'],1,v=>v.ok===false&&typeof v.error?.message==='string'],
  ]) {
    const result=spawnSync('dotnet',[join(output,'BgiBridge.Recovery.dll'),...args],{cwd:output,encoding:'utf8',windowsHide:true,timeout:12000});
    let value;try {value=JSON.parse(result.stdout);}catch {throw Error(`Protected recovery did not return JSON: ${result.stderr}`);}
    if(result.status!==expectedExit||!check(value))throw Error(`Protected recovery contract failed: ${result.stdout} ${result.stderr}`);
  }
  console.log('Protected recovery origin, status and error JSON processes passed');
  await writeFile(join(scratch,'managed-report.json'),JSON.stringify({revision:'json-contracts-v2',recoveryProcessChecks:['origin','status','error'],files:await Promise.all(modules.map(async name=>({name,sha256:hash(await readFile(join(output,name)))})))},null,2));
  for(const name of modules) await copyFile(join(output,name),join(input,name));
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  if(process.argv.includes('--prepare'))await prepare();
  if(process.argv.includes('--managed'))await managed();
}
