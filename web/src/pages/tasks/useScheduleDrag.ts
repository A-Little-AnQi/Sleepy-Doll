import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import type { ScheduleAction } from "../../ipc/scheduler";
import { cancelVisual, springPosition } from "@sleepy-doll/ui/motion";

export type ScheduleDragSource =
  | { kind: "library"; action: ScheduleAction }
  | { kind: "entry"; key: string; title: string };
export function useScheduleDrag(
  queue: RefObject<HTMLElement | null>,
  library: RefObject<HTMLElement | null>,
  drop: (source: ScheduleDragSource, index: number) => string | undefined,
) {
  const [source, setSource] = useState<ScheduleDragSource | null>(null);
  const ghost = useRef<HTMLDivElement>(null),
    marker = useRef<HTMLDivElement>(null);
  const commit = useRef(drop);
  const releaseFrame = useRef(0),
    generation = useRef(0);
  commit.current = drop;
  const motion = useRef<{
    source: ScheduleDragSource;
    id: number;
    capture: HTMLElement;
    startX: number;
    startY: number;
    x: number;
    y: number;
    active: boolean;
    index: number | null;
    frame: number;
    time: number;
    vx: number;
    vy: number;
    offsetX: number;
    offsetY: number;
    width: number;
    height: number;
  } | null>(null);
  useLayoutEffect(() => {
    const drag = motion.current,
      node = ghost.current;
    if (!drag?.active || !node) return;
    node.style.width = `${drag.width}px`;
    node.style.height = `${drag.height}px`;
    node.style.transform = `translate3d(${drag.x - drag.offsetX}px,${drag.y - drag.offsetY}px,0)`;
    node.style.opacity = "1";
  }, [source]);
  useEffect(() => {
    const draw = () => {
      const drag = motion.current;
      if (!drag?.active) return;
      // 同一帧集中读命中边界，之后只写影像、插入线和滚动位置。
      const list = queue.current,
        lib = library.current;
      if (!list) return;
      const box = list.getBoundingClientRect(),
        libBox = lib?.getBoundingClientRect();
      const rows = [...list.querySelectorAll<HTMLElement>("[data-entry-key]")]
        .filter(
          (row) =>
            !(
              drag.source.kind === "entry" &&
              row.dataset.entryKey === drag.source.key
            ),
        )
        .map((row) => row.getBoundingClientRect());
      const inside =
        drag.x >= box.left &&
        drag.x <= box.right &&
        drag.y >= box.top &&
        drag.y <= box.bottom;
      const next = inside
        ? rows.findIndex((row) => drag.y < row.top + row.height / 2)
        : null;
      const index = next === -1 ? rows.length : next;
      drag.index = index;
      const markerY =
        index === null
          ? 0
          : index === rows.length
            ? (rows.at(-1)?.bottom ?? box.top + 12)
            : rows[index]!.top;
      if (ghost.current) {
        ghost.current.style.opacity = "1";
        ghost.current.style.transform = `translate3d(${drag.x - drag.offsetX}px,${drag.y - drag.offsetY}px,0)`;
      }
      if (marker.current) {
        marker.current.style.opacity = inside ? "1" : "0";
        marker.current.style.transform = `translate3d(${box.left + 8}px,${Math.max(box.top + 4, Math.min(markerY, box.bottom - 4))}px,0)`;
        marker.current.style.width = `${box.width - 16}px`;
      }
      const scroll = (element: HTMLElement, rect: DOMRect) => {
        if (
          drag.x < rect.left ||
          drag.x > rect.right ||
          drag.y < rect.top ||
          drag.y > rect.bottom
        )
          return false;
        const delta =
          drag.y < rect.top + 32
            ? -Math.min(10, (rect.top + 32 - drag.y) * 0.3)
            : drag.y > rect.bottom - 32
              ? Math.min(10, (drag.y - rect.bottom + 32) * 0.3)
              : 0;
        if (delta) {
          const previous = element.scrollTop;
          element.scrollTop += delta;
          return element.scrollTop !== previous;
        }
        return false;
      };
      const scrolling =
        scroll(list, box) || (lib && libBox ? scroll(lib, libBox) : false);
      drag.frame = scrolling ? requestAnimationFrame(draw) : 0;
    };
    const move = (event: PointerEvent) => {
      const drag = motion.current;
      if (!drag || drag.id !== event.pointerId) return;
      const now = performance.now(),
        dt = Math.max(8, now - drag.time);
      drag.vx = ((event.clientX - drag.x) / dt) * 1000;
      drag.vy = ((event.clientY - drag.y) / dt) * 1000;
      drag.time = now;
      drag.x = event.clientX;
      drag.y = event.clientY;
      if (
        !drag.active &&
        Math.hypot(drag.x - drag.startX, drag.y - drag.startY) > 4
      ) {
        drag.active = true;
        setSource(drag.source);
        document.documentElement.dataset.sdDragging = "schedule";
      }
      if (drag.active) {
        event.preventDefault();
        if (!drag.frame) drag.frame = requestAnimationFrame(draw);
      }
    };
    const finish = (apply: boolean) => {
      const drag = motion.current;
      if (!drag) return;
      cancelAnimationFrame(drag.frame);
      let destination: string | undefined;
      if (apply && drag.active) {
        draw();
        cancelAnimationFrame(drag.frame);
        if (drag.index !== null)
          destination = commit.current(drag.source, drag.index);
      }
      motion.current = null;
      if (drag.capture.hasPointerCapture(drag.id))
        drag.capture.releasePointerCapture(drag.id);
      delete document.documentElement.dataset.sdDragging;
      if (marker.current) marker.current.style.opacity = "0";
      if (!drag.active || !ghost.current) {
        setSource(null);
        return;
      }
      const current = ++generation.current;
      releaseFrame.current = requestAnimationFrame(() => {
        const node = ghost.current;
        if (!node || generation.current !== current) return;
        const target = destination
          ? queue.current?.querySelector<HTMLElement>(
              `[data-entry-key="${destination}"]`,
            )
          : drag.capture.closest<HTMLElement>(
              ".schedule-library-row,.schedule-queue-row",
            );
        const rect = target?.getBoundingClientRect();
        const to = rect
          ? { x: rect.left, y: rect.top }
          : { x: drag.x - drag.offsetX, y: drag.y - drag.offsetY };
        const velocity =
          performance.now() - drag.time > 90
            ? { x: 0, y: 0 }
            : { x: drag.vx, y: drag.vy };
        const fade = node.animate([{ opacity: 1 }, { opacity: 0 }], {
          duration: 170,
          delay: 100,
          fill: "forwards",
        });
        void springPosition(
          node,
          { x: drag.x - drag.offsetX, y: drag.y - drag.offsetY },
          to,
          velocity,
        ).then(
          () => {
            if (generation.current === current) {
              fade.cancel();
              setSource(null);
            }
          },
          () => {
            fade.cancel();
          },
        );
      });
    };
    const up = (event: PointerEvent) => {
      if (motion.current?.id === event.pointerId) finish(true);
    };
    const cancel = (event: PointerEvent) => {
      if (motion.current?.id === event.pointerId) finish(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape" && motion.current) {
        event.preventDefault();
        event.stopImmediatePropagation();
        finish(false);
      }
    };
    const blur = () => finish(false);
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", blur);
    window.addEventListener("keydown", key, true);
    return () => {
      finish(false);
      ++generation.current;
      cancelAnimationFrame(releaseFrame.current);
      cancelVisual(ghost.current);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", blur);
      window.removeEventListener("keydown", key, true);
    };
  }, [queue, library]);
  const start = (
    event: ReactPointerEvent<HTMLElement>,
    next: ScheduleDragSource,
  ) => {
    if (event.button !== 0 || motion.current) return;
    ++generation.current;
    cancelAnimationFrame(releaseFrame.current);
    cancelVisual(ghost.current);
    ghost.current?.getAnimations().forEach((animation) => animation.cancel());
    setSource(null);
    event.currentTarget.setPointerCapture(event.pointerId);
    const rect = (
      event.currentTarget.closest(
        ".schedule-library-row,.schedule-queue-row",
      ) ?? event.currentTarget
    ).getBoundingClientRect();
    motion.current = {
      source: next,
      id: event.pointerId,
      capture: event.currentTarget,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
      index: null,
      frame: 0,
      time: performance.now(),
      vx: 0,
      vy: 0,
      offsetX: event.clientX - rect.left,
      offsetY: event.clientY - rect.top,
      width: rect.width,
      height: rect.height,
    };
  };
  return { source, ghost, marker, start };
}
