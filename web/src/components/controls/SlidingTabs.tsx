import { useLayoutEffect, useRef, type ReactNode } from "react";
import "./SlidingTabs.css";

export function SlidingTabs<T extends string>({
  items,
  value,
  onChange,
  ariaLabel,
}: {
  items: ReadonlyArray<{
    id: T;
    name: string;
    icon?: ReactNode;
    extra?: ReactNode;
  }>;
  value: T;
  /** T 只由 items 与 value 推导；不加 NoInfer 时，直接传入的 setState 会把它推导成 string。 */
  onChange(id: NoInfer<T>): void;
  ariaLabel: string;
}) {
  const root = useRef<HTMLElement>(null);
  const pill = useRef<HTMLSpanElement>(null);
  // 选中指示条按按钮本地 CSS 像素对齐（offsetParent 是 nav）：屏幕 rect 在
  // NativeScene 嵌套缩放祖先下会被二次换算导致灰底错位/错尺寸。
  const box = useRef<{ left: number; width: number; top: number; height: number } | null>(null);
  const last = useRef(value);

  const place = (animate: boolean) => {
    const list = root.current;
    const marker = pill.current;
    const active = list?.querySelector<HTMLElement>("[data-tab-active='true']");
    if (!list || !marker || !active) return;
    const next = {
      left: active.offsetLeft,
      width: active.offsetWidth,
      top: active.offsetTop,
      height: active.offsetHeight,
    };
    const prev = box.current;
    const same =
      prev !== null &&
      prev.left === next.left &&
      prev.width === next.width &&
      prev.top === next.top &&
      prev.height === next.height;
    // 影片页（capture query / 固定时钟 film）：墙钟动画不得影响帧渲染，确定性落位。
    const film =
      typeof window !== "undefined" &&
      Boolean((window as { __VIDEO__?: unknown }).__VIDEO__);
    if (same && marker.dataset.ready === "true") {
      // 几何未变：不打断进行中的 200ms 过渡（ResizeObserver 初次回调/无变化触发）。
      if (film && typeof marker.getAnimations === "function")
        for (const animation of marker.getAnimations()) animation.cancel();
      return;
    }
    box.current = next;
    marker.style.left = `${next.left}px`;
    marker.style.top = `${next.top}px`;
    marker.style.width = `${next.width}px`;
    marker.style.height = `${next.height}px`;
    marker.dataset.ready = "true";
    const reduced =
      typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!animate || film || reduced || !prev) {
      if (typeof marker.getAnimations === "function")
        for (const animation of marker.getAnimations()) animation.cancel();
      return;
    }
    if (typeof marker.animate !== "function") return;
    if (typeof marker.getAnimations === "function")
      for (const animation of marker.getAnimations()) animation.cancel();
    // 简洁 200ms left/width/top/height 插值：无 scale、无过冲弹性。
    marker.animate(
      [
        {
          left: `${prev.left}px`,
          width: `${prev.width}px`,
          top: `${prev.top}px`,
          height: `${prev.height}px`,
        },
        {
          left: `${next.left}px`,
          width: `${next.width}px`,
          top: `${next.top}px`,
          height: `${next.height}px`,
        },
      ],
      { duration: 200, easing: "cubic-bezier(0.4, 0, 0.2, 1)" },
    );
  };

  useLayoutEffect(() => {
    const list = root.current;
    if (!list) return;
    // moved 与 last 的更新只在提交后的 layout effect 中进行，渲染期不改 ref。
    const moved = last.current !== value;
    last.current = value;
    place(moved);
    // 布局/字号/尺寸变化后重测对齐：除 nav 外同时观察各按钮（全宽 nav 自身不变时
    // 按钮字体/内容宽变化仍需重测）；观察器卸载时清理。
    const targets: Element[] = [list, ...list.querySelectorAll("button")];
    const observer =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => place(false))
        : null;
    if (observer) for (const target of targets) observer.observe(target);
    return () => observer?.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, items]);

  return (
    <nav ref={root} className="sd-tabs" aria-label={ariaLabel}>
      <span className="sd-tabs-pill" ref={pill} aria-hidden="true" />
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={value === item.id ? "is-active" : ""}
          data-tab-active={value === item.id ? "true" : undefined}
          aria-current={value === item.id ? "page" : undefined}
          onClick={() => onChange(item.id)}
        >
          {item.icon ? (
            <span className="sd-tabs-icon" aria-hidden="true">
              {item.icon}
            </span>
          ) : null}
          {item.name}
          {item.extra}
        </button>
      ))}
    </nav>
  );
}
