import character from "../assets/moon-character-v5-feet.png";
import "./design-landing.css";
import { useRef, useState, type ReactNode } from "react";
import type { Bootstrap } from "../types";
import type { Page } from "../App";

// All furniture is vector geometry. The only raster is the existing character asset.
function Star({ x = 0, y = 0, size = 20 }: { x?: number; y?: number; size?: number }) {
  return <g transform={`translate(${x} ${y}) scale(${size / 40})`}>
    <path d="M0-40 9-12 29-29 12-9 40 0 12 9 29 29 9 12 0 40-9 12-29 29-12 9-40 0-12-9-29-29-9-12Z" fill="url(#ivory)" stroke="#ae8a53" strokeWidth="2" />
    <path d="M0-32 0 0 32 0 8 7 0 32 0 0-32 0-8-7Z" fill="#d4b98d" opacity=".65" />
  </g>;
}

function Bow({ x, y, scale = 1, rotate = 0 }: { x: number; y: number; scale?: number; rotate?: number }) {
  return <g transform={`translate(${x} ${y}) rotate(${rotate}) scale(${scale})`} stroke="#681724" strokeWidth="1.2" filter="url(#fabric-shadow)">
    <path d="M-8 9C-38 28-47 91-77 124L-106 111-121 126C-90 69-71 31-18-4Z" fill="url(#silk)" />
    <path d="M8 5C52 17 68 64 111 91L139 76 138 106C79 88 34 55 12 24Z" fill="url(#silk)" />
    <path d="M-9-3C-37-41-80-57-96-35-111-13-62 26-13 17Z" fill="url(#silk)" />
    <path d="M9-3C42-43 72-47 87-24 98-5 61 24 12 17Z" fill="url(#silk)" />
    <path d="M-12 7C-41-10-65-28-87-30M13 7C38-4 60-19 78-25M-19 23C-44 43-60 80-87 111" fill="none" stroke="#e26665" opacity=".6" />
    <path d="M-15 12Q-61 2-93-28M15 12Q55 10 83-19M-23 27Q-48 71-89 114M24 29Q75 72 123 88" fill="none" stroke="#eac3a0" strokeWidth=".8" opacity=".55" />
    <path d="M-14 4Q-57-22-89-34M15 3Q54-28 79-26M-25 34Q-54 79-76 102" fill="none" stroke="#4f1122" strokeWidth="3" opacity=".45" />
    <path d="M-14-8Q0-18 14-7L17 20Q0 29-16 17Z" fill="#a82032" />
    <Star size={24} />
    <path d="M0-12 8 0 0 14-8 0Z" fill="#7dc9ed" stroke="#f8e8bc" />
  </g>;
}

function Medallion({ x, y, size = 56 }: { x: number; y: number; size?: number }) {
  return <g transform={`translate(${x} ${y}) scale(${size / 56})`}>
    <circle cy="5" r="58" fill="#0b285b" opacity=".4" />
    <circle r="56" fill="url(#ivory)" stroke="#b79766" strokeWidth="2" filter="url(#relief)" />
    <circle r="54" fill="none" stroke="#fffdf2" strokeWidth="1" />
    <circle r="49" fill="url(#enamel)" stroke="#b9a17a" strokeWidth="2" />
    <circle r="37" fill="none" stroke="#ede3c5" strokeWidth="1.5" />
    <circle r="46" fill="none" stroke="#dce9f4" strokeWidth=".6" />
    <path d="M-39-15A42 42 0 0 1 26-33" fill="none" stroke="#fffdf0" strokeWidth="1.5" opacity=".32" />
    <path d="M-25 35A43 43 0 0 0 39 13" fill="none" stroke="#284e90" strokeWidth="2" opacity=".6" />
    {Array.from({length:8},(_,i)=><g key={i} transform={`rotate(${i*45})`}><path d="M0-49 0-27" stroke="#fff0d4" /><circle cy="-43" r="3" fill="#fff3de" /></g>)}
    <g filter="url(#relief)"><Star size={39} /></g>
    <path d="M0-59 4-51 0-45-4-51Z M0 59 4 51 0 45-4 51Z" fill="#f7ebd4" stroke="#ae8a53" />
  </g>;
}

