import { readFile,writeFile,readdir,mkdir } from 'node:fs/promises';
import { join,resolve } from 'node:path';
import { createHash } from 'node:crypto';

const scratch=resolve('target/.tmp/client-protection');
if(process.argv.includes('--source')) {
  let cargo=await readFile('Cargo.toml','utf8');
  if(!cargo.includes('client-protection ='))cargo=cargo.replace('[features]','[features]\nclient-protection = ["dep:obfstr"]');
  if(!cargo.includes('client-resources-test ='))cargo=cargo.replace('[features]','[features]\nclient-resources-test = ["client-protection"]');
  if(!cargo.includes('obfstr ='))cargo=cargo.replace('[dependencies]','[dependencies]\nobfstr = { version = "=0.4.6", optional = true }');
  await writeFile('Cargo.toml',cargo);
  let lib=await readFile('src/lib.rs','utf8');
  if(!lib.includes('mod protected_resources_validation;'))lib+='\n#[cfg(all(test, feature = "client-resources-test"))]\n#[path = "protected_ui_assets.rs"]\nmod protected_resources_validation;\n';
  await writeFile('src/lib.rs',lib);
  let main=await readFile('src/main.rs','utf8');
  if(!main.includes('mod protected_ui_assets;')) {
    main=main.replace('use rust_embed::RustEmbed;','#[cfg(not(feature = "client-protection"))]\nuse rust_embed::RustEmbed;');
    main=main.replace(/#\[derive\(RustEmbed\)\]\r?\n#\[folder = "target\/ui\/"\]\r?\nstruct UiAssets;/, '#[cfg(not(feature = "client-protection"))]\n#[derive(RustEmbed)]\n#[folder = "target/ui/"]\nstruct UiAssets;\n#[cfg(feature = "client-protection")]\nmod protected_ui_assets;\n#[cfg(feature = "client-protection")]\nuse protected_ui_assets::UiAssets;');
    if(!main.includes('mod protected_ui_assets;'))throw Error('Client asset declaration did not match');
    await writeFile('src/main.rs',main);
  }
  await writeFile('src/protected_ui_assets.rs','include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/.tmp/client-protection/native-assets.rs"));\n');
  let tray=await readFile('src/tray_popup.rs','utf8');
  tray=tray.replace(/<crate::UiAssets as (?:rust_embed::)?RustEmbed>::get/g,'crate::UiAssets::get');
  tray=tray.replace(/^( +)use rust_embed::RustEmbed;/gm,'$1#[cfg(not(feature = "client-protection"))]\n$1use rust_embed::RustEmbed;');
  if(!tray.startsWith('#[cfg(not(feature = "client-protection"))]'))tray='#[cfg(not(feature = "client-protection"))]\nuse rust_embed::RustEmbed;\n'+tray;
  await writeFile('src/tray_popup.rs',tray);
  let runtime=await readFile('src/runtime/mod.rs','utf8');
  if(!runtime.includes('fn protected_core_policy()')) {
    runtime=runtime.replace('message(Role::System, CONTEXT_COMPACTION_PROMPT)', 'message(Role::System, protected_compaction_prompt())');
    runtime=runtime.replace('"{CORE_AGENT_POLICY}\\n\\n用户自定义指令：\\n{}\\n\\n可用资料：\\n{}",', '"{}\\n\\n用户自定义指令：\\n{}\\n\\n可用资料：\\n{}",\n                protected_core_policy(),');
    if(runtime.includes('"{CORE_AGENT_POLICY}'))throw Error('Core policy format did not match');
    runtime=runtime.replace('pub fn configured_agent_instructions(prompt: &str) -> &str {','#[cfg(not(feature = "client-protection"))]\npub fn configured_agent_instructions(prompt: &str) -> &str {');
    runtime+=`
#[cfg(feature = "client-protection")]
fn protected_core_policy() -> &'static str {
    static POLICY: OnceLock<String> = OnceLock::new();
    POLICY.get_or_init(|| { let mut text=String::new(); obfstr::obfstmt! { text.push_str(obfstr::obfstr!(CORE_AGENT_POLICY)); text.shrink_to_fit(); } text }).as_str()
}
#[cfg(not(feature = "client-protection"))]
fn protected_core_policy() -> &'static str { CORE_AGENT_POLICY }
#[cfg(feature = "client-protection")]
fn protected_compaction_prompt() -> &'static str {
    static PROMPT: OnceLock<String> = OnceLock::new();
    PROMPT.get_or_init(|| { let mut text=String::new(); obfstr::obfstmt! { text.push_str(obfstr::obfstr!(CONTEXT_COMPACTION_PROMPT)); text.shrink_to_fit(); } text }).as_str()
}
#[cfg(not(feature = "client-protection"))]
fn protected_compaction_prompt() -> &'static str { CONTEXT_COMPACTION_PROMPT }
#[cfg(feature = "client-protection")]
pub fn configured_agent_instructions(prompt: &str) -> &str {
    let mut legacy=false; let mut result=prompt;
    obfstr::obfstmt! {
        legacy=prompt.starts_with(obfstr::obfstr!("你是 Sleepy Doll，一个操作 BetterGI 的桌面 Agent。"));
        legacy=legacy && (prompt.contains(obfstr::obfstr!("# 接口分两层")) || prompt.contains(obfstr::obfstr!("# 用户配置在文件里")));
        result=if legacy { "" } else { prompt.trim() };
    }
    result
}
#[cfg(all(test, feature = "client-protection"))]
mod protection_contract_tests {
    use super::*;
    #[test] fn protected_policy_keeps_exact_model_instructions() { assert_eq!(protected_core_policy(), CORE_AGENT_POLICY); assert_eq!(protected_compaction_prompt(), CONTEXT_COMPACTION_PROMPT); }
    #[test] fn protected_instruction_filter_preserves_user_text() { assert_eq!(configured_agent_instructions("  自己的要求  "), "自己的要求"); assert_eq!(configured_agent_instructions("你是 Sleepy Doll，一个操作 BetterGI 的桌面 Agent。# 接口分两层"), ""); }
}
`;
    await writeFile('src/runtime/mod.rs',runtime);
  }
}
if(process.argv.includes('--assets')) {
  const input=resolve('target/ui'),output=join(scratch,'assets'); await mkdir(output,{recursive:true});
  const walk=async dir=>{let all=[];for(const entry of await readdir(dir,{withFileTypes:true})){const p=join(dir,entry.name);all.push(...entry.isDirectory()?await walk(p):[p]);}return all;};
  const files=await walk(input),arms=[],reports=[];
  for(let i=0;i<files.length;i++) {
    const file=files[i],name=file.slice(input.length+1).replaceAll('\\','/');
    if(name.endsWith('.map'))throw Error('Source maps must not ship');
    const plain=await readFile(file),key=createHash('sha256').update(name).update(plain).update('sleepy-client-protection').digest();
    const encoded=Buffer.from(plain);for(let n=0;n<encoded.length;n++)encoded[n]^=key[n%key.length];
    const path=join(output,`${i}.bin`);await writeFile(path,encoded);
    const decoded=Buffer.from(encoded);for(let n=0;n<decoded.length;n++)decoded[n]^=key[n%key.length];
    if(!decoded.equals(plain))throw Error('Native resource round-trip failed');
    arms.push(`${JSON.stringify(name)} => { let key=obfstr::obfbytes!(&[${[...key].join(',')}]).to_vec(); let data=include_bytes!(${JSON.stringify(path.replaceAll('\\','/'))}); Some(Asset { data: std::borrow::Cow::Owned(decode(data,&key)) }) },`);
    reports.push({name,bytes:plain.length,plainSha256:createHash('sha256').update(plain).digest('hex'),encodedSha256:createHash('sha256').update(encoded).digest('hex')});
  }
  const source=`pub struct Asset { pub data: std::borrow::Cow<'static,[u8]> }\npub struct UiAssets;\nfn decode(input:&[u8],key:&[u8])->Vec<u8>{let mut data=input.to_vec();let mut position=0usize;while position<data.len(){obfstr::obfstmt!{data[position]^=key[position%key.len()];position+=1;}}data}\nimpl UiAssets{pub fn get(name:&str)->Option<Asset>{match name{${arms.join('\n')}_=>None}}}\n#[cfg(test)]mod tests{use super::*;#[test]fn resource_decoder_roundtrips(){let expected=include_bytes!(${JSON.stringify(join(input,'index.html').replaceAll('\\','/'))});let actual=UiAssets::get("index.html").unwrap();assert_eq!(actual.data.as_ref(),expected.as_slice());}}`;
  await writeFile(join(scratch,'native-assets.rs'),source);
  await writeFile(join(scratch,'native-report.json'),JSON.stringify(reports,null,2));
  console.log(`Protected ${files.length} native client resources`);
}
if(process.argv.includes('--verify')) {
  const binary=await readFile('target/release/sleepy-doll.exe');
  for(const text of ['你是 Sleepy Doll，一个本地桌面助手。使用简体中文。','你负责把一段较早的 Agent 对话压缩成可继续工作的检查点。'])
    if(binary.includes(Buffer.from(text)))throw Error('Protected native instructions are present as plaintext');
  const report=JSON.parse(await readFile(join(scratch,'native-report.json'),'utf8'));
  if(!report.length||report.some(r=>r.plainSha256===r.encodedSha256))throw Error('Native resources were not transformed');
  const index=await readFile('target/ui/index.html');
  if(binary.includes(index))throw Error('Client HTML still embedded as plaintext');
  console.log('Native executable has protected instructions and transformed UI resources');
}
