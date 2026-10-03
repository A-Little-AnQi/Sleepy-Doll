import { BrandIcon } from "../web/src/brand/BrandIcon";
import { ease, seg } from "./clock";
import { cueSheet, cueTime } from "./sync";
import "./film-overlays.css";

const norm = (s: string) => s.replace(/[^\p{L}\p{N}]/gu, "").toLowerCase();
const clauses = cueSheet.alignment.flatMap((clip) => {
  const text = norm(clip.text);
  let offset = 0;
  const positions = clip.words.map((word) => {
    const from = offset;
    offset += norm(word.text).length;
    return { ...word, from, to: offset };
  });
  let cursor = 0;
  return (clip.text.match(/[^。！？；，]+[。！？；，]?/g) ?? [clip.text]).map((phrase) => {
    const needle = norm(phrase), index = text.indexOf(needle, cursor);
    const first = positions.find((w) => w.to > index);
    const last = positions.find((w) => w.to >= index + needle.length);
    cursor = index + needle.length;
    return { text: phrase.replace(/[，。]$/, ""), start: first?.start ?? clip.start,
      end: Math.min(clip.end, (last?.end ?? clip.end) + 180) };
  });
});

// 标题只承担开篇与收束；句内动作由产品对象完成。
export function FilmOverlays({ time }: { time: number }) {
  const phrase = clauses.find((item) => time >= item.start - 30 && time < item.end);
  const intro = ease.camera(seg(time, cueTime("intro"), cueTime("intro") + 350));
  const product = ease.camera(seg(time, cueTime("intro-product"), cueTime("intro-product") + 350));
  const reveal = ease.transition(seg(time, 3350, 4950));
  const outro = time >= 56450;
  const brand = ease.camera(seg(time, cueTime("brand"), cueTime("brand") + 420));
  return (
    <>
      {time < 5000 && <div className="film-opening" style={{ clipPath: `inset(0 0 0 ${reveal * 100}%)` }}>
        <div className="opening-brand"><BrandIcon /><span>Sleepy Doll</span></div>
        <div className="opening-title">
          <h1 style={{ opacity: intro, transform: `translateY(${(1 - intro) * 90}px)` }}>一句话</h1>
          <h2 style={{ opacity: product, transform: `translateX(${(1 - product) * -100}px)` }}>安排日常任务</h2>
        </div>
        <svg className="opening-route" viewBox="0 0 1920 1080">
          <path d="M240 740 H1180 Q1320 740 1320 870 H1670" pathLength="1" strokeDasharray="1"
            strokeDashoffset={1 - ease.camera(seg(time, cueTime("intro-product"), 3750))} />
          <circle cx={240 + ease.camera(seg(time, cueTime("intro-product"), 3750)) * 1430} cy="740" r="7" />
        </svg>
        <div className="opening-target" style={{ opacity: product }}>BetterGI</div>
      </div>}
      {time >= 41950 && time < 56450 && <div className="film-feature-label">
        <span>Sleepy Doll</span><strong>{time < 49200 ? "模型接入" : time < 53300 ? "技能与插件" : "本地桥"}</strong>
      </div>}
      {time >= cueTime("run-again") - 200 && time < cueTime("reused") + 1600 && <div className="film-reuse-label"
        style={{ opacity: ease.camera(seg(time, cueTime("run-again"), cueTime("run-again") + 220)) }}>
        <span>下次</span><strong style={{ opacity: ease.camera(seg(time, cueTime("reused"), cueTime("reused") + 200)) }}>直接运行已有项</strong>
      </div>}
      {outro && <div className="brand-transition" style={{ opacity: ease.camera(seg(time, 56450, 56800)) }}>
        <div className="brand-lockup">
          <div className="brand-icon-holder"><BrandIcon /></div><h2 style={{ clipPath: `inset(0 ${(1 - brand) * 100}% 0 0)` }}>Sleepy Doll</h2>
          <p style={{ opacity: seg(time, cueTime("brand") + 400, cueTime("brand") + 800) }}>本地桌面 Agent</p>
        </div>
        <div className="brand-thread" style={{ transform: `scaleX(${brand})` }} />
      </div>}
      {phrase && <div className="narration-subtitle" key={phrase.start}
        style={{ opacity: Math.min(seg(time, phrase.start - 30, phrase.start + 60), 1 - seg(time, phrase.end - 70, phrase.end)) }}>
        {phrase.text}
      </div>}
      {time > 59650 && <div className="film-final-black" style={{ opacity: seg(time, 59650, 59850) }} />}
    </>
  );
}