function Panel({ x, y, w, h }: { x: number; y: number; w: number; h: number }) {
  const outline = `M${x+18} ${y}H${x+w-22}Q${x+w-20} ${y+14} ${x+w} ${y+17}V${y+h-18}Q${x+w-20} ${y+h-14} ${x+w-22} ${y+h}H${x+18}Q${x+17} ${y+h-15} ${x} ${y+h-18}V${y+17}Q${x+17} ${y+14} ${x+18} ${y}Z`;
  return <g><path d={outline} fill="#0b254e" opacity=".24" transform="translate(4 6)" /><path d={outline} fill="url(#ivory)" stroke="#c5aa81" strokeWidth="2" filter="url(#paper-grain)" /><path d={outline} fill="none" stroke="#fffaf0" strokeWidth="1" transform={`translate(${x+w/2} ${y+h/2}) scale(.989 .965) translate(${-x-w/2} ${-y-h/2})`} /><path d={outline} fill="none" stroke="#c3a680" strokeWidth=".7" transform={`translate(${x+w/2} ${y+h/2}) scale(.965 .9) translate(${-x-w/2} ${-y-h/2})`} /><path d={`M${x+30} ${y+8}H${x+w-34}`} stroke="#fffefa" strokeWidth="1.5" opacity=".8"/></g>;
}

function SealKey() {
  return <g transform="translate(1270 835)">
    <circle cy="5" r="58" fill="#173363" opacity=".4" />
    <circle r="57" fill="url(#metal)" stroke="#e4c99a" strokeWidth="3" />
    <circle r="49" fill="none" stroke="#f4dfb4" />
    <circle r="43" fill="none" stroke="#68431e" strokeWidth="2" />
    <circle r="53" fill="none" stroke="#684822" strokeWidth=".7"/>
    <path d="M-45-11A47 47 0 0 1 19-43" fill="none" stroke="#ffedc8" strokeWidth="2.5" opacity=".8"/>
    {Array.from({length:16},(_,i)=><g key={i} transform={`rotate(${i*22.5})`}><path d="M0-45v3" stroke="#efd5a0" strokeWidth=".8"/><circle cy="-50" r="1.1" fill="#fff0c6"/></g>)}
    <path d="M-2 3C-11-29-39-20-28-2-17 13-6-3-2-6M2 3C11-29 39-20 28-2 17 13 6-3 2-6M0 3V34H9V27H0" fill="none" stroke="#f8e1b1" strokeWidth="6" strokeLinecap="round" />
    <Star y={-43} size={7}/><Star y={43} size={7}/>
  </g>;
}

interface LandingProps {
  children?: ReactNode;
  page: Page;
  onNew(): void;
  bootstrap: Bootstrap;
  onPage(page: Page): void;
  onConversation(id: string): void;
  onModel(id: string): Promise<void>;
  onSend(prompt: string): Promise<void>;
}

