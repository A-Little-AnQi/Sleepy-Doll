import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { ProductWindow } from "./components/ProductWindow";
import { BetterGiWindow } from "./components/BetterGiWindow";
import { FeatureWindow } from "./components/FeatureWindow";
import { FilmOverlays } from "./film-overlays";
import { FilmShortcutPreview } from "./components/FilmShortcutPreview";
import { ProductRelay } from "./product-relay";
import { filmAt } from "./film";
import { bgiStateAt } from "./scenes";
import { useClock, seg, lerp, ease } from "./clock";
import recording from "./recording.json";
import { durationMs } from "./timeline.mjs";
import { cueTime, syncedStoryAt } from "./sync";
import "./stage.css";
import "./product-motion.css";

export const RECORDING = {
  width: recording.width,
  height: recording.height,
  fps: recording.fps,
  durationMs: durationMs(recording),
};
type Bounds = { x: number; y: number; w: number; h: number; naturalWidth?: number };
type Camera = { x: number; y: number; scale: number };
type Measures = Record<string, Bounds>;

// 时间表只改变演示节奏，产品状态继续使用原状态机。
function sourceTime(t: number) {
  const keys = recording.sourceTime;
  for (let i = 1; i < keys.length; i++) {
    const a = keys[i - 1]!,
      b = keys[i]!;
    if (t <= b[0]!) return lerp(a[1]!, b[1]!, seg(t, a[0]!, b[0]!));
  }
  return keys[keys.length - 1]![1]!;
}

// 用目标实际尺寸限制缩放，防止 UI 调整后控件被裁切。
function frame(
  target: Bounds | undefined,
  fallback: Camera,
  maxScale = 2.05,
): Camera {
  if (!target) return fallback;
  return {
    x: target.x,
    y: target.y,
    scale: Math.min(
      maxScale,
      (recording.width * 0.76) / Math.max(target.w, 1),
      (recording.height * 0.72) / Math.max(target.h, 1),
    ),
  };
}

function blend(a: Camera, b: Camera, k: number): Camera {
  return {
    x: lerp(a.x, b.x, k),
    y: lerp(a.y, b.y, k),
    scale: lerp(a.scale, b.scale, k),
  };
}

