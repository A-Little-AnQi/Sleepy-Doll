// 导演段「木偶的牵引线」0-12 秒：A 品牌雕塑+宣言，B 真实输入→句子纸带→真实消息，
// C 真实 RunPlanCard 三行分层→归位。全部画面是 t 的纯函数；实体用 SVG 原品牌路径
// （web/src/brand/icon.svg）+ 分层挤出 + 连续渐变做 2.5D，不再用逐三角 canvas。
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { ProductWindow } from "../components/ProductWindow";
import { bootstrapAt, type FilmSnapshot } from "../film";
import type { Bootstrap } from "../../web/src/ipc/types";
import fixture from "../fixtures/launch-film.json";
import { useClock, seg, lerp, ease } from "../clock";
import "./director.css";

const STAGE_W = 1920;
const STAGE_H = 1080;
/** 真实组件视口 1060×660，阅读缩放 1.39：窗口 1473×918，边距 223/81。 */
const VIEW_W = 1060;
const VIEW_H = 660;
const READ_SCALE = 1.39;
const UI_BOX = { x: 223, y: 81, w: Math.round(VIEW_W * READ_SCALE), h: Math.round(VIEW_H * READ_SCALE) };
const SENTENCE = "运行演示配置组";
const TYPING_GROUPS = ["运行", "演示", "配置组"];
const PROBE_T = 11400;

/** 品牌原路径（icon.svg），组内绕 (11,11) 旋转 -35°。 */
const MOON_D =
  "M17.71 5.01A9 9 0 1 0 17.71 16.99A6.5 6.5 0 1 1 17.71 5.01Z";
const STAR_D =
  "M22.602 11.291Q24 11 22.602 10.709L18.13 9.776L17.124 7.228Q16.6 5.9 16.037 7.213L15.07 9.47L12.813 10.437Q11.5 11 12.813 11.563L15.07 12.53L16.037 14.787Q16.6 16.1 17.124 14.772L18.13 12.224Z";

/** 月牙外半径 250px：icon 单位 → 舞台像素 27.78。星自身中心 (17.2,11)。 */
const MOON_C = { x: 1290, y: 455 };
const MOON_K = 250 / 9;
const STAR_K = 11.5;
// 星自身中心 (17.2,11)：随月牙组旋转 -35° 后落在月牙开口处。
const STAR_POS = {
  x: MOON_C.x + 6.2 * MOON_K * 0.82,
  y: MOON_C.y - 6.2 * MOON_K * 0.42,
};

/** 暂定开发时间线（毫秒），正式制作前按旁白重对齐。 */
const DT = {
  enter: [0, 350] as const,
  yaw: [250, 1100] as const,
  starTighten: [150, 450] as const,
  starRebound: [450, 1150] as const,
  bandHead: [600, 1500] as const,
  title1: [700, 1080] as const,
  title2: [1020, 1400] as const,
  titleOut: [3250, 3900] as const,
  solidsOut: [3400, 4300] as const,
  flatten: [3600, 4600] as const,
  windowIn: [3900, 4700] as const,
  bgOut: [3600, 4400] as const,
  welcomeEnd: [5900, 6500] as const,
  type: [4800, 5650] as const,
  pointerIn: [5500, 5720] as const,
  press: [5780, 5860] as const,
  draftClear: 5900,
  stripHead: [5900, 6500] as const,
  stripTail: [6300, 6650] as const,
  stripChase: [6650, 7000] as const,
  stripFade: [6900, 7100] as const,
  messageIn: [6800, 7300] as const,
  phase: [7150, 7950] as const,
  planEnter: [8000, 8600] as const,
  threads: [8150, 8330, 8510] as const,
  layerIn: 8100,
  swing: [8100, 9100] as const,
  swingBack: [9100, 10600] as const,
  cTitle: [8400, 8900] as const,
};
// 审批镜在 14.2s 才允许提交：0-12s 内第三步始终保持待确认（undefined），
// 第一二步可先后完成读取/确认，不抢跑「已提交」。
const ROW_OUTCOME: Array<[number, number, string | undefined]> = [
  [8200, 8900, "verifiedSucceeded"],
  [9300, 9900, "verifiedSucceeded"],
  [10300, 10800, undefined],
];

/** 导演段目前只覆盖到这个时间；正式成片要求 60000。 */
export const DIRECTOR_COVERAGE_MS = 12000;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const px = (v: number) => v.toFixed(1);

