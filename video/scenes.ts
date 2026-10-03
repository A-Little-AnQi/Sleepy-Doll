import bgiFixture from "./fixtures/bettergi.json";
import { T } from "./manifest";
import { clamp01, ease, lerp, seg } from "./clock";
import type { FilmSnapshot } from "./film";
import { filmAt } from "./film";

/**
 * 舞台呈现层：每个 t 的窗口盒子、镜头、字幕、光标与 BetterGI 状态。
 * 镜头与光标都以「锚点」表达，锚点坐标由 Stage 实测（层坐标，不受镜头影响）。
 */

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface CameraPoint {
  x: number;
  y: number;
  scale: number;
}

export interface CameraKey {
  at: number;
  to: CameraPoint;
  anchor?: string;
  dx?: number;
  dy?: number;
  ease?: (x: number) => number;
}

export type CameraTrack = CameraKey[];

/** 光标：要么钉在锚点上，要么在两锚点间移动。坐标一律「锚点 + 偏移」。 */
export interface CursorState {
  anchor?: string;
  fromAnchor?: string;
  toAnchor?: string;
  k?: number;
  dx?: number;
  dy?: number;
  press: number;
  visible: boolean;
}

export interface BgiState {
  running: boolean;
  rowSelected: boolean;
  row1Status: "待运行" | "运行中" | "已完成";
  logCount: number;
  statusText: string;
  brightness: number;
}

export interface SceneLayout {
  film: FilmSnapshot;
  sdBox: Box;
  sdOpacity: number;
  /** 品牌收束：窗口沉入夜色的程度 0..1（透明度 + 虚化）。 */
  brandFade: number;
  sdBlur: number;
  bgi?: { box: Box; opacity: number; state: BgiState };
  camera: CameraTrack;
  cursor?: CursorState;
  fadeToBlack: number;
  /** 环境光强度：暖金（产品窗）/ 冷蓝（BetterGI 窗），位置由舞台按窗口投影。 */
  warmGlow: number;
  coolGlow: number;
}

const STAGE_W = 2560;
const STAGE_H = 1440;

/** 产品窗口统一按 1440×900 设计稿等比缩放：72% 宽的主舞台，右侧留字幕负空间。 */
const SD_HERO: Box = { x: 120, y: 144, w: 1843, h: 1152 };
/** S05 分屏：46 / 54，同高同上边线（1095/1285 ≈ 46%/50%，余量给间隙）。 */
const SD_SPLIT: Box = { x: 70, y: 340, w: 1095, h: 684 };
const BGI_SPLIT: Box = { x: 1205, y: 296, w: 1285, h: 772 };
/** S01 的 BetterGI 大窗口。 */
const BGI_HERO: Box = { x: 230, y: 130, w: 2100, h: 1180 };

function boxLerp(from: Box, to: Box, k: number): Box {
  return {
    x: lerp(from.x, to.x, k),
    y: lerp(from.y, to.y, k),
    w: lerp(from.w, to.w, k),
    h: lerp(from.h, to.h, k),
  };
}

const CENTER: CameraPoint = { x: STAGE_W / 2, y: STAGE_H / 2, scale: 1 };

