import { useSyncExternalStore } from "react";

/**
 * 全片唯一的时间源：录制时按帧 seek，预览时由 URL 参数定位。
 * 所有画面状态都是 t 的纯函数，不依赖 Date.now()。
 */
const listeners = new Set<() => void>();
let nowMs = 0;

export function seekClock(ms: number) {
  nowMs = ms;
  listeners.forEach((listener) => listener());
}

export function readClock() {
  return nowMs;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useClock() {
  return useSyncExternalStore(subscribe, readClock);
}

export const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

/** t 在 [start, end) 内的进度，钳到 0..1。 */
export function seg(t: number, start: number, end: number) {
  return clamp01((t - start) / (end - start));
}

export const lerp = (from: number, to: number, k: number) =>
  from + (to - from) * k;

/** 与 CSS 同名的三次贝塞尔求值（x 为时间归一值，返回缓动后的进度）。 */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number) {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;
  const sampleX = (u: number) => ((ax * u + bx) * u + cx) * u;
  const sampleY = (u: number) => ((ay * u + by) * u + cy) * u;
  const sampleDX = (u: number) => (3 * ax * u + 2 * bx) * u + cx;
  return (x: number) => {
    const target = clamp01(x);
    let u = target;
    for (let i = 0; i < 8; i += 1) {
      const err = sampleX(u) - target;
      if (Math.abs(err) < 1e-6) break;
      const d = sampleDX(u);
      if (Math.abs(d) < 1e-6) break;
      u -= err / d;
    }
    u = Math.min(1, Math.max(0, u));
    return sampleY(u);
  };
}

/** 脚本第 3 节的 Motion Token。 */
export const ease = {
  press: cubicBezier(0.2, 0, 0, 1),
  ui: cubicBezier(0.2, 0, 0, 1),
  panel: cubicBezier(0.22, 0.08, 0.16, 1),
  camera: cubicBezier(0.16, 1, 0.3, 1),
  transition: cubicBezier(0.65, 0, 0.35, 1),
};

/** 分段缓动：[start,end] 内用 easeFn(progress)，否则钳在两端。 */
export function tween(
  t: number,
  start: number,
  end: number,
  easeFn: (x: number) => number = ease.ui,
) {
  return easeFn(seg(t, start, end));
}