interface Pt {
  x: number;
  y: number;
}

function lerpColor(a: string, b: string, k: number) {
  const pa = a.match(/\w\w/g)!;
  const pb = b.match(/\w\w/g)!;
  const c = pa.map((s, i) =>
    Math.round(lerp(parseInt(s, 16), parseInt(pb[i]!, 16), k)),
  );
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

/** Catmull-Rom 点列 → 单条平滑三次贝塞尔路径（SVG d）。 */
function smoothPath(pts: Pt[]): string {
  let d = `M ${px(pts[0]!.x)} ${px(pts[0]!.y)}`;
  for (let i = 0; i < pts.length - 1; i += 1) {
    const p0 = pts[Math.max(0, i - 1)]!;
    const p1 = pts[i]!;
    const p2 = pts[i + 1]!;
    const p3 = pts[Math.min(pts.length - 1, i + 2)]!;
    d += ` C ${px(p1.x + (p2.x - p0.x) / 6)} ${px(p1.y + (p2.y - p0.y) / 6)}, ${px(p2.x - (p3.x - p1.x) / 6)} ${px(p2.y - (p3.y - p1.y) / 6)}, ${px(p2.x)} ${px(p2.y)}`;
  }
  return d;
}

function typedDraft(t: number) {
  if (t < DT.type[0] || t >= DT.draftClear) return "";
  if (t >= DT.type[1]) return SENTENCE;
  const slot = (DT.type[1] - DT.type[0]) / TYPING_GROUPS.length;
  let text = "";
  for (let i = 0; i < TYPING_GROUPS.length; i += 1) {
    const local = seg(t, DT.type[0] + i * slot, DT.type[0] + (i + 1) * slot - 80);
    if (local <= 0) break;
    const group = TYPING_GROUPS[i]!;
    text += group.slice(0, Math.ceil(group.length * local));
  }
  return text;
}

function directorPlan(t: number): NonNullable<FilmSnapshot["plan"]> {
  return {
    goal: "运行演示配置组（示例）",
    steps: ROW_OUTCOME.map(([active, done, final], i) => ({
      id: `p${i + 1}`,
      title: ["读取状态", "确认项目", "提交运行"][i]!,
      outcome: t >= done ? final : t >= active ? "active" : undefined,
    })),
  };
}

function directorSnapshot(t: number): FilmSnapshot {
  const sent = t >= DT.messageIn[0];
  return {
    page: "chat",
    conversationId: sent ? "conv-director" : undefined,
    welcome: t >= 4300 && t < 5900,
    welcomeFadeOut: seg(t, DT.welcomeEnd[0], DT.welcomeEnd[1]),
    conversationFadeIn: seg(t, DT.messageIn[0], DT.messageIn[1]),
    composerDraft: typedDraft(t),
    messages: sent
      ? [{ role: "user", content: SENTENCE, createdAt: "2026-10-01T10:00:00" }]
      : [],
    phase:
      t >= DT.phase[0] && t < DT.phase[1] ? "正在处理请求" : undefined,
    seconds: sent ? Math.max(1, Math.floor((t - DT.messageIn[0]) / 1000)) : 0,
    task: undefined,
    plan: t >= DT.planEnter[0] ? directorPlan(t) : undefined,
    planEnter: seg(t, DT.planEnter[0], DT.planEnter[1]),
    planShiftPx: 0,
    approvalVisible: false,
    approvalEnter: 0,
    approvalExit: 0,
    saveVisible: false,
    saveDisabled: false,
    savedCardVisible: 0,
    workflows: [],
    workflowBusy: false,
    // 工具事实：真实接口只有 bgi.get_status 与 bgi.run_script_group（提交即返回）。
    toolLabels: {
      "bgi.get_status": "读取状态",
      "bgi.run_script_group": "提交运行",
    },
  };
}

function directorBootstrap(t: number): Bootstrap {
  const base = bootstrapAt(0);
  const sent = t >= DT.messageIn[0];
  const conversation = {
    ...fixture.historyConversation,
    id: "conv-director",
    title: "演示配置组",
    createdAt: "2026-10-01T09:59:30",
    updatedAt: "2026-10-01T10:00:00",
    taskCount: 1,
  };
  return {
    ...base,
    conversations: sent ? [conversation, ...base.conversations] : base.conversations,
    tools: [
      {
        name: "bgi.get_status",
        label: "读取游戏状态",
        description: "读取一次截图器、游戏窗口与任务锁状态。",
        source: "bettergi",
      },
      {
        name: "bgi.run_script_group",
        label: "运行配置组",
        description: "按 groupName 提交运行，默认只等启动交接（waitForCompletion=false）。",
        source: "bettergi",
      },
    ],
    workflows: [],
    tasks: [],
  };
}

/** 未变换基准坐标（舞台像素）里的 UI 矩形；fonts.ready 后在探测层测一次。 */
interface BaseRects {
  composer: Pt & { w: number; h: number };
  send: Pt & { w: number; h: number };
  message: Pt & { w: number; h: number };
  rows: Array<Pt & { w: number; h: number }>;
}

/** 分层挤出的 2.5D 实体：右移的多层轮廓（厚度墙）+ 正面连续渐变。 */
function ExtrudedPath({
  d,
  wall,
  layers,
  wallBack,
  wallFront,
  faceFill,
  rim,
}: {
  d: string;
  wall: number;
  layers: number;
  wallBack: string;
  wallFront: string;
  faceFill: string;
  rim: string;
}) {
  const wallLayers = [];
  for (let i = layers; i >= 1; i -= 1) {
    const k = i / layers;
    wallLayers.push(
      <path
        key={i}
        d={d}
        transform={`translate(${px(wall * k)} 0)`}
        fill={lerpColor(wallFront, wallBack, k)}
      />,
    );
  }
  return (
    <g>
      {wallLayers}
      <path d={d} fill={faceFill} stroke={rim} strokeWidth={0.07} strokeLinejoin="round" />
    </g>
  );
}

export function DirectorFilm() {
  const t = useClock();
  const stageRef = useRef<HTMLDivElement>(null);
  const [base, setBase] = useState<BaseRects | null>(null);

  // 字体就绪后测一次基准布局；动画帧只读缓存，不从已变换 DOM 反推。
  useEffect(() => {
    if (base) return;
    let alive = true;
    document.fonts.ready.then(() => {
      if (!alive) return;
      requestAnimationFrame(() => {
        const stage = stageRef.current;
        const probe = stage?.querySelector<HTMLElement>(".director-probe");
        if (!stage || !probe) return;
        const sr = stage.getBoundingClientRect();
        const scale = sr.width / STAGE_W;
        const pick = (selector: string) => {
          const el = probe.querySelector(selector);
          if (!el) return undefined;
          const b = el.getBoundingClientRect();
          return {
            x: (b.left + b.width / 2 - sr.left) / scale,
            y: (b.top + b.height / 2 - sr.top) / scale,
            w: b.width / scale,
            h: b.height / scale,
          };
        };
        const composer = pick(".composer-dock textarea");
        const send = pick("[data-video-anchor='sd-send']");
        const message = pick(".user-message");
        const rowEls = probe.querySelectorAll(".run-plan-motion li");
        const rows = Array.from(rowEls).map((el) => {
          const b = el.getBoundingClientRect();
          return {
            x: (b.left + b.width / 2 - sr.left) / scale,
            y: (b.top + b.height / 2 - sr.top) / scale,
            w: b.width / scale,
            h: b.height / scale,
          };
        });
        if (!alive || !composer || !send || !message || rows.length < 3) return;
        setBase({ composer, send, message, rows });
      });
    });
    return () => {
      alive = false;
    };
  }, [base]);

  /* ---------------------------- 时间函数 ---------------------------- */
  const enter = ease.camera(seg(t, DT.enter[0], DT.enter[1]));
  const yawDeg = lerp(12, 6, ease.camera(seg(t, DT.yaw[0], DT.yaw[1])));
  const solidsK = seg(t, DT.solidsOut[0], DT.solidsOut[1]);
  const solidsOut = ease.panel(solidsK);
  const solidsOpacity = enter * (1 - seg(t, 3950, 4300));
  const solidsShiftX = solidsOut * 560;
  const solidsShiftY = solidsOut * -26;
  const solidsScale = 1 - solidsOut * 0.05;
  const anticipate =
    ease.transition(seg(t, DT.starTighten[0], DT.starTighten[1])) *
    (1 - seg(t, DT.starTighten[1], DT.starTighten[1] + 260));
  const rebound = ease.camera(seg(t, DT.starRebound[0], DT.starRebound[1]));
  // 星心 → 月牙心方向（收紧）；星心 → 带起点方向（回弹牵带）。
  const toMoon = { x: MOON_C.x - STAR_POS.x, y: MOON_C.y - STAR_POS.y };
  const toMoonLen = Math.hypot(toMoon.x, toMoon.y) || 1;
  const bandStartBase: Pt = { x: 1460, y: 428 };
  const toBand = { x: bandStartBase.x - STAR_POS.x, y: bandStartBase.y - STAR_POS.y };
  const toBandLen = Math.hypot(toBand.x, toBand.y) || 1;
  const starOffset = {
    x:
      (-toMoon.x / toMoonLen) * 8 * anticipate +
      (toBand.x / toBandLen) * 10 * rebound,
    y:
      (-toMoon.y / toMoonLen) * 8 * anticipate +
      (toBand.y / toBandLen) * 10 * rebound,
  };

  const bgK = seg(t, DT.bgOut[0], DT.bgOut[1]);
  const winIn = ease.panel(seg(t, DT.windowIn[0], DT.windowIn[1]));
  const windowVisible = t >= DT.windowIn[0] - 200;
  const sendPressed = t >= DT.press[0] && t < DT.press[1] + 60;
  const pressPulse =
    Math.sin(Math.PI * clamp01(seg(t, DT.press[0], DT.press[1] + 120))) *
    (1 - seg(t, DT.press[1] + 120, DT.press[1] + 360));

  /* 金带：A 段雕塑路径 → B 段压平为输入框底边；t 的纯函数。 */
  const bandPts = ((): Pt[] => {
    if (!base) return [];
    const c = base.composer;
    const bottom = c.y + c.h / 2;
    const left = c.x - c.w / 2;
    const right = c.x + c.w / 2;
    const sculpt: Pt[] = [
      { x: bandStartBase.x + starOffset.x * 0.4, y: bandStartBase.y + starOffset.y * 0.4 },
      { x: 1470, y: 700 },
      { x: 1235, y: 800 },
      { x: 990, y: 805 },
      { x: 790, y: 798 },
      { x: 600, y: 776 },
    ];
    const flat: Pt[] = [
      { x: left - 36, y: bottom + 12 },
      { x: left + (right - left) * 0.22, y: bottom + 12 },
      { x: left + (right - left) * 0.45, y: bottom + 12 },
      { x: left + (right - left) * 0.68, y: bottom + 12 },
      { x: left + (right - left) * 0.9, y: bottom + 12 },
      { x: right + 36, y: bottom + 12 },
    ];
    const flatK = ease.panel(seg(t, DT.flatten[0], DT.flatten[1]));
    return sculpt.map((p, i) => ({
      x: lerp(p.x, flat[i]!.x, flatK),
      y: lerp(p.y, flat[i]!.y, flatK),
    }));
  })();
  const bandReveal = t < DT.flatten[1] ? ease.camera(seg(t, DT.bandHead[0], DT.bandHead[1])) : 1;
  const bandOpacity = 1 - seg(t, 7750, 8100);
  const bandD = bandPts.length >= 2 ? smoothPath(bandPts) : "";
  // 按压时金带收紧：中部控制点短暂上提。
  const bandTightD = bandD;

  /* 纸带：输入框 → 画面中部 → 真实用户消息，两段三次贝塞尔。 */
  const strip = ((): { d: string; head: number; tail: number; alpha: number } | null => {
    if (!base || t < DT.stripHead[0] || t >= DT.stripFade[1]) return null;
    const c = base.composer;
    const m = base.message;
    const start = { x: c.x - c.w / 2 + 96, y: c.y + 8 };
    const end = { x: m.x - m.w / 2 + 66, y: m.y + m.h / 2 + 6 };
    const mid = { x: (start.x + end.x) / 2 + 250, y: (start.y + end.y) / 2 + 24 };
    const d =
      `M ${px(start.x)} ${px(start.y)} ` +
      `C ${px(start.x + 30)} ${px(start.y - 150)}, ${px(mid.x + 170)} ${px(mid.y + 150)}, ${px(mid.x)} ${px(mid.y)} ` +
      `C ${px(mid.x - 170)} ${px(mid.y - 160)}, ${px(end.x + 130)} ${px(end.y + 90)}, ${px(end.x)} ${px(end.y)}`;
    const head = ease.camera(seg(t, DT.stripHead[0], DT.stripHead[1]));
    const tail =
      seg(t, DT.stripTail[0], DT.stripTail[1]) * 0.24 +
      ease.panel(seg(t, DT.stripChase[0], DT.stripChase[1])) * 0.62;
    const alpha = 1 - ease.panel(seg(t, DT.stripFade[0], DT.stripFade[1]));
    return { d, head, tail, alpha };
  })();

  /* 指针：靠近发送按钮 → 按下 2px → 抬起淡出。 */
  const pointer = ((): { x: number; y: number; press: number; alpha: number } | null => {
    if (!base || t < DT.pointerIn[0] || t >= 6250) return null;
    const s = base.send;
    const k = ease.camera(seg(t, DT.pointerIn[0], DT.pointerIn[1]));
    const from = { x: s.x + 210, y: s.y + 150 };
    const x = lerp(from.x, s.x + 2, k);
    const y = lerp(from.y, s.y - 4, k);
    const press = t >= DT.press[0] && t < DT.press[1] ? 2 : 0;
    return { x, y, press, alpha: t > 6050 ? 1 - seg(t, 6050, 6250) : 1 };
  })();

  /* C 镜：真实计划行独立解折（perspective + rotateX + translateZ/X），9.1-10.6 归回真实整块。
     材质与阴影只随 --layer-progress 存在，归位后仅剩原生 plan。 */
  const layerProgress =
    clamp01(seg(t, 8100, 9000)) * (1 - seg(t, 9100, 10600));
  const ROW_Z = [0, 55, 110];
  const ROW_X = [0, 24, 48];
  const rowK = ROW_Z.map((_, i) => {
    const inK = ease.camera(seg(t, DT.layerIn + i * 120, DT.layerIn + 420 + i * 120));
    const outK = ease.panel(seg(t, DT.swingBack[0] + i * 60, DT.swingBack[1]));
    return inK * (1 - outK);
  });
  const planLayerStyle = ((): CSSProperties | undefined => {
    if (t < DT.planEnter[0]) return undefined;
    const style: Record<string, string> = { "--lp": layerProgress.toFixed(3) };
    rowK.forEach((k, i) => {
      const reveal = seg(t, DT.threads[i]!, DT.threads[i]! + 420);
      style[`--p${i + 1}-o`] = reveal.toFixed(3);
      style[`--p${i + 1}-rx`] = `${(-10 * (1 - k)).toFixed(2)}deg`;
      style[`--p${i + 1}-z`] = `${(ROW_Z[i]! * k).toFixed(1)}px`;
      style[`--p${i + 1}-x`] = `${(ROW_X[i]! * k).toFixed(1)}px`;
    });
    return style as CSSProperties;
  })();

  /* 金带一分为三的引线：message 底边短分叉 → x615 垂直轨道 → 水平接各行左缘状态点。
     全长 <330px 不遮正文，10.3-10.7s 收净。 */
  const threads =
    (base && t >= DT.threads[0] && t < 10700 ? base : null) &&
    ((): Array<{ d: string; reveal: number }> => {
      const out: Array<{ d: string; reveal: number }> = [];
      const m = base!.message;
      const sx = m.x;
      const sy = m.y + m.h / 2;
      const tx = 615;
      const retract = 1 - ease.panel(seg(t, 10300, 10700));
      for (let i = 0; i < 3; i += 1) {
        const row = base!.rows[i]!;
        const ex = row.x - row.w / 2 - 5;
        const ey = row.y;
        // 横摆段走 y~400：C 标题（285-336）之下、plan summary（413）之上的留白。
        const d =
          `M ${px(sx)} ${px(sy)} ` +
          `C ${px(sx)} ${px(sy + 70)}, 780 400, ${px(tx)} 402 ` +
          `L ${px(tx)} ${px(ey - 26)} ` +
          `Q ${px(tx)} ${px(ey)}, ${px(tx + 26)} ${px(ey)} ` +
          `L ${px(ex)} ${px(ey)}`;
        out.push({
          d,
          reveal:
            ease.camera(seg(t, DT.threads[i]!, DT.threads[i]! + 520)) * retract,
        });
      }
      return out;
    })();

  /* ---------------------------- 标题层 ---------------------------- */
  const titleParentOpacity = 1 - seg(t, DT.titleOut[0], DT.titleOut[1]);
  const titleLine = (index: number) => {
    const at = index === 0 ? DT.title1 : DT.title2;
    const k = ease.camera(seg(t, at[0], at[1]));
    const outK = ease.panel(seg(t, DT.titleOut[0], DT.titleOut[1]));
    return {
      transform: `translateY(${px(lerp(26, 0, k) - outK * 92)}px)`,
      opacity: clamp01(k * 1.6),
    };
  };

  const moonWall = 32 * Math.sin((yawDeg * Math.PI) / 180) * 1.6 + 1.6;
  const starWall = 22 * Math.sin((yawDeg * Math.PI) / 180) * 1.6 + 1.4;

  const uiStyle: CSSProperties = {
    clipPath: `inset(${((1 - winIn) * 100).toFixed(1)}% ${((1 - winIn) * 16).toFixed(1)}% 0 0)`,
    opacity: seg(t, 4000, 4500),
    transform: winIn < 1 ? `translateY(${((1 - winIn) * 36).toFixed(1)}px)` : undefined,
  };

  return (
    <div
      className="director-stage"
      ref={stageRef}
      style={{
        width: STAGE_W,
        height: STAGE_H,
        backgroundColor: lerpColor("#101719", "#E8E5DD", ease.panel(bgK)),
      }}
    >
      {bgK < 1 && <div className="director-bg-a" style={{ opacity: 1 - bgK }} />}
      <svg
        className="director-under"
        width={STAGE_W}
        height={STAGE_H}
        viewBox={`0 0 ${STAGE_W} ${STAGE_H}`}
      >
        <defs>
          <radialGradient id="dg-shadow" cx="0.5" cy="0.5" r="0.5">
            <stop offset="0" stopColor="#0A120E" stopOpacity="0.16" />
            <stop offset="0.7" stopColor="#0A120E" stopOpacity="0.08" />
            <stop offset="1" stopColor="#0A120E" stopOpacity="0" />
          </radialGradient>
          <filter id="df-soft" x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur stdDeviation="7" />
          </filter>
        </defs>
        {/* 装置接触影：紧贴物体、偏黑绿、宽约 450px，不是长斜线。 */}
        {solidsOpacity > 0.01 && (
          <g
            opacity={solidsOpacity * (0.85 + 0.15 * (1 - bandReveal * 0))}
            transform={`translate(${px(solidsShiftX)} ${px(solidsShiftY)}) scale(${solidsScale.toFixed(3)})`}
            style={{ transformOrigin: "1290px 455px" }}
          >
            <ellipse cx={MOON_C.x} cy={MOON_C.y + 292} rx={228} ry={24} fill="url(#dg-shadow)" />
            <ellipse cx={STAR_POS.x + 24} cy={STAR_POS.y + 96} rx={62} ry={11} fill="url(#dg-shadow)" />
          </g>
        )}
        {/* 金带地面影：跟随带形，落定后收细。 */}
        {bandD && bandOpacity > 0.01 && t < 4700 && (
          <path
            d={bandD}
            transform="translate(0 22)"
            fill="none"
            stroke="#0A120E"
            strokeWidth={lerp(34, 22, ease.panel(seg(t, DT.flatten[0], DT.flatten[1])))}
            strokeLinecap="round"
            opacity={0.13 * bandOpacity * bandReveal}
            filter="url(#df-soft)"
          />
        )}
      </svg>

      {windowVisible && (
        <div className="director-ui" style={uiStyle}>
          <ProductWindow
            t={t}
            box={UI_BOX}
            viewport={{ width: VIEW_W, height: VIEW_H }}
            opacity={1}
            brandFade={0}
            blur={0}
            film={directorSnapshot(t)}
            presses={{ send: sendPressed }}
            bootstrapOverride={directorBootstrap(t)}
            planLayerStyle={planLayerStyle}
          />
        </div>
      )}

      <svg
        className="director-over"
        width={STAGE_W}
        height={STAGE_H}
        viewBox={`0 0 ${STAGE_W} ${STAGE_H}`}
      >
        <defs>
          <linearGradient id="dg-band" x1="0" y1="0" x2="1" y2="0">
            <stop offset="0" stopColor="#B08C3E" />
            <stop offset="0.55" stopColor="#E8CE8C" />
            <stop offset="1" stopColor="#C6A567" />
          </linearGradient>
          <linearGradient id="dg-moon" x1="0.1" y1="0" x2="0.9" y2="1">
            <stop offset="0" stopColor="#F9F3E4" />
            <stop offset="0.55" stopColor="#F1EADB" />
            <stop offset="1" stopColor="#DCD1B4" />
          </linearGradient>
          <radialGradient id="dg-moon-spec" cx="0.32" cy="0.28" r="0.55">
            <stop offset="0" stopColor="#FFFFFF" stopOpacity="0.5" />
            <stop offset="1" stopColor="#FFFFFF" stopOpacity="0" />
          </radialGradient>
          <linearGradient id="dg-star" x1="0.1" y1="0" x2="0.9" y2="0.9">
            <stop offset="0" stopColor="#EDD48F" />
            <stop offset="0.5" stopColor="#D9B96B" />
            <stop offset="1" stopColor="#B78F3F" />
          </linearGradient>
          <radialGradient id="dg-star-spec" cx="0.34" cy="0.3" r="0.6">
            <stop offset="0" stopColor="#FFF0CC" stopOpacity="0.6" />
            <stop offset="1" stopColor="#FFF0CC" stopOpacity="0" />
          </radialGradient>
          <path id="strip-path" d={strip?.d ?? ""} fill="none" />
          <mask id="strip-mask">
            {strip && (
              <path
                d={strip.d}
                pathLength={1}
                fill="none"
                stroke="#fff"
                strokeWidth={96}
                strokeLinecap="butt"
                strokeDasharray={`${(strip.head - strip.tail).toFixed(4)} ${(1 - strip.head + strip.tail).toFixed(4)}`}
                strokeDashoffset={(-strip.tail).toFixed(4)}
              />
            )}
          </mask>
        </defs>

        {/* 金带：2px 暗厚底 + 12px 连续渐变主面 + 3px 带状高光；7.8-8.1s 完全退去。 */}
        {bandD && bandOpacity > 0.01 && (
          <g opacity={bandOpacity}>
            <path
              d={bandTightD}
              transform={`translate(0 ${px(7 + pressPulse * 2)})`}
              fill="none"
              stroke="#7A5E26"
              strokeWidth={2}
              strokeLinecap="round"
              pathLength={1}
              strokeDasharray={`${bandReveal.toFixed(4)} ${(1 - bandReveal).toFixed(4)}`}
            />
            <path
              d={bandTightD}
              fill="none"
              stroke="url(#dg-band)"
              strokeWidth={12 - pressPulse * 1}
              strokeLinecap="round"
              pathLength={1}
              strokeDasharray={`${bandReveal.toFixed(4)} ${(1 - bandReveal).toFixed(4)}`}
            />
            <path
              d={bandTightD}
              transform={`translate(0 ${px(-5 - pressPulse * 1)})`}
              fill="none"
              stroke="#FFF0CC"
              strokeWidth={3}
              strokeLinecap="round"
              opacity={0.5}
              pathLength={1}
              strokeDasharray={`${bandReveal.toFixed(4)} ${(1 - bandReveal).toFixed(4)}`}
            />
          </g>
        )}

        {/* 品牌装置：月牙（外半径 250、厚 32）+ 四角星（开口处，~150px）。 */}
        {solidsOpacity > 0.01 && (
          <g
            opacity={solidsOpacity}
            transform={`translate(${px(solidsShiftX)} ${px(solidsShiftY)}) scale(${solidsScale.toFixed(3)})`}
            style={{ transformOrigin: "1290px 455px" }}
          >
            <g
              transform={`translate(${px(MOON_C.x)} ${px(MOON_C.y)}) rotate(-35) scale(${MOON_K}) translate(-11 -11)`}
            >
              <ExtrudedPath
                d={MOON_D}
                wall={moonWall / MOON_K}
                layers={12}
                wallBack="#6E5F45"
                wallFront="#A3927A"
                faceFill="url(#dg-moon)"
                rim="rgba(255,240,204,0.4)"
              />
              <path d={MOON_D} fill="url(#dg-moon-spec)" />
            </g>
            <g
              transform={`translate(${px(STAR_POS.x + starOffset.x)} ${px(STAR_POS.y + starOffset.y)}) rotate(-35) scale(${STAR_K}) translate(-17.2 -11)`}
            >
              <ExtrudedPath
                d={STAR_D}
                wall={starWall / STAR_K}
                layers={10}
                wallBack="#6B5423"
                wallFront="#A98A3C"
                faceFill="url(#dg-star)"
                rim="rgba(255,240,204,0.5)"
              />
              <path d={STAR_D} fill="url(#dg-star-spec)" />
            </g>
          </g>
        )}

        {/* 句子纸带：暖白实体 + 2px 暗背面 + 轻投影 + 30px+ 文字沿弧线。 */}
        {strip && strip.alpha > 0.01 && (
          <g opacity={strip.alpha}>
            <g mask="url(#strip-mask)">
              <path
                d={strip.d}
                transform="translate(0 26)"
                fill="none"
                stroke="#0A120E"
                strokeWidth={62}
                strokeLinecap="butt"
                opacity={0.12}
                filter="url(#df-soft)"
              />
              <path
                d={strip.d}
                transform="translate(0 4)"
                fill="none"
                stroke="#C9BCA0"
                strokeWidth={62}
                strokeLinecap="butt"
              />
              <path
                d={strip.d}
                fill="none"
                stroke="#F6F1E4"
                strokeWidth={58}
                strokeLinecap="butt"
              />
              <text
                className="director-strip-text"
                fontSize={33}
                fill="#262B36"
                fontWeight={600}
                dominantBaseline="middle"
              >
                <textPath href="#strip-path" startOffset="16%">
                  {SENTENCE}
                </textPath>
              </text>
            </g>
          </g>
        )}

        {/* C 镜金线一分为三：连到真实计划行左缘。 */}
        {threads &&
          threads.map((th, i) =>
            th.reveal > 0.01 ? (
              <path
                key={i}
                d={th.d}
                fill="none"
                stroke="#C6A567"
                strokeWidth={2.6}
                strokeLinecap="round"
                opacity={0.92}
                pathLength={1}
                strokeDasharray={`${th.reveal.toFixed(4)} ${(1 - th.reveal).toFixed(4)}`}
              />
            ) : null,
          )}

        {/* 发送指针：44×54 深轮廓，靠近 → 按下 2px。 */}
        {pointer && pointer.alpha > 0.01 && (
          <g
            opacity={pointer.alpha}
            transform={`translate(${px(pointer.x)} ${px(pointer.y + pointer.press)}) scale(1.32)`}
          >
            <path
              d="M0 0 L0 40 L9.6 31.5 L15.5 46 L21.5 43.5 L15.8 29.5 L27 28.4 Z"
              fill="#232833"
              stroke="#F6F1E4"
              strokeWidth={1.6}
              strokeLinejoin="round"
            />
          </g>
        )}
      </svg>

      {/* 左侧宣言：整行遮罩显露；「指挥」香槟金。 */}
      {titleParentOpacity > 0.01 && t < 4300 && (
        <div
          className="director-title"
          style={{ opacity: titleParentOpacity }}
        >
          <div className="dt-line">
            <span style={titleLine(0)}>让已有工具</span>
          </div>
          <div className="dt-line">
            <span style={titleLine(1)}>
              听你的<span className="dt-gold">指挥</span>
            </span>
          </div>
        </div>
      )}
      {t < 4300 && (
        <div
          className="director-brand-label"
          style={{ opacity: enter * (1 - seg(t, 3400, 3900)) }}
        >
          Sleepy Doll
        </div>
      )}
      {t >= DT.cTitle[0] && (
        <div
          className="director-c-title"
          style={{ opacity: ease.camera(seg(t, DT.cTitle[0], DT.cTitle[1])) }}
        >
          先确认，再执行。
        </div>
      )}

      {/* 探测层：最终布局（消息+计划终态），fonts 就绪后测一次基准。 */}
      {!base && (
        <div
          className="director-probe"
          aria-hidden
          style={{
            position: "absolute",
            inset: 0,
            zIndex: 0,
            opacity: 0,
            pointerEvents: "none",
          }}
        >
          <ProductWindow
            t={PROBE_T}
            box={UI_BOX}
            viewport={{ width: VIEW_W, height: VIEW_H }}
            opacity={1}
            brandFade={0}
            blur={0}
            film={directorSnapshot(PROBE_T)}
            presses={{}}
            bootstrapOverride={directorBootstrap(PROBE_T)}
            planLayerStyle={{}}
          />
        </div>
      )}
    </div>
  );
}
