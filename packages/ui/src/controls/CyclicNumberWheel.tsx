import { useEffect, useLayoutEffect, useRef } from "react";
import { motionDisabled } from "../motion";
import "./CyclicNumberWheel.css";

const ROW = 38;
const offsets = Array.from({ length: 9 }, (_, i) => i - 4);
const modulo = (n: number, count: number) => ((n % count) + count) % count;

/** 固定九个槽位循环复用；指针和弹簧逐帧只写 DOM，不驱动 React。 */
export function CyclicNumberWheel({
  value,
  count,
  label,
  onChange,
  active = true,
}: {
  value: number;
  count: number;
  label: string;
  onChange(value: number): void;
  active?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);
  const position = useRef(value);
  const targetValue = useRef(value);
  const targetIndex = useRef(value);
  const frame = useRef(0);
  const velocity = useRef(0);
  const dragging = useRef<{
    id: number;
    startY: number;
    start: number;
    lastY: number;
    time: number;
    moved: boolean;
  } | null>(null);
  const suppressClick = useRef(false);
  const change = useRef(onChange);
  change.current = onChange;
  const draw = (next: number) => {
    position.current = next;
    const base = Math.round(next);
    root.current
      ?.querySelectorAll<HTMLElement>("[data-wheel-slot]")
      .forEach((node, i) => {
        const distance = base + offsets[i]! - next;
        node.textContent = String(modulo(base + offsets[i]!, count)).padStart(
          2,
          "0",
        );
        node.style.transform = `translate3d(0,${distance * ROW}px,0) scale(${1 - Math.min(1, Math.abs(distance)) * 0.16})`;
        node.style.opacity = String(Math.max(0, 1 - Math.abs(distance) * 0.3));
      });
    root.current?.setAttribute(
      "aria-valuenow",
      String(modulo(Math.round(next), count)),
    );
  };
  const settle = (target: number, speed = velocity.current) => {
    cancelAnimationFrame(frame.current);
    target = Math.round(target);
    targetIndex.current = target;
    targetValue.current = modulo(target, count);
    change.current(modulo(target, count));
    const from = position.current,
      started = performance.now(),
      omega = 24;
    const tick = (now: number) => {
      const t = (now - started) / 1000,
        delta = from - target;
      const c = speed + omega * delta,
        decay = Math.exp(-omega * t);
      velocity.current = (speed - omega * c * t) * decay;
      const next = target + (delta + c * t) * decay;
      if (
        motionDisabled() ||
        t > 0.55 ||
        (Math.abs(next - target) < 0.002 && Math.abs(velocity.current) < 0.05)
      ) {
        draw(modulo(target, count));
        targetIndex.current = modulo(target, count);
        velocity.current = 0;
        frame.current = 0;
      } else {
        draw(next);
        frame.current = requestAnimationFrame(tick);
      }
    };
    frame.current = requestAnimationFrame(tick);
  };
  useLayoutEffect(() => {
    if (!active) {
      cancelAnimationFrame(frame.current);
      frame.current = 0;
      velocity.current = 0;
      const drag = dragging.current;
      dragging.current = null;
      if (drag && root.current?.hasPointerCapture(drag.id))
        root.current.releasePointerCapture(drag.id);
      draw(value);
      return;
    }
    if (dragging.current) return;
    if (frame.current && value !== targetValue.current) {
      const nearest =
        position.current +
        modulo(
          value - modulo(Math.round(position.current), count) + count / 2,
          count,
        ) -
        count / 2;
      settle(nearest);
    } else if (!frame.current) draw(value);
  }, [value, count, active]);
  useEffect(() => {
    const node = root.current;
    if (!node) return;
    let wheelDelta = 0;
    const wheel = (event: WheelEvent) => {
      if (!active) return;
      event.preventDefault();
      event.stopPropagation();
      wheelDelta +=
        event.deltaY *
        (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 160 : 1);
      if (Math.abs(wheelDelta) < 18) return;
      const steps =
        Math.sign(wheelDelta) *
        Math.max(1, Math.min(4, Math.floor(Math.abs(wheelDelta) / 38)));
      wheelDelta = 0;
      settle(
        (frame.current ? targetIndex.current : Math.round(position.current)) +
          steps,
      );
    };
    node.addEventListener("wheel", wheel, { passive: false });
    return () => {
      node.removeEventListener("wheel", wheel);
      cancelAnimationFrame(frame.current);
      const drag = dragging.current;
      if (drag && node.hasPointerCapture(drag.id))
        node.releasePointerCapture(drag.id);
    };
  }, [count, active]);
  return (
    <div
      ref={root}
      className="sd-number-wheel"
      role="spinbutton"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={count - 1}
      aria-valuenow={value}
      onKeyDown={(event) => {
        if (!active) return;
        const amount =
          event.key === "ArrowUp"
            ? -1
            : event.key === "ArrowDown"
              ? 1
              : event.key === "PageUp"
                ? -5
                : event.key === "PageDown"
                  ? 5
                  : 0;
        if (amount) {
          event.preventDefault();
          settle(
            (frame.current
              ? targetIndex.current
              : Math.round(position.current)) + amount,
          );
        }
        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          settle(event.key === "Home" ? 0 : count - 1);
        }
      }}
      onPointerDown={(event) => {
        if (!active || event.button !== 0) return;
        cancelAnimationFrame(frame.current);
        frame.current = 0;
        velocity.current = 0;
        event.currentTarget.setPointerCapture(event.pointerId);
        dragging.current = {
          id: event.pointerId,
          startY: event.clientY,
          lastY: event.clientY,
          start: position.current,
          time: performance.now(),
          moved: false,
        };
        suppressClick.current = false;
      }}
      onPointerMove={(event) => {
        const drag = dragging.current;
        if (!drag || drag.id !== event.pointerId) return;
        const now = performance.now(),
          dt = Math.max(8, now - drag.time);
        velocity.current =
          velocity.current * 0.3 +
          ((drag.lastY - event.clientY) / ROW / dt) * 1000 * 0.7;
        if (Math.abs(event.clientY - drag.startY) > 3) drag.moved = true;
        draw(drag.start + (drag.startY - event.clientY) / ROW);
        drag.lastY = event.clientY;
        drag.time = now;
      }}
      onPointerUp={(event) => {
        const drag = dragging.current;
        if (!drag || drag.id !== event.pointerId) return;
        dragging.current = null;
        suppressClick.current = drag.moved;
        event.currentTarget.releasePointerCapture(event.pointerId);
        if (drag.moved) {
          const speed =
            performance.now() - drag.time > 90
              ? 0
              : Math.max(-25, Math.min(25, velocity.current));
          settle(position.current + speed * 0.09, speed);
        }
      }}
      onPointerCancel={(event) => {
        if (dragging.current?.id !== event.pointerId) return;
        dragging.current = null;
        suppressClick.current = true;
        settle(position.current, 0);
      }}
      onLostPointerCapture={(event) => {
        if (dragging.current?.id !== event.pointerId) return;
        dragging.current = null;
        suppressClick.current = true;
        settle(position.current, 0);
      }}
      onClick={(event) => {
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        const offset =
          (event.clientY -
            event.currentTarget.getBoundingClientRect().top -
            95) /
          ROW;
        settle(Math.round(position.current) + Math.round(offset));
      }}
    >
      {offsets.map((offset) => (
        <span key={offset} data-wheel-slot="" aria-hidden="true" />
      ))}
    </div>
  );
}