/** BetterGI 状态：S01 三次循环 + S05 连续推进，日志行数由时间驱动。 */
function bgiStateAt(t: number): BgiState {
  const brightness = (() => {
    if (t < T.s01DimStart) return 1;
    if (t < T.s01ZoomStart)
      return lerp(1, 0.84, ease.ui(seg(t, T.s01DimStart, T.s01ZoomStart)));
    if (t < T.s01Cut) return 0.84;
    if (t < T.s05Settled) return lerp(0.9, 1, seg(t, 24000, T.s05Settled));
    if (t < T.s05Merge) return lerp(1, 0.88, seg(t, T.s05BgiDim, T.s05Merge));
    return 0.88;
  })();

  if (t < T.s01Cut) {
    const running =
      (t >= T.s01Loop1Click + 50 && t < T.s01Loop1End) ||
      (t >= T.s01Loop2Click + 50 && t < T.s01Loop2End);
    return {
      running,
      rowSelected: t >= T.s01Loop1Start,
      row1Status: running ? "运行中" : "待运行",
      logCount: running ? 2 : 1,
      statusText: running ? "运行中：每日任务" : "就绪",
      brightness,
    };
  }

  if (t < 24000) {
    return {
      running: false,
      rowSelected: true,
      row1Status: "待运行",
      logCount: 1,
      statusText: "就绪",
      brightness: 0.78,
    };
  }

  const logTimes = [
    24000, 25000, 27000, 28200, 29000, 30100, 31100, 32200, 34500,
  ];
  let logCount = 1;
  logTimes.forEach((at, index) => {
    if (t >= at) logCount = index + 2;
  });
  const done = t >= T.s05BgiDim;
  const commission = Math.min(
    4,
    Math.max(1, Math.floor((t - 27500) / 1800) + 1),
  );
  return {
    running: !done,
    rowSelected: true,
    row1Status: done ? "已完成" : t >= 27000 ? "运行中" : "待运行",
    logCount: Math.min(logCount, bgiFixture.scheduler.runningLog.length + 1),
    statusText: done
      ? bgiFixture.scheduler.statusDone
      : `运行中：每日任务 · 委托 ${commission}/4`,
    brightness,
  };
}

/** S01 一次循环：选中路线 → 移到启动 → 点击；第三次停在点击前。 */
function s01Cursor(t: number): CursorState | undefined {
  const loop = (t0: number, speed: number, halt: boolean): CursorState => {
    const select = t0 + 300 / speed;
    const moveStart = t0 + 420 / speed;
    const moveEnd = t0 + 840 / speed;
    const click = t0 + 870 / speed;
    if (t < select) {
      return { anchor: "bgi-route1", dx: -46, dy: -8, press: 0, visible: true };
    }
    if (t < select + 45) {
      return {
        anchor: "bgi-route1",
        press: clamp01(seg(t, select, select + 45)),
        visible: true,
      };
    }
    if (halt && t >= moveEnd) {
      return { anchor: "bgi-start", dx: -34, dy: 8, press: 0, visible: true };
    }
    if (t >= click && t < click + 90) {
      return {
        anchor: "bgi-start",
        press: clamp01(seg(t, click, click + 45)),
        visible: true,
      };
    }
    if (t >= click + 90) {
      return { anchor: "bgi-start", press: 0, visible: true };
    }
    return {
      fromAnchor: "bgi-route1",
      toAnchor: "bgi-start",
      k: ease.camera(seg(t, moveStart, moveEnd)),
      press: 0,
      visible: true,
    };
  };
  let held: CursorState | undefined;
  if (t < 1800) held = loop(0, 1, false);
  else if (t < 3000) held = loop(1800, 1.08, false);
  else if (t < T.s01DimStart + 400) held = loop(3000, 1, true);
  else return undefined;
  if (t >= T.s01DimStart) {
    return { ...held, visible: false };
  }
  return held;
}

function s02Cursor(t: number): CursorState | undefined {
  if (t < 6600) return undefined;
  if (t < 6900) {
    return {
      anchor: "sd-composer",
      dx: -190,
      dy: -34,
      press: 0,
      visible: true,
    };
  }
  if (t < 6945) {
    return {
      anchor: "sd-composer",
      press: clamp01(seg(t, 6900, 6945)),
      visible: true,
    };
  }
  if (t < 9700) {
    return {
      anchor: "sd-composer",
      dx: -190,
      dy: -34,
      press: 0,
      visible: true,
    };
  }
  if (t < T.s02SendPress + 90) {
    return {
      fromAnchor: "sd-composer",
      toAnchor: "sd-send",
      k: ease.camera(seg(t, 9700, T.s02SendPress)),
      press:
        t >= T.s02SendPress
          ? clamp01(seg(t, T.s02SendPress, T.s02SendPress + 45))
          : 0,
      visible: true,
    };
  }
  const fade = seg(t, T.s02SendPress + 300, T.s02SendPress + 800);
  return fade >= 1
    ? undefined
    : { anchor: "sd-send", press: 0, visible: fade < 1 };
}

