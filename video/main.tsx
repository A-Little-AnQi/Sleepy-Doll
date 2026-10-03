import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import "../web/src/product.css";
import "../web/src/motion.css";
import "./preview.css";
import { readClock, seekClock, useClock } from "./clock";
import { DirectorFilm, DIRECTOR_COVERAGE_MS } from "./director/DirectorFilm";
import { LateFilm, LATE_COVERAGE_MS } from "./director/LateFilm";
import { StyleLab, STYLE_SPEC } from "./design/StyleLab";
import { NativeScene, NATIVE_STATES, type NativeState } from "./design/native-scenes";

const params = new URLSearchParams(location.search);
const capture = params.has("capture");
// 离线 design-preview.html 无 query，靠内联注入的 flag 进入试验台。
const designLab = params.has("design-lab") || window.__DESIGN_LAB__ === true;
const nativeQuery = params.has("design-native");
const spec = designLab
  ? STYLE_SPEC
  : nativeQuery
    ? { width: 1440, height: 900, fps: 60, durationMs: 1000 }
    : ({ width: 1920, height: 1080, fps: 60, durationMs: 60000 } as const);
// 虚拟录制使用固定时钟，跳过浏览器按墙钟执行的页面快照过渡。
document.startViewTransition = (options) => {
  const callback = typeof options === "function" ? options : options?.update;
  const done = Promise.resolve(callback?.());
  return {
    finished: done,
    ready: done,
    updateCallbackDone: done,
    types: new Set<string>(),
    skipTransition() {},
  };
};
document.documentElement.dataset.theme = "light";
localStorage.setItem("sleepy-doll-locale", "zh");
localStorage.setItem("sleepy-doll-theme", "light");
localStorage.removeItem("sleepy-doll-sidebar-collapsed");
seekClock(Number(params.get("t") ?? 0));
const nativeState = params.get("state");