export function DesignLanding({ bootstrap, onPage, onConversation, onModel, onSend, onNew, children, page }: LandingProps) {
  const [prompt, setPrompt] = useState("");
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState("");
  const [changingModel, setChangingModel] = useState(false);
  const input = useRef<HTMLTextAreaElement>(null);
  const submitLock = useRef(false);
  const submit = async () => {
    if (!prompt.trim() || submitLock.current) return;
    submitLock.current = true;
    setSending(true);
    setNotice("");
    try { await onSend(prompt.trim()); }
    catch (error) { setNotice(error instanceof Error ? error.message : String(error)); }
    finally { submitLock.current = false; setSending(false); }
  };
  const suggest = (text: string) => { setPrompt(text); input.current?.focus(); };
  return <section className="design-landing" aria-label="Sleepy Doll 工作空间">
    <svg viewBox="0 0 1586 992" className="design-scene" aria-label="Sleepy Doll 月夜信笺首页">
      <defs>
        <linearGradient id="ivory" x2=".8" y2="1"><stop stopColor="#fffaf2"/><stop offset=".18" stopColor="#eee2d5"/><stop offset=".43" stopColor="#f6eee6"/><stop offset=".75" stopColor="#f3eae0"/><stop offset="1" stopColor="#e4d7c8"/></linearGradient>
        <linearGradient id="silk" x2=".5" y2="1"><stop stopColor="#742536"/><stop offset=".25" stopColor="#9d3442"/><stop offset=".48" stopColor="#ab414b"/><stop offset=".73" stopColor="#8a2d3d"/><stop offset="1" stopColor="#793041"/></linearGradient>
        <linearGradient id="metal" x2="1" y2="1"><stop stopColor="#f2d6a0"/><stop offset=".25" stopColor="#ac7b38"/><stop offset=".55" stopColor="#765025"/><stop offset=".8" stopColor="#bd9453"/><stop offset="1" stopColor="#e8cb91"/></linearGradient>
        <radialGradient id="enamel"><stop stopColor="#93b4d7"/><stop offset="1" stopColor="#637eac"/></radialGradient>
        <radialGradient id="night"><stop stopColor="#4165a5"/><stop offset=".6" stopColor="#254787"/><stop offset="1" stopColor="#142f67"/></radialGradient>
        <radialGradient id="character-mask"><stop offset=".69" stopColor="white"/><stop offset="1" stopColor="black"/></radialGradient>
        <mask id="character-fade"><rect width="1254" height="1254" fill="url(#character-mask)"/></mask>
        <filter id="glow"><feGaussianBlur stdDeviation="4"/></filter>
        <filter id="paper-grain" x="0" y="0" width="100%" height="100%" colorInterpolationFilters="sRGB"><feTurbulence type="fractalNoise" baseFrequency=".72" numOctaves="3" seed="8" result="noise"/><feColorMatrix in="noise" type="saturate" values="0"/><feComponentTransfer><feFuncA type="linear" slope=".065"/></feComponentTransfer><feBlend in="SourceGraphic" mode="multiply"/></filter>
        <filter id="relief" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx=".8" dy="1.4" stdDeviation=".7" floodColor="#644a28" floodOpacity=".65"/></filter>
        <filter id="fabric-shadow" x="-30%" y="-30%" width="160%" height="160%" colorInterpolationFilters="sRGB"><feTurbulence type="fractalNoise" baseFrequency=".48 .92" numOctaves="2" seed="12" result="weave"/><feColorMatrix in="weave" type="saturate" values="0"/><feComponentTransfer><feFuncA type="linear" slope=".16"/></feComponentTransfer><feComposite in2="SourceAlpha" operator="in"/><feBlend in="SourceGraphic" mode="multiply"/><feDropShadow dx="1" dy="3" stdDeviation="2" floodColor="#241b27" floodOpacity=".24"/></filter>
      </defs>
      <rect width="1586" height="992" fill="url(#night)"/>
      <g fill="none" stroke="#bfa777" strokeWidth=".8" opacity=".4">
        <path d="M410 181C631 190 610 9 773-38M408 169C630 235 716 1 1033-42M390 570C543 353 902 530 1240 730M424 690C755 442 1185 617 1610 955M570 991C775 748 1124 619 1610 693"/>
        <circle cx="580" cy="102" r="72"/><circle cx="580" cy="102" r="48"/><path d="M508 102h144M580 30v144"/>
        <circle cx="532" cy="542" r="82"/><circle cx="532" cy="542" r="64"/>
      </g>
      {Array.from({length:30},(_,i)=><Star key={i} x={440+(i*193)%1125} y={18+(i*97)%681} size={i%7===0?8: i%3===0?4:2}/>)}
      <g transform="translate(1320 133) rotate(12)"><rect width="180" height="125" fill="url(#ivory)" stroke="#c5b18f" strokeWidth="2"/><path d="M0 0 90 72 180 0M0 125 62 55M180 125 117 54" fill="none" stroke="#b7a183"/><Star x={90} y={72} size={18}/></g>
      <g transform="translate(1415 265) rotate(13)"><rect width="182" height="218" fill="url(#ivory)" stroke="#c5b18f" strokeWidth="2"/><rect x="9" y="8" width="164" height="201" fill="none" stroke="#d6c4ac"/>{Array.from({length:10},(_,i)=><path key={i} d={`M22 ${38+i*15}q34 -6 67 0t64 0`} stroke="#bba78e" opacity=".45" fill="none"/>)}</g>
      <g opacity=".7"><Medallion x={1514} y={516} size={62}/></g>
      <g transform="translate(1467 722) rotate(-22)"><path d="M0 0H112V128H0Z" fill="#e7e0d1" stroke="#b9a37d" strokeWidth="8" strokeDasharray="9 4"/><rect x="9" y="9" width="94" height="110" fill="#4568a4" stroke="#e9d9bb"/><path d="M61 30a26 26 0 1 0 8 49 25 25 0 0 1-8-49" fill="#f0e3c8"/><Star x={26} y={25} size={8}/></g>
      <svg className="landing-character" x="646" y="-7" width="790" height="790" viewBox="0 0 1254 1254"><image href={character} width="1254" height="1254" mask="url(#character-fade)"/></svg>
      <g fill="none"><path d="M449 576C422 686 935 616 1173 491S1462 204 1326 279" stroke="#68aaff" strokeWidth="12" filter="url(#glow)"/><path d="M449 576C422 686 935 616 1173 491S1462 204 1326 279" stroke="#c4eeff" strokeWidth="3"/></g>
      <path d="M17 17 390 22 416 45 391 582 359 949 26 972 0 931 4 65Z" fill="#8694ae" stroke="#b8a483" strokeWidth="3"/>
      <path d="M20 13 386 18 398 37 413 43 387 578 352 941 27 955 12 933 0 930 4 43 16 38Z" fill="url(#ivory)" stroke="#d5b88c" strokeWidth="3" filter="url(#paper-grain)"/>
      <path d="M26 25 379 29 383 46 400 51 375 577 341 929 33 941 25 924 12 920 17 51Z" fill="none" stroke="#cdb08a"/>
      <text x="77" y="176" className="design-brand">Sleepy</text><text x="128" y="266" className="design-brand">Doll</text>
      <path d="M336 222a24 24 0 1 1-29 33 25 25 0 0 0 29-33" fill="#90b9eb" stroke="#6486c2"/>
      <Bow x={64} y={56} scale={.78} rotate={-32}/><Bow x={318} y={916} scale={.86} rotate={-28}/>
      <Star x={339} y={73} size={9}/><Star x={346} y={104} size={5}/><Star x={110} y={286} size={6}/>
      <path d="M28 319H349L378 355 352 387 32 395 12 374 18 336Z" fill="url(#silk)" stroke="#bb9562" strokeWidth="2"/>
      <path d="M37 328H344L367 354 345 379H33" fill="none" stroke="#e4bd80"/>
      <Bow x={57} y={347} scale={.43}/><text x="135" y="373" className="design-new">新对话</text>
      <text x="47" y="440" className="design-label">最近对话</text><path d="M156 431H359" stroke="#c2a476"/><Star x={359} y={431} size={11}/>
      {[464,519,574].map(y=><g key={y}><Panel x={40} y={y} w={314} h={44}/><Star x={61} y={y+22} size={8}/></g>)}
      <path d="M35 654H342" stroke="#c3a680"/><Star x={331} y={654} size={4}/>
      {["运行库","BetterGI","模型","能力库","设置"].map((label,i)=><g key={label} transform={`translate(0 ${670+i*54})`}><circle cx="59" r="22" fill="none" stroke="#d1baa0"/>{i===0?<g fill="none" stroke="#a68148"><rect x="48" y="-13" width="22" height="27" rx="2"/><path d="M53-7h12M53-1h12M53 5h8"/></g>:i===1?<Star x={59} size={20}/>:i===2?<g stroke="#a68148" fill="url(#metal)"><path d="m59-17 14 8v18l-14 9-14-9V-9Z"/><path d="m45-9 14 9 14-9M59 0v18" fill="none" stroke="#efdbb6"/></g>:i===3?<g fill="url(#metal)" stroke="#b59459"><path d="M40-12q9-6 19 2 10-8 19-2V13q-9-6-19 0-10-6-19 0Z"/><path d="M59-10V13" fill="none" stroke="#f4e7ca"/></g>:<g><path d="m55-19h8l2 7 7-2 5 6-5 6 5 5-5 7-7-2-2 8h-8l-2-8-7 2-5-7 5-5-5-6 5-6 7 2Z" fill="url(#metal)" stroke="#94723d"/><circle cx="59" r="7" fill="#f5eadb"/></g>}<text x="102" y="7" className="design-nav">{label}</text><path d="M35 26H342" stroke="#d1baa0" opacity=".55"/></g>)}
      <text x="473" y="243" className="design-title">今晚要把哪件事</text><text x="473" y="316" className="design-title design-title-large">交给她？</text>
      <path d="M465 339H735" stroke="#dbc092"/><Star x={735} y={339} size={11}/><Star x={465} y={339} size={7}/>
      <text x="473" y="389" className="design-subtitle">先看清，再动手。</text>
      <Panel x={1298} y={25} w={267} h={65}/><Star x={1340} y={57} size={22}/>
      {[{x:481,cx:495,title:"检查当前状态",sub:"读取游戏现场"},{x:1270,cx:1280,title:"规划下一步",sub:"寻找可靠路线"}].map(a=><g key={a.title}><Panel x={a.x} y={562} w={a.x===481?298:257} h={117}/><Medallion x={a.cx} y={598} size={56}/><text x={a.x+86} y={606} className="design-action">{a.title}</text><path d={`M${a.x+85} 623h${a.x===481?193:145}`} stroke="#c1a172"/><Star x={a.x+89} y={623} size={4}/><text x={a.x+117} y={657} className="design-action-sub">{a.sub}</text></g>)}
      <g stroke="#c5b391"><path d="M451 795 495 721H1398L1449 795V992H451Z" fill="#a4b4d2"/><path d="m451 795 477 171 521-171M451 992l360-222M1449 992l-360-222" fill="none" strokeWidth="2"/><path d="M500 730 932 958 1389 730" fill="url(#ivory)"/><Panel x={531} y={741} w={844} h={202}/></g>
      <path d="M565 808q4-33 24-43-5 22-24 43Zm0 0-7 10m7-10 18-24" fill="none" stroke="#a69b86"/>
      <path d="M498 841Q922 985 1407 841V885Q954 1010 498 884Z" fill="url(#silk)" stroke="#8a2232" strokeWidth="1" filter="url(#fabric-shadow)"/>
      <path d="M501 845Q922 986 1403 845M502 880Q954 1005 1403 881" fill="none" stroke="#b87778" strokeWidth=".7" opacity=".3"/>
      <path d="M509 888Q951 1011 1401 890" fill="none" stroke="#4b3340" strokeWidth="3" opacity=".16"/>
      <Bow x={935} y={938} scale={1.02}/><g transform="translate(935 935)"><path d="M0-37 11-30 23-29 28-17 34-7 30 7 29 20 15 27 4 35-8 31-22 27-27 12-34 1-29-13-25-26-12-29Z" fill="url(#metal)" stroke="#eed5a6" strokeWidth="3"/><ellipse rx="23" ry="29" fill="none" stroke="#eed5a6"/><path d="M0-11a6 6 0 0 0-3 11L-6 13H6L3 0a6 6 0 0 0-3-11Z" fill="#5e4329" stroke="#f0d9a9"/></g>
      <SealKey/>
      <path d="M1053 695C1009 799 1175 766 1228 817" fill="none" stroke="#6cb2ff" strokeWidth="10" filter="url(#glow)"/><path d="M1053 695C1009 799 1175 766 1228 817" fill="none" stroke="#dcf8ff" strokeWidth="3"/><Star x={1053} y={695} size={20}/><Star x={1228} y={817} size={17}/>
      <foreignObject x="1370" y="37" width="177" height="42">
        <select className="landing-model" aria-label="激活模型" value={bootstrap.models.find(m=>m.active)?.id ?? ""} disabled={changingModel || sending} onChange={async e=>{
          setChangingModel(true); setNotice("");
          try { await onModel(e.target.value); } catch(error) { setNotice(String(error)); } finally { setChangingModel(false); }
        }}>{bootstrap.models.map(m=><option key={m.id} value={m.id}>{m.name}</option>)}</select>
      </foreignObject>
      {!children && <foreignObject x="565" y="765" width="630" height="87">
        <textarea ref={input} className="landing-input" aria-label="给 Agent 的消息" placeholder="给 Sleepy Doll 一个目标…" value={prompt} disabled={sending} onChange={e=>setPrompt(e.target.value)} onKeyDown={e=>{
          if(e.key==='Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {e.preventDefault(); void submit();}
        }}/>
      </foreignObject>}
      {!children && <foreignObject x="1210" y="775" width="120" height="120"><button className="landing-hit landing-send" aria-label={sending ? "正在发送" : "发送"} disabled={sending || changingModel || !prompt.trim()} onClick={()=>void submit()} /></foreignObject>}
      <foreignObject x="15" y="316" width="365" height="84"><button className="landing-hit" aria-label="新对话" onClick={()=>{onNew();setPrompt("");setNotice("");input.current?.focus();}} /></foreignObject>
      {!children && <foreignObject x="442" y="542" width="340" height="140"><button className="landing-hit" aria-label="检查当前状态" onClick={()=>suggest("检查 BGI 是否已连接，并读取当前游戏状态")} /></foreignObject>}
      {!children && <foreignObject x="1218" y="542" width="313" height="140"><button className="landing-hit" aria-label="规划下一步" onClick={()=>suggest("查找目前可用于运行路线的能力")} /></foreignObject>}
      {([{page:'library',label:'运行库'},{page:'bridge',label:'BetterGI'},{page:'models',label:'模型'},{page:'extensions',label:'能力库'},{page:'settings',label:'设置'}] as const).map((item,i)=><foreignObject key={item.page} x="33" y={644+i*54} width="306" height="51"><button className={`landing-hit ${page===item.page?'is-active':''}`} aria-label={item.label} onClick={()=>onPage(item.page)} /></foreignObject>)}
      <foreignObject x="78" y="469" width="252" height="150"><nav className="landing-history" aria-label="最近对话">{bootstrap.conversations.slice(0,3).map(c=><button key={c.id} title={c.title} onClick={()=>onConversation(c.id)}>{c.title}</button>)}</nav></foreignObject>
      {notice && <foreignObject x="545" y="689" width="810" height="48"><div className="landing-notice" role="alert">{notice}</div></foreignObject>}
      {children && <g><Panel x={442} y={115} w={1100} h={837}/><foreignObject x="465" y="136" width="1054" height="793"><div className={`product-content product-${page}`}>{children}</div></foreignObject></g>}
    </svg>
  </section>;
}