function s04Cursor(t: number): CursorState | undefined {
  if (t < T.s04CursorMove) return undefined;
  if (t < T.s04CursorMove + 60) {
    return { anchor: "sd-allow", dx: -320, dy: 220, press: 0, visible: true };
  }
  if (t < T.s04Click + 90) {
    return {
      fromAnchor: "sd-allow",
      toAnchor: "sd-allow",
      k: ease.camera(seg(t, T.s04CursorMove + 60, T.s04Click)),
      press: t >= T.s04Click ? clamp01(seg(t, T.s04Click, T.s04Click + 45)) : 0,
      visible: true,
      dx: lerp(-320, 0, ease.camera(seg(t, T.s04CursorMove + 60, T.s04Click))),
      dy: lerp(220, 0, ease.camera(seg(t, T.s04CursorMove + 60, T.s04Click))),
    };
  }
  const fade = seg(t, T.s04Click + 700, T.s04Click + 1200);
  return fade >= 1
    ? undefined
    : { anchor: "sd-allow", press: 0, visible: true };
}

function s06s07Cursor(t: number): CursorState | undefined {
  if (t < T.s06Pan) return undefined;
  if (t < T.s07SaveClick + 90) {
    return {
      fromAnchor: "sd-save",
      toAnchor: "sd-save",
      k: 1,
      dx: lerp(-120, 0, ease.camera(seg(t, T.s06Pan + 300, T.s07SaveClick))),
      dy: lerp(-140, 0, ease.camera(seg(t, T.s06Pan + 300, T.s07SaveClick))),
      press:
        t >= T.s07SaveClick
          ? clamp01(seg(t, T.s07SaveClick, T.s07SaveClick + 45))
          : 0,
      visible: true,
    };
  }
  if (t < T.s07CursorMove) return undefined;
  if (t < T.s07RunClick + 90) {
    return {
      fromAnchor: "sd-run",
      toAnchor: "sd-run",
      k: 1,
      dx: lerp(-300, 0, ease.camera(seg(t, T.s07CursorMove, T.s07RunClick))),
      dy: lerp(180, 0, ease.camera(seg(t, T.s07CursorMove, T.s07RunClick))),
      press:
        t >= T.s07RunClick
          ? clamp01(seg(t, T.s07RunClick, T.s07RunClick + 45))
          : 0,
      visible: true,
    };
  }
  const fade = seg(t, T.s07RunClick + 600, T.s07RunClick + 1100);
  return fade >= 1 ? undefined : { anchor: "sd-run", press: 0, visible: true };
}

