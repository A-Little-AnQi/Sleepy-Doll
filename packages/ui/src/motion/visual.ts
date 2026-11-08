/** 动画数值留在浏览器动画引擎中，React 只提交交互状态。 */
const active = new WeakMap<HTMLElement, Animation>();
let reduced: MediaQueryList | undefined;
const springs = new WeakMap<
  HTMLElement,
  {
    from: { x: number; y: number };
    to: { x: number; y: number };
    velocity: { x: number; y: number };
    duration: number;
  }
>();
export function motionDisabled() {
  return (
    Boolean((window as { __VIDEO__?: unknown }).__VIDEO__) ||
    (reduced ??= window.matchMedia("(prefers-reduced-motion: reduce)")).matches
  );
}

export function cancelVisual(node: HTMLElement | null) {
  if (!node) return;
  const animation = active.get(node);
  active.delete(node);
  springs.delete(node);
  animation?.cancel();
}

export function animateVisual(
  node: HTMLElement,
  target: { opacity: number; y?: number; scale?: number },
  duration = 180,
) {
  // 在取消旧动画、写入新目标之前采样真实呈现值，支持连续反向。
  const current = getComputedStyle(node);
  const from = { opacity: current.opacity, transform: current.transform };
  cancelVisual(node);
  node.style.opacity = String(target.opacity);
  node.style.transform =
    !target.y && (target.scale ?? 1) === 1
      ? "none"
      : `translateY(${target.y ?? 0}px) scale(${target.scale ?? 1})`;
  if (motionDisabled() || typeof node.animate !== "function")
    return Promise.resolve();
  const animation = node.animate(
    [from, { opacity: node.style.opacity, transform: node.style.transform }],
    { duration, easing: "cubic-bezier(0.2,0,0,1)" },
  );
  active.set(node, animation);
  return animation.finished.then(() => {
    if (active.get(node) !== animation)
      throw new DOMException("Animation superseded", "AbortError");
    active.delete(node);
  });
}

/** 临界阻尼弹簧：以松手位置及速度生成合成层关键帧，无逐帧 React 更新。 */
export function springPosition(
  node: HTMLElement,
  from: { x: number; y: number },
  to: { x: number; y: number },
  velocity: { x: number; y: number },
) {
  const omega = 24;
  const previous = springs.get(node);
  const playing = active.get(node);
  if (previous && playing) {
    const t = Math.min(0.5, Number(playing.currentTime ?? 0) / 1000);
    const axis = (start: number, end: number, speed: number) => {
      const delta = start - end,
        c = speed + omega * delta,
        decay = Math.exp(-omega * t);
      return (speed - omega * c * t) * decay;
    };
    const x = axis(previous.from.x, previous.to.x, previous.velocity.x);
    const y = axis(previous.from.y, previous.to.y, previous.velocity.y);
    const rendered = new DOMMatrixReadOnly(getComputedStyle(node).transform);
    from = { x: rendered.m41, y: rendered.m42 };
    velocity =
      Number(playing.currentTime ?? 0) >= previous.duration
        ? { x: 0, y: 0 }
        : { x, y };
  } else if (node.isConnected) {
    const rendered = new DOMMatrixReadOnly(getComputedStyle(node).transform);
    from = { x: rendered.m41, y: rendered.m42 };
  }
  cancelVisual(node);
  const transform = (x: number, y: number) => `translate3d(${x}px,${y}px,0)`;
  node.style.transform = transform(to.x, to.y);
  if (motionDisabled()) return Promise.resolve();
  velocity = {
    x: Math.max(-6000, Math.min(6000, velocity.x)),
    y: Math.max(-6000, Math.min(6000, velocity.y)),
  };
  const sample = (start: number, end: number, speed: number, t: number) =>
    end +
    (start - end + (speed + omega * (start - end)) * t) * Math.exp(-omega * t);
  const samples: { transform: string; time: number }[] = [];
  for (let i = 0; i <= 60; i++) {
    const t = (i / 60) * 0.5;
    const x = sample(from.x, to.x, velocity.x, t),
      y = sample(from.y, to.y, velocity.y, t);
    samples.push({ transform: transform(x, y), time: t });
    const speed = (start: number, end: number, v: number) =>
      (v - omega * (v + omega * (start - end)) * t) * Math.exp(-omega * t);
    if (
      i > 1 &&
      Math.abs(x - to.x) < 0.25 &&
      Math.abs(y - to.y) < 0.25 &&
      Math.abs(speed(from.x, to.x, velocity.x)) < 6 &&
      Math.abs(speed(from.y, to.y, velocity.y)) < 6
    )
      break;
  }
  const duration = samples.at(-1)!.time * 1000;
  const frames: Keyframe[] = samples.map((value) => ({
    transform: value.transform,
    offset: (value.time * 1000) / duration,
  }));
  frames[frames.length - 1] = { transform: transform(to.x, to.y), offset: 1 };
  const animation = node.animate(frames, { duration, easing: "linear" });
  active.set(node, animation);
  springs.set(node, { from, to, velocity, duration });
  return animation.finished.then(() => {
    if (active.get(node) !== animation)
      throw new DOMException("Animation superseded", "AbortError");
    active.delete(node);
    springs.delete(node);
  });
}
