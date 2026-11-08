import {
  useLayoutEffect,
  useRef,
  type MutableRefObject,
  type RefObject,
} from "react";
import { motionDisabled } from "@sleepy-doll/ui/motion";

/** 一个控制器协调内容增长、输入框伸缩和阅读锚点，不叠加原生 smooth scroll。 */
export function useConversationScroll(
  viewKey: string,
  loading: boolean,
  scroll: RefObject<HTMLDivElement | null>,
  flow: RefObject<HTMLDivElement | null>,
  dock: RefObject<HTMLDivElement | null>,
  follow: MutableRefObject<boolean>,
  unread: (value: boolean) => void,
) {
  const latest = useRef<() => void>(() => {});
  const unreadCallback = useRef(unread);
  unreadCallback.current = unread;
  useLayoutEffect(() => {
    const root = scroll.current;
    if (!root) return;
    let frame = 0,
      velocity = 0,
      lastTime = 0,
      position = 0;
    let first = true,
      userScroll = false,
      detached = false,
      newContent = false;
    let expected = -1,
      unreadState = false;
    let selecting = false,
      touchY = 0;
    let anchor: { node: Node; character?: number; offset: number } | null =
      null;
    let reduced = motionDisabled();
    const publishUnread = (value: boolean) => {
      if (unreadState === value) return;
      unreadState = value;
      unreadCallback.current(value);
    };
    const findAnchor = (top: number) => {
      const content = flow.current;
      const blocks = content?.querySelectorAll<HTMLElement>(
        "[data-turn-key] .message-content :is(p, pre, li, table, h1, h2, h3, h4)",
      );
      const nodes = blocks?.length
        ? blocks
        : content?.querySelectorAll<HTMLElement>("[data-turn-key]");
      for (const node of nodes ?? []) {
        if (node.closest('[aria-hidden="true"], [inert]')) continue;
        const box = node.getBoundingClientRect();
        if (box.height > 0 && box.bottom > top + 1) {
          const caret = document as Document & {
            caretRangeFromPoint?: (x: number, y: number) => Range | null;
            caretPositionFromPoint?: (
              x: number,
              y: number,
            ) => { offsetNode: Node; offset: number } | null;
          };
          const x = box.left + 2,
            y = Math.max(top + 12, box.top + Math.min(12, box.height / 2));
          const position = caret.caretPositionFromPoint?.(x, y);
          const range = position ? null : caret.caretRangeFromPoint?.(x, y);
          const text = position?.offsetNode ?? range?.startContainer;
          if (
            text?.nodeType === Node.TEXT_NODE &&
            root.contains(text) &&
            text.textContent?.length
          ) {
            const character = Math.min(
              position?.offset ?? range?.startOffset ?? 0,
              text.textContent.length - 1,
            );
            const probe = document.createRange();
            probe.setStart(text, character);
            probe.setEnd(text, character + 1);
            return {
              node: text,
              character,
              offset: probe.getBoundingClientRect().top - top,
            };
          }
          return { node, offset: box.top - top };
        }
      }
      return null;
    };
    const request = () => {
      if (!frame) frame = requestAnimationFrame(tick);
    };
    const tick = (now: number) => {
      frame = 0;
      // 集中读取滚动范围和锚点后，再写本帧位置。
      const max = Math.max(0, root.scrollHeight - root.clientHeight);
      const current = root.scrollTop;
      const top = root.getBoundingClientRect().top;
      if (userScroll) {
        userScroll = false;
        detached = selecting || max - current > (detached ? 4 : 100);
        follow.current = !detached;
        anchor = detached ? findAnchor(top) : null;
        velocity = 0;
        position = current;
      }
      let next = current;
      if (follow.current) {
        detached = false;
        anchor = null;
        publishUnread(false);
        if (first || reduced || Math.abs(max - position) < 0.5) {
          next = max;
          velocity = 0;
        } else {
          const dt = Math.min(0.032, Math.max(0.001, (now - lastTime) / 1000));
          const omega = 26,
            delta = position - max,
            c = velocity + omega * delta;
          const decay = Math.exp(-omega * dt);
          next = max + (delta + c * dt) * decay;
          velocity = (velocity - omega * c * dt) * decay;
          if (Math.abs(next - max) < 0.5 && Math.abs(velocity) < 8) {
            next = max;
            velocity = 0;
          }
        }
      } else {
        velocity = 0;
        if (
          anchor?.node.isConnected &&
          !anchor.node.parentElement?.closest('[aria-hidden="true"], [inert]')
        ) {
          let anchorTop: number;
          if (
            anchor.character !== undefined &&
            anchor.node.textContent?.length
          ) {
            const probe = document.createRange();
            const index = Math.max(
              0,
              Math.min(anchor.character, anchor.node.textContent.length - 1),
            );
            probe.setStart(anchor.node, index);
            probe.setEnd(anchor.node, index + 1);
            anchorTop = probe.getBoundingClientRect().top;
          } else if (anchor.node instanceof HTMLElement)
            anchorTop = anchor.node.getBoundingClientRect().top;
          else {
            anchor = findAnchor(top);
            anchorTop = top + (anchor?.offset ?? 0);
          }
          next += anchorTop - top - (anchor?.offset ?? 0);
        } else anchor = findAnchor(top);
        if (newContent) publishUnread(true);
      }
      first = false;
      newContent = false;
      lastTime = now;
      next = Math.max(0, Math.min(max, next));
      position = next;
      if (Math.abs(next - current) > 0.1) {
        expected = next;
        root.scrollTop = next;
      }
      if (follow.current && Math.abs(next - max) > 0.5) request();
    };
    const onScroll = () => {
      if (Math.abs(root.scrollTop - expected) < 1) {
        expected = -1;
        return;
      }
      userScroll = true;
      request();
    };
    const onWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) {
        detached = true;
        follow.current = false;
        velocity = 0;
        userScroll = true;
      }
      request();
    };
    const onSelection = () => {
      const selection = window.getSelection();
      selecting = Boolean(
        selection &&
        !selection.isCollapsed &&
        selection.anchorNode &&
        root.contains(selection.anchorNode),
      );
      if (selecting) {
        follow.current = false;
        detached = true;
        userScroll = true;
        velocity = 0;
        request();
      }
    };
    const onTouchStart = (event: TouchEvent) => {
      touchY = event.touches[0]?.clientY ?? 0;
    };
    const onTouchMove = (event: TouchEvent) => {
      if ((event.touches[0]?.clientY ?? touchY) - touchY > 2) {
        follow.current = false;
        detached = true;
        userScroll = true;
        velocity = 0;
        request();
      }
    };
    const observer = new ResizeObserver(request);
    observer.observe(root);
    if (flow.current) observer.observe(flow.current);
    if (dock.current) observer.observe(dock.current);
    const mutation = new MutationObserver(() => {
      newContent = true;
      request();
    });
    if (flow.current)
      mutation.observe(flow.current, {
        childList: true,
        characterData: true,
        subtree: true,
      });
    root.addEventListener("scroll", onScroll, { passive: true });
    root.addEventListener("wheel", onWheel, { passive: true });
    root.addEventListener("touchstart", onTouchStart, { passive: true });
    root.addEventListener("touchmove", onTouchMove, { passive: true });
    document.addEventListener("selectionchange", onSelection);
    const media = matchMedia("(prefers-reduced-motion: reduce)");
    const preference = () => {
      reduced = motionDisabled();
      request();
    };
    media.addEventListener("change", preference);
    latest.current = () => {
      position = root.scrollTop;
      velocity = 0;
      selecting = false;
      follow.current = true;
      detached = false;
      userScroll = false;
      publishUnread(false);
      request();
    };
    request();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutation.disconnect();
      root.removeEventListener("scroll", onScroll);
      root.removeEventListener("wheel", onWheel);
      root.removeEventListener("touchstart", onTouchStart);
      root.removeEventListener("touchmove", onTouchMove);
      document.removeEventListener("selectionchange", onSelection);
      media.removeEventListener("change", preference);
      latest.current = () => {};
    };
  }, [viewKey, loading, scroll, flow, dock, follow]);
  return () => latest.current();
}