export function sceneLayout(t: number): SceneLayout {
  const film = filmAt(t);

  // ---- 窗口盒子：主舞台与分屏之间的推拉（脚本 960ms transition / 760ms camera）----
  const splitIn = ease.camera(seg(t, T.s04Split, T.s04Split + 700));
  const splitOut = 1 - ease.camera(seg(t, T.s05Merge, T.s05Merge + 1000));
  const splitK = t < T.s05Merge ? splitIn : splitOut;
  const sdBox = boxLerp(SD_HERO, SD_SPLIT, splitK);

  let bgi: SceneLayout["bgi"];
  if (t < T.s01Cut) {
    // 匹配硬切：切点前 BGI 满幅，切点后整窗让位，不做软溶解。
    bgi = {
      box: BGI_HERO,
      opacity: 1,
      state: bgiStateAt(t),
    };
  } else if (t >= T.s04Split - 200) {
    const inK = ease.camera(seg(t, T.s04Split - 200, T.s04Split + 700));
    const outK = 1 - ease.camera(seg(t, T.s05Merge, T.s05Merge + 900));
    bgi = {
      box: BGI_SPLIT,
      opacity: Math.min(t < T.s05Merge ? inK : outK, 1),
      state: bgiStateAt(t),
    };
  }

  // ---- 产品窗口：S02 入场即形状匹配硬切的另一端 ----
  const sdOpacity = t < T.s01Cut ? 0 : 1;

  // ---- S08 品牌收束：窗口沉入夜色（透明 + 虚化），不再灰度压暗 ----
  const brandFade = ease.camera(seg(t, T.s08Dim, T.s08DimSettled));
  const sdBlur = 4 * brandFade;

  // 镜头在产品全景、操作锚点与宿主日志之间连续移动。
  const hero: CameraPoint = { x: 1042, y: 720, scale: 1.02 };
  const camera: CameraTrack = [
    { at: 0, to: CENTER },
    { at: 500, to: CENTER },
    { at: 1550, to: { x: 1500, y: 640, scale: 1.28 } },
    { at: 3600, to: { x: 1580, y: 600, scale: 1.4 } },
    { at: 5100, to: { x: 1710, y: 550, scale: 1.65 } },
    { at: 5399, to: { x: 1710, y: 550, scale: 1.65 } },
    { at: 5400, to: { ...hero, scale: 0.92 } },
    { at: 6350, to: hero },
    { at: 6750, to: hero },
    {
      at: 7950,
      to: { x: 1200, y: 1040, scale: 1.9 },
      anchor: "sd-composer",
      dy: -100,
    },
    {
      at: 10100,
      to: { x: 1200, y: 1040, scale: 1.9 },
      anchor: "sd-composer",
      dy: -100,
    },
    { at: 11400, to: { x: 1090, y: 520, scale: 1.65 } },
    {
      at: 13500,
      to: { x: 1100, y: 520, scale: 2.15 },
      anchor: "sd-plan",
      dx: 300,
      dy: 100,
    },
    {
      at: 16700,
      to: { x: 1100, y: 520, scale: 2.25 },
      anchor: "sd-plan",
      dx: 300,
      dy: 100,
    },
    { at: 18200, to: { x: 1100, y: 530, scale: 2.0 } },
    {
      at: 19900,
      to: { x: 1080, y: 630, scale: 2.3 },
      anchor: "sd-allow",
      dx: 300,
      dy: -40,
    },
    {
      at: 22600,
      to: { x: 1080, y: 630, scale: 2.3 },
      anchor: "sd-allow",
      dx: 300,
      dy: -40,
    },
    { at: 24100, to: { x: 1280, y: 710, scale: 1 } },
    { at: 25200, to: { x: 1280, y: 710, scale: 1 } },
    {
      at: 26600,
      to: { x: 1950, y: 640, scale: 1.65 },
      anchor: "bgi-route1",
      dy: 195,
    },
    {
      at: 28900,
      to: { x: 1950, y: 640, scale: 1.72 },
      anchor: "bgi-route1",
      dy: 195,
    },
    {
      at: 31000,
      to: { x: 2040, y: 810, scale: 1.78 },
      anchor: "bgi-log",
      dy: -150,
    },
    {
      at: 34100,
      to: { x: 2040, y: 810, scale: 1.78 },
      anchor: "bgi-log",
      dy: -150,
    },
    { at: 36000, to: { x: 1280, y: 710, scale: 1 } },
    { at: 37600, to: hero },
    {
      at: 39900,
      to: { x: 1100, y: 510, scale: 2.1 },
      anchor: "sd-plan",
      dx: 300,
      dy: -25,
    },
    {
      at: 42400,
      to: { x: 1100, y: 510, scale: 2.1 },
      anchor: "sd-plan",
      dx: 300,
      dy: -25,
    },
    {
      at: 43900,
      to: { x: 1110, y: 535, scale: 2.25 },
      anchor: "sd-save",
      dx: 290,
      dy: -90,
    },
    {
      at: 45100,
      to: { x: 1110, y: 535, scale: 2.25 },
      anchor: "sd-save",
      dx: 290,
      dy: -90,
    },
    { at: 46199, to: hero },
    { at: 46200, to: hero },
    {
      at: 47400,
      to: { x: 960, y: 480, scale: 1.95 },
      anchor: "sd-task",
      dx: 220,
      dy: 85,
    },
    {
      at: 49700,
      to: { x: 960, y: 480, scale: 2.05 },
      anchor: "sd-task",
      dx: 220,
      dy: 85,
    },
    { at: 50099, to: { ...hero, scale: 1.35 } },
    { at: 50100, to: { ...hero, scale: 1.35 } },
    {
      at: 51400,
      to: { x: 1100, y: 520, scale: 1.95 },
      anchor: "sd-plan",
      dx: 300,
      dy: 100,
    },
    { at: 52199, to: hero },
    { at: 52200, to: hero },
    {
      at: 52900,
      to: { x: 960, y: 480, scale: 1.6 },
      anchor: "sd-task",
      dx: 220,
      dy: 85,
    },
    { at: 54300, to: hero },
    { at: 60000, to: hero },
  ];

  // ---- 光标 ----
  let cursor: CursorState | undefined;
  if (t < T.s01Cut + 100) cursor = s01Cursor(t);
  else if (t < 12000) cursor = s02Cursor(t);
  else if (t < 18000) cursor = undefined;
  else if (t < 24000) cursor = s04Cursor(t);
  else if (t < T.s06Pan) cursor = undefined;
  else cursor = s06s07Cursor(t);

  const fadeToBlack =
    t >= T.s08Black
      ? 1
      : t >= T.s08FadeOut
        ? ease.transition(seg(t, T.s08FadeOut, T.s08Black))
        : 0;

  // ---- 环境光：跟着窗口出现/退场；匹配切前后短暂压低，避免闪光 ----
  const cutDip =
    t < T.s01Cut
      ? 1 - ease.ui(seg(t, T.s01ZoomStart, T.s01Cut))
      : ease.ui(seg(t, T.s01Cut, T.s01Cut + 500));
  const warm =
    sdOpacity * cutDip * (t >= T.s08Dim ? lerp(1, 0.55, brandFade) : 1);
  const cool =
    (bgi ? bgi.opacity : 0) *
    (t < T.s01Cut
      ? cutDip
      : ease.ui(seg(t, T.s04Split - 200, T.s04Split + 700)));

  return {
    film,
    sdBox,
    sdOpacity,
    brandFade,
    sdBlur,
    bgi,
    camera,
    cursor,
    fadeToBlack,
    warmGlow: warm,
    coolGlow: cool,
  };
}