// 与样片相同的产品组件、消息移动和镜头跟随，覆盖完整任务流程。
export function ProductFilm() {
  const playback = useClock();
  const t = syncedStoryAt(playback);
  const feature = recording.features.some(
    (item) => playback >= item.start && playback < item.end,
  );
  const world = useRef<HTMLDivElement>(null);
  const [measurement, setMeasurement] = useState<{
    time: number;
    bounds: Measures;
    planHtml?: string;
    taskHtml?: string;
    previewHtml?: string;
  }>({ time: -1, bounds: {} });
  const passes = useRef({ time: -1, count: 0 });
  const bounds = measurement.time === t ? measurement.bounds : {};
  const source = sourceTime(t);
  const film = filmAt(source);
  film.welcome = t < 3800;
  film.welcomeFadeOut = seg(t, 3460, 3800);
  film.conversationFadeIn = 1;
  film.planEnter = t >= 27800 ? seg(t, 27800, 28200) : seg(t, 5000, 5580);
  film.planShiftPx = 0;
  if (t >= 22300 && t < 33700) film.page = "tasks";
  film.savedCardVisible = 0;
  if (t >= 4650 && t < 5000) film.phase = "正在整理步骤";

  const { width, height, ui } = recording;
  const hero: Camera = { x: width / 2, y: height / 2, scale: 0.94 };
  const input = bounds.input ?? {
    x: width * 0.58,
    y: height * 0.8,
    w: ui.width * 0.48,
    h: 46,
  };
  const plan = bounds.plan ??
    bounds.futurePlan ?? { x: width * 0.4, y: height * 0.3, w: 100, h: 30 };
  const flowWidth = bounds.flow?.w ?? bounds.futureFlow?.w ?? ui.width * 0.486;
  const inputCamera = {
    x: input.x,
    y: input.y - input.h * 2.17,
    scale: Math.min(1.9, (width * 0.7) / input.w),
  };
  const planCamera = {
    x: plan.x + flowWidth * 0.4214,
    y: plan.y + plan.h * 0.667,
    scale: Math.min(2.05, (width * 0.76) / flowWidth),
  };
  const push = ease.camera(seg(playback, cueTime("focus-input"), cueTime("focus-input") + 580));
  const follow = ease.transition(seg(playback, cueTime("send"), cueTime("send") + 620));
  let camera = blend(blend(hero, inputCamera, push), planCamera, follow);
  const split =
    ease.transition(seg(t, 12600, 13800)) *
    (1 - ease.transition(seg(t, 19800, 21000)));
  const box = {
    x: lerp((width - ui.width) / 2, width * 0.0286, split),
    y:
      lerp((height - ui.height) / 2, height * 0.24, split) +
      (1 - ease.camera(seg(t, 0, 700))) * 65,
    w: lerp(ui.width, width * 0.4193, split),
    h: ui.height,
  };
  if (t >= 9000) {
    const approval = frame(bounds.approval, planCamera);
    const logs = frame(bounds.bgiLog, hero, 2);
    logs.y -= (bounds.bgiLog?.h ?? 100) * 0.3;
    const result = { ...planCamera, y: plan.y - plan.h * 1.5 };
    const task = frame(bounds.task, hero, 2.05);
    const keys: Array<[number, Camera]> = [
      [9000, planCamera],
      [9700, planCamera],
      [10700, approval],
      [12100, approval],
      [13700, hero],
      [14500, hero],
      [15699, hero],
      [15700, logs],
      [19600, logs],
      [21000, hero],
      [21800, result],
      [22299, result],
      [22300, hero],
      [24900, hero],
      [25000, hero],
      [25100, hero],
      [26000, task],
      [27200, task],
      [27799, hero],
      [27800, hero],
      [28400, task],
      [29300, task],
      [29699, task],
      [29700, task],
      [30400, task],
      [31600, task],
      [33400, hero],
      [36000, hero],
    ];
    camera = keys[keys.length - 1]![1];
    for (let i = 1; i < keys.length; i++) {
      const a = keys[i - 1]!,
        b = keys[i]!;
      if (t <= b[0]) {
        const progress = seg(t, a[0], Math.min(b[0], a[0] + 650));
        camera = blend(a[1], b[1], ease.camera(progress));
        break;
      }
    }
  }
  const tx = width / 2 - camera.x * camera.scale,
    ty = height / 2 - camera.y * camera.scale;
  const clickCues: Record<string, string> = { send: "send", allow: "allow", save: "save", run: "run-again" };
  const action = recording.clicks.find(([, key]) => {
    const at = cueTime(clickCues[String(key)]!);
    return playback >= at - 400 && playback < at + 250;
  });
  const target = bounds[String(action?.[1] ?? "send")] ?? input;
  const clickAt = cueTime(clickCues[String(action?.[1] ?? "send")]!);
  const travel = ease.camera(seg(playback, clickAt - 340, clickAt - 60));
  const clickPress =
    seg(playback, clickAt - 70, clickAt + 10) * (1 - seg(playback, clickAt + 60, clickAt + 180));
  const cursor = {
    x: lerp(target.x - 95, target.x, travel),
    y: lerp(target.y + 75, target.y, travel),
  };

  useLayoutEffect(() => {
    const layer = world.current;
    if (!layer) return;
    const live = layer.querySelector<HTMLElement>(".film-live")!;
    const message = live.querySelector<HTMLElement>(recording.anchors.message);
    if (message) {
      message.style.transform = "none";
      message.style.opacity = "1";
    }
    const r = layer.getBoundingClientRect(),
      scale = r.width / width;
    const next: Measures = {};
    const selectors: Record<string, string> = {
      ...recording.anchors,
      futurePlan: `.film-probe ${recording.anchors.plan}`,
      futureFlow: `.film-probe ${recording.anchors.flow}`,
      futureTask: `.film-task-probe ${recording.anchors.task}`,
      completedPlan: `.film-plan-probe .run-plan-cluster`,
      futurePreview: `.film-shortcut-probe .shortcut-preview`,
      step0: `${recording.anchors.steps}:nth-child(1)`,
      step1: `${recording.anchors.steps}:nth-child(2)`,
      step2: `${recording.anchors.steps}:nth-child(3)`,
    };
    for (const [key, selector] of Object.entries(selectors)) {
      const element = key.startsWith("future") || key === "completedPlan"
        ? layer.querySelector(selector)
        : live.querySelector(selector);
      if (!element) continue;
      const b = element.getBoundingClientRect();
      next[key] = {
        x: Math.round((b.left + b.width / 2 - r.left) / scale),
        y: Math.round((b.top + b.height / 2 - r.top) / scale),
        w: Math.round(b.width / scale),
        h: Math.round(b.height / scale),
        naturalWidth: (element as HTMLElement).offsetWidth,
      };
    }
    if (passes.current.time !== t) passes.current = { time: t, count: 0 };
    if (
      passes.current.count < 4 &&
      (measurement.time !== t ||
        JSON.stringify(measurement.bounds) !== JSON.stringify(next))
    ) {
      passes.current.count++;
      setMeasurement({
        time: t,
        bounds: next,
        planHtml: layer.querySelector(".film-plan-probe .run-plan-cluster")?.outerHTML,
        taskHtml: layer.querySelector(".film-task-probe .task-card")?.outerHTML,
        previewHtml: layer.querySelector(".film-shortcut-probe .shortcut-preview")?.outerHTML,
      });
    }
    if (message && next.message && next.input && t >= 3800 && t < 4980) {
      const k = ease.transition(seg(playback, cueTime("send"), cueTime("send") + 620));
      const originX = next.input.x - next.input.w / 2 + next.message.w / 2;
      message.style.transform = `translate(${(originX - next.message.x) * (1 - k) - Math.sin(k * Math.PI) * 65}px,${(next.input.y - next.message.y) * (1 - k)}px)`;
      message.style.opacity = String(seg(t, 3800, 3890));
    }
    live
      .querySelectorAll<HTMLElement>(recording.anchors.steps)
      .forEach((step, i) => {
        const at = cueTime(
          ["plan-check", "plan-route", "plan-verify"][i] ?? "plan-verify",
        );
        const k =
          t >= 27800
            ? ease.camera(seg(t, 27800 + i * 90, 28100 + i * 90))
            : ease.camera(seg(playback, at, at + 280));
        step.style.opacity = String(k);
        step.style.transform = `translate(${(1 - k) * 48}px,${(1 - k) * 12}px)`;
        step.style.clipPath = `inset(0 ${(1 - k) * 100}% 0 0)`;
      });
    const taskCard = live.querySelector<HTMLElement>(recording.anchors.task);
    if (taskCard) taskCard.style.opacity = String(t < 25100 ? 0 : seg(t, 25100, 25400));
  });

  return (
    <div
      className="video-stage product-film"
      data-story-time={t}
      style={
        {
          width,
          height,
          "--video-spin": `${t / 1300}turn`,
          "--task-enter":
            t >= 29700 ? seg(t, 29700, 30100) : seg(t, 25100, 25600),
        } as CSSProperties
      }
    >
      <div
        className="product-world"
        ref={world}
        style={{
          width,
          height,
          opacity: feature ? 0 : 1,
          transform: `translate3d(${tx.toFixed(2)}px,${ty.toFixed(2)}px,0) scale(${camera.scale.toFixed(4)})`,
        }}
      >
        <div className="film-live">
          {split > 0 && (
            <div style={{ opacity: split }}>
              <BetterGiWindow
                box={{
                  x: width * 0.4531,
                  y: height * 0.24,
                  w: width * 0.5156,
                  h: height * 0.46,
                }}
                state={bgiStateAt(source)}
              />
            </div>
          )}
          <ProductWindow
            t={source}
            box={box}
            viewport={ui}
            opacity={seg(t, 0, 260)}
            brandFade={seg(t, 33500, 34200)}
            blur={0}
            film={film}
            presses={{
              send: action?.[1] === "send" && clickPress > 0.3,
              allow: action?.[1] === "allow" && clickPress > 0.3,
            }}
          />
          {t >= 22300 && t < 24300 && <FilmShortcutPreview time={t} />}
        </div>
        {t < 5300 && (
          <div className="film-probe" aria-hidden>
            <ProductWindow
              t={13700}
              box={{
                x: (width - ui.width) / 2,
                y: (height - ui.height) / 2,
                w: ui.width,
                h: ui.height,
              }}
              viewport={ui}
              opacity={1}
              brandFade={0}
              blur={0}
              film={filmAt(13700)}
              presses={{}}
            />
          </div>
        )}
        {t >= 20000 && t < 26700 && (
          <>
            <div className="film-plan-probe film-probe" aria-hidden>
              <ProductWindow t={41600} box={{ x: (width - ui.width) / 2, y: (height - ui.height) / 2, w: ui.width, h: ui.height }}
                viewport={ui} opacity={1} brandFade={0} blur={0} film={filmAt(41600)} presses={{}} />
            </div>
            <div className="film-task-probe film-probe" aria-hidden>
              <ProductWindow t={46200} box={{ x: (width - ui.width) / 2, y: (height - ui.height) / 2, w: ui.width, h: ui.height }}
                viewport={ui} opacity={1} brandFade={0} blur={0} film={filmAt(46200)} presses={{}} />
            </div>
            <div className="film-shortcut-probe film-probe" aria-hidden><FilmShortcutPreview time={23000} /></div>
          </>
        )}
      </div>
      <ProductRelay
        time={playback}
        story={t}
        bounds={bounds}
        camera={{ scale: camera.scale, tx, ty }}
        planHtml={measurement.planHtml}
        taskHtml={measurement.taskHtml}
        previewHtml={measurement.previewHtml}
      />
      {!!action && (
        <div
          className="video-cursor"
          style={{
            left: cursor.x * camera.scale + tx,
            top: cursor.y * camera.scale + ty,
            opacity:
              seg(playback, clickAt - 400, clickAt - 320) * (1 - seg(playback, clickAt + 60, clickAt + 250)),
            transform: `scale(${1 - clickPress * 0.18})`,
          }}
        >
          <svg viewBox="0 0 24 24">
            <path d="M4 2 L4 19 L8.6 14.8 L11.6 21.4 L14.6 20 L11.6 13.6 L18 13.2 Z" />
          </svg>
        </div>
      )}
      {t >= 14200 && t < 19600 && (
        <div className="film-disclosure">执行过程已加速</div>
      )}
      {feature && <FeatureWindow time={playback} />}
      <FilmOverlays time={playback} />
    </div>
  );
}