function Root() {
  const t = useClock();
  const [playing, setPlaying] = useState(
    !capture && params.get("play") === "1",
  );
  const [scale, setScale] = useState(
    Math.min(innerWidth / spec.width, innerHeight / spec.height),
  );
  useEffect(() => {
    const resize = () =>
      setScale(Math.min(innerWidth / spec.width, innerHeight / spec.height));
    addEventListener("resize", resize);
    return () => removeEventListener("resize", resize);
  }, []);
  useEffect(() => {
    if (!playing) {
      return;
    }
    const from = readClock() >= spec.durationMs - 20 ? 0 : readClock();
    const start = performance.now();
    let frame = 0;
    const tick = () => {
      const next = from + performance.now() - start;
      seekClock(Math.min(next, spec.durationMs - 1));
      if (next >= spec.durationMs) setPlaying(false);
      else frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing]);
  return (
    <div className={capture ? "preview-shell film-capture" : "preview-shell"}>
      <div
        className="preview-scaler"
        style={{ width: spec.width * scale, height: spec.height * scale }}
      >
        <div
          style={{
            position: "relative",
            width: spec.width,
            height: spec.height,
            transform: `scale(${scale})`,
            transformOrigin: "0 0",
          }}
        >
          {designLab ? (
            <StyleLab />
          ) : nativeQuery ? (
            <NativeScene
              state={
                NATIVE_STATES.includes(nativeState as NativeState)
                  ? (nativeState as NativeState)
                  : "compose"
              }
            />
          ) : t < DIRECTOR_COVERAGE_MS ? (
            <DirectorFilm />
          ) : (
            <LateFilm />
          )}
        </div>
      </div>
      {!capture && (
        <>
          <div className="preview-controls">
            <button onClick={() => setPlaying(!playing)}>
              {playing ? "暂停" : "播放"}
            </button>
            <input
              aria-label="时间线"
              type="range"
              min="0"
              max={spec.durationMs - 1}
              value={t}
              onChange={(e) => {
                setPlaying(false);
                seekClock(Number(e.target.value));
              }}
            />
            <span>
              {(t / 1000).toFixed(1)} / {spec.durationMs / 1000} 秒
            </span>
          </div>
        </>
      )}
    </div>
  );
}

declare global {
  interface Window {
    __DESIGN_LAB__?: boolean;
    __VIDEO__?: {
      seek(ms: number): void;
      ready(): Promise<void>;
      time(): number;
      spec(): {
        width: number;
        height: number;
        fps: number;
        durationMs: number;
      };
      coverage(): { directorMs: number; lateMs: number };
    };
  }
}
async function settleFrames() {
  await document.fonts.ready;
  await Promise.all(
    Array.from(document.images).map((img) => img.decode().catch(() => undefined)),
  );
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}
window.__VIDEO__ = {
  seek: (ms) =>
    flushSync(() => seekClock(Math.max(0, Math.min(ms, spec.durationMs - 1)))),
  time: readClock,
  spec: () => spec,
  coverage: () => ({ directorMs: DIRECTOR_COVERAGE_MS, lateMs: LATE_COVERAGE_MS }),
  async ready() {
    if (designLab) {
      // 风格试验台：只等 __STYLE_READY__（纹理载入 + 首帧渲染）+ 字体 + 双 rAF，
      // 不等导演段探针。
      const deadline = performance.now() + 15_000;
      while (!window.__STYLE_READY__) {
        if (performance.now() > deadline)
          throw new Error("StyleLab 15 秒内未就绪（__STYLE_READY__ 未置位）");
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      }
      await settleFrames();
      return;
    }
    if (nativeQuery) {
      if (!document.querySelector("[data-native-root]"))
        throw new Error(`原生状态页未挂载（state=${nativeState}）`);
      await settleFrames();
      return;
    }
    // 导演段（0-12 秒）依赖真实组件的基准布局：等 .director-probe 测完卸载才算就绪，
    // 有截止时间，不无限等待。
    if (readClock() < DIRECTOR_COVERAGE_MS) {
      if (!document.querySelector(".director-stage"))
        throw new Error(
          `导演段未挂载（t=${readClock()}ms，缺少 .director-stage）`,
        );
      const deadline = performance.now() + 10_000;
      while (document.querySelector(".director-probe")) {
        if (performance.now() > deadline)
          throw new Error(
            `导演段基准布局未完成（t=${readClock()}ms，.director-probe 10 秒内未卸载）`,
          );
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => resolve()),
        );
      }
    } else {
      // LateFilm 用实际 DOM 锚点缓存：根 data-late-anchor-ready='0' 表示尚未测好，
      // 等双 rAF（最多 10 秒）；data-late-anchor-missing 出现说明控件缺失，立即失败，
      // 不允许第一帧 cursor 未测好就开始截帧。
      const lateReady = () =>
        document.querySelector<HTMLElement>(".late-film")?.dataset
          .lateAnchorReady;
      const lateMissing = () =>
        document.querySelector<HTMLElement>(".late-film")?.dataset
          .lateAnchorMissing;
      const missing = lateMissing();
      if (missing)
        throw new Error(
          `LateFilm 锚点缺失（t=${readClock()}ms，data-late-anchor-missing=${missing}）`,
        );
      const deadline = performance.now() + 10_000;
      while (lateReady() === "0") {
        if (performance.now() > deadline)
          throw new Error(
            `LateFilm 锚点测量未完成（t=${readClock()}ms，data-late-anchor-ready 10 秒内仍为 0）`,
          );
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() =>
            requestAnimationFrame(() => resolve()),
          ),
        );
      }
    }
    for (const animation of document.getAnimations()) {
      try {
        animation.finish();
      } catch {
        animation.cancel();
      }
    }
    await document.fonts.ready;
    await Promise.all(
      Array.from(document.images).map((img) =>
        img.decode().catch(() => undefined),
      ),
    );
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
    for (const tabs of document.querySelectorAll<HTMLElement>(".sd-tabs")) {
      const active = tabs.querySelector<HTMLElement>("[data-tab-active='true']");
      const pill = tabs.querySelector<HTMLElement>(".sd-tabs-pill");
      if (active && pill) {
        pill.style.width = `${active.offsetWidth}px`;
        pill.style.transform = `translateX(${active.offsetLeft}px)`;
      }
    }
  },
};
createRoot(document.getElementById("root")!).render(<Root />);