export interface Anchors {
  [id: string]: { x: number; y: number } | undefined;
}

/** 镜头解析：锚点实测坐标由 Stage 传入。 */
export function resolveCamera(
  track: CameraTrack,
  t: number,
  anchors: Anchors,
): CameraPoint {
  const pointOf = (key: CameraKey): CameraPoint => {
    const anchor = key.anchor ? anchors[key.anchor] : undefined;
    return anchor
      ? {
          x: anchor.x + (key.dx ?? 0),
          y: anchor.y + (key.dy ?? 0),
          scale: key.to.scale,
        }
      : key.to;
  };
  if (t <= track[0]!.at) return pointOf(track[0]!);
  if (t >= track[track.length - 1]!.at)
    return pointOf(track[track.length - 1]!);
  for (let i = 1; i < track.length; i++) {
    const a = track[i - 1]!,
      b = track[i]!;
    if (t <= b.at) {
      const p = seg(t, a.at, b.at);
      const k = b.ease ? b.ease(p) : p * p * p * (10 - 15 * p + 6 * p * p);
      const from = pointOf(a),
        to = pointOf(b);
      return {
        x: lerp(from.x, to.x, k),
        y: lerp(from.y, to.y, k),
        scale: lerp(from.scale, to.scale, k),
      };
    }
  }
  return pointOf(track[track.length - 1]!);
}

export { bgiFixture, bgiStateAt };
