import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { flushSync } from "react-dom";
import type { ScheduleAction } from "../../ipc/scheduler";
import {
  cancelVisual,
  motionDisabled,
  springPosition,
} from "@sleepy-doll/ui/motion";

export type ScheduleDragSource =
  | { kind: "library"; action: ScheduleAction }
  | { kind: "entry"; key: string; title: string };
export function useScheduleDrag(
  queue: RefObject<HTMLElement | null>,
  library: RefObject<HTMLElement | null>,
  drop: (source: ScheduleDragSource, index: number) => string | undefined,
  discard: (key: string) => void,
) {
  const [presentation, setPresentation] = useState<{
    source: ScheduleDragSource;
    landingKey?: string;
  } | null>(null);
  const source = presentation?.source ?? null;
  const ghost = useRef<HTMLDivElement>(null),
    marker = useRef<HTMLDivElement>(null);
  const commit = useRef(drop);
  const remove = useRef(discard);
  const releaseFrame = useRef(0),
    generation = useRef(0);
  const releaseAnimations = useRef<Animation[]>([]);
  const releaseGhost = useRef<HTMLElement | null>(null);
  const dropZone = useRef<HTMLElement | null>(null);
  commit.current = drop;
  remove.current = discard;
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
    discard: boolean;
    frame: number;
    time: number;
    vx: number;
    vy: number;
    offsetX: number;
    offsetY: number;
    width: number;
    height: number;
  } | null>(null);
  const released = useRef<NonNullable<typeof motion.current> | null>(null);
  useLayoutEffect(() => {
    const drag = motion.current ?? released.current,
      node = ghost.current;
    if (!drag?.active || !node) return;
    node.style.width =
      drag.source.kind === "entry" ? "max-content" : `${drag.width}px`;
    node.style.maxWidth = drag.source.kind === "entry" ? "280px" : "";
    node.style.height = `${drag.height}px`;
    node.style.transform = `translate3d(${drag.x - drag.offsetX}px,${drag.y - drag.offsetY}px,0)`;
    node.style.opacity = "1";
  }, [source]);
  const stopRelease = () => {
    ++generation.current;
    cancelAnimationFrame(releaseFrame.current);
    releaseFrame.current = 0;
    cancelVisual(releaseGhost.current ?? ghost.current);
    for (const animation of releaseAnimations.current) animation.cancel();
    releaseAnimations.current = [];
    releaseGhost.current = null;
    released.current = null;
  };
  useEffect(() => {
    const clearDragging = () => {
      if (document.documentElement.dataset.sdDragging === "schedule")
        delete document.documentElement.dataset.sdDragging;
    };
    const clearDropHint = () => {
      dropZone.current?.removeAttribute("data-drop-action");
      dropZone.current = null;
      if (marker.current) marker.current.style.opacity = "0";
    };
    const draw = (allowScroll: boolean) => {
      const drag = motion.current;
      if (!drag?.active) return;
      // 同一帧集中读命中边界，之后只写影像、插入线和滚动位置。
      const list = queue.current,
        lib = library.current;
      if (!list) return;
      const box = list.getBoundingClientRect(),
        libBox = lib?.getBoundingClientRect();
      const panel = lib?.closest<HTMLElement>(".schedule-library") ?? lib;
      const panelBox = panel?.getBoundingClientRect();
      const region =
        panelBox?.width && panelBox.height
          ? panel
          : list
              .closest(".sd-dialog")
              ?.querySelector<HTMLElement>(
                '.schedule-mobile-tabs [data-tab-id="library"]',
              );
      const regionBox =
        region === panel ? panelBox : region?.getBoundingClientRect();
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
      drag.discard = Boolean(
        drag.source.kind === "entry" &&
        regionBox &&
        drag.x >= regionBox.left &&
        drag.x <= regionBox.right &&
        drag.y >= regionBox.top &&
        drag.y <= regionBox.bottom,
      );
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
      const scrollTarget = (element: HTMLElement, rect: DOMRect) => {
        if (
          !allowScroll ||
          drag.x < rect.left ||
          drag.x > rect.right ||
          drag.y < rect.top ||
          drag.y > rect.bottom
        )
          return null;
        const delta =
          drag.y < rect.top + 32
            ? -Math.min(10, (rect.top + 32 - drag.y) * 0.3)
            : drag.y > rect.bottom - 32
              ? Math.min(10, (drag.y - rect.bottom + 32) * 0.3)
              : 0;
        const top = Math.max(
          0,
          Math.min(
            element.scrollHeight - element.clientHeight,
            element.scrollTop + delta,
          ),
        );
        return top === element.scrollTop ? null : { element, top };
      };
      const scrolling =
        scrollTarget(list, box) ??
        (drag.source.kind === "library" && lib && libBox
          ? scrollTarget(lib, libBox)
          : null);
      // 几何读取完成后再更新拖影、目标提示和滚动。
      if (ghost.current) {
        ghost.current.style.opacity = "1";
        ghost.current.style.transform = `translate3d(${drag.x - drag.offsetX}px,${drag.y - drag.offsetY}px,0)`;
        const action = drag.discard ? "discard" : "";
        if (ghost.current.dataset.dropAction !== action)
          ghost.current.dataset.dropAction = action;
      }
      if (marker.current) {
        marker.current.style.opacity = inside ? "1" : "0";
        marker.current.style.transform = `translate3d(${box.left + 8}px,${Math.max(box.top + 4, Math.min(markerY, box.bottom - 4))}px,0)`;
        marker.current.style.width = `${box.width - 16}px`;
      }
      if (dropZone.current !== region)
        dropZone.current?.removeAttribute("data-drop-action");
      dropZone.current = region ?? null;
      if (region) {
        if (drag.discard && region.dataset.dropAction !== "discard")
          region.dataset.dropAction = "discard";
        else if (!drag.discard && region.hasAttribute("data-drop-action"))
          region.removeAttribute("data-drop-action");
      }
      if (scrolling) scrolling.element.scrollTop = scrolling.top;
      drag.frame = scrolling ? requestAnimationFrame(() => draw(true)) : 0;
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
        setPresentation({ source: drag.source });
        document.documentElement.dataset.sdDragging = "schedule";
      }
      if (drag.active) {
        event.preventDefault();
        if (!drag.frame) drag.frame = requestAnimationFrame(() => draw(true));
      }
    };
    const finish = (apply: boolean) => {
      const drag = motion.current;
      if (!drag) return;
      cancelAnimationFrame(drag.frame);
      if (drag.active) {
        draw(false);
        cancelAnimationFrame(drag.frame);
      }
      motion.current = null;
      if (drag.capture.hasPointerCapture(drag.id))
        drag.capture.releasePointerCapture(drag.id);
      clearDropHint();
      if (!drag.active) {
        clearDragging();
        return;
      }
      const discarded = apply && drag.discard && drag.source.kind === "entry";
      let destination: string | undefined;
      released.current = drag;
      // 只在松手提交语义状态；目标行与拖影在同次提交交接，不能露出一帧半透明行。
      flushSync(() => {
        if (discarded && drag.source.kind === "entry")
          remove.current(drag.source.key);
        else if (apply && drag.index !== null)
          destination = commit.current(drag.source, drag.index);
        const landingKey = discarded
          ? undefined
          : (destination ??
            (drag.source.kind === "entry" ? drag.source.key : undefined));
        setPresentation({
          source: drag.source,
          ...(landingKey ? { landingKey } : {}),
        });
      });
      if (discarded && document.activeElement === document.body)
        queue.current?.focus({ preventScroll: true });
      clearDragging();
      const current = ++generation.current;
      const complete = () => {
        if (generation.current !== current || !ghost.current?.isConnected)
          return;
        flushSync(() => setPresentation(null));
        stopRelease();
      };
      releaseFrame.current = requestAnimationFrame(() => {
        releaseFrame.current = 0;
        const node = ghost.current;
        if (!node || generation.current !== current) return;
        releaseGhost.current = node;
        if (discarded) {
          const transform = getComputedStyle(node).transform;
          const animation = node.animate(
            [
              { opacity: 1, transform },
              { opacity: 0, transform: `${transform} scale(0.94)` },
            ],
            {
              duration: motionDisabled() ? 0 : 140,
              easing: "cubic-bezier(0.2,0,0,1)",
              fill: "forwards",
            },
          );
          releaseAnimations.current.push(animation);
          void animation.finished.then(complete, () => {});
          return;
        }
        node.removeAttribute("data-drop-action");
        const candidate = destination
          ? queue.current?.querySelector<HTMLElement>(
              `[data-entry-key="${destination}"]`,
            )
          : drag.capture.closest<HTMLElement>(
              ".schedule-library-row,.schedule-queue-row",
            );
        const target =
          candidate?.isConnected && candidate.getClientRects().length
            ? candidate
            : null;
        const rect = target?.getBoundingClientRect();
        const to = rect
          ? { x: rect.left, y: rect.top }
          : { x: drag.x - drag.offsetX, y: drag.y - drag.offsetY };
        const velocity =
          performance.now() - drag.time > 90
            ? { x: 0, y: 0 }
            : { x: drag.vx, y: drag.vy };
        void springPosition(
          node,
          { x: drag.x - drag.offsetX, y: drag.y - drag.offsetY },
          to,
          velocity,
        ).then(
          async () => {
            if (generation.current !== current) return;
            // 先到真实落点再交叉淡入，弹簧运行期间拖影始终完整可见。
            const options = {
              duration: motionDisabled() ? 0 : 90,
              fill: "forwards" as const,
            };
            const fade = node.animate(
              [{ opacity: 1 }, { opacity: 0 }],
              options,
            );
            const landing =
              destination || drag.source.kind === "entry" ? target : null;
            const reveal = landing?.animate(
              [{ opacity: 0 }, { opacity: 1 }],
              options,
            );
            releaseAnimations.current.push(fade, ...(reveal ? [reveal] : []));
            try {
              await Promise.all([fade.finished, reveal?.finished]);
              complete();
            } catch {
              /* 新手势拥有后续清理，取消的动画不能卸载它。 */
            }
          },
          () => {},
        );
      });
    };
    const up = (event: PointerEvent) => {
      const drag = motion.current;
      if (drag?.id !== event.pointerId) return;
      drag.x = event.clientX;
      drag.y = event.clientY;
      finish(true);
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
      const drag = motion.current;
      motion.current = null;
      if (drag) {
        cancelAnimationFrame(drag.frame);
        if (drag.capture.hasPointerCapture(drag.id))
          drag.capture.releasePointerCapture(drag.id);
      }
      clearDropHint();
      clearDragging();
      stopRelease();
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
    stopRelease();
    if (ghost.current) flushSync(() => setPresentation(null));
    event.preventDefault();
    event.currentTarget.focus({ preventScroll: true });
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
      discard: false,
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
  return { source, landingKey: presentation?.landingKey, ghost, marker, start };
}
