import { useLayoutEffect, useRef, type RefObject } from "react";
import { motionDisabled } from "./visual";

/** 先集中读位置与呈现变换，再以稳定元素身份写 FLIP。 */
export function useFlipList(
  ref: RefObject<HTMLElement | null>,
  version: string,
) {
  const previous = useRef(new Map<HTMLElement, number>());
  const motions = useRef(new Map<HTMLElement, Animation>());
  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const box = root.getBoundingClientRect();
    if (!box.width) {
      for (const animation of motions.current.values()) animation.cancel();
      motions.current.clear();
      previous.current.clear();
      return;
    }
    const scale = box.width / (root.offsetWidth || box.width || 1);
    const measured = [
      ...root.querySelectorAll<HTMLElement>("[data-sd-slot][data-sd-id]"),
    ]
      .filter((node) => !node.closest(".app-group-chats.is-collapsed"))
      .map((node) => {
        const rect = node.getBoundingClientRect();
        const transform = new DOMMatrixReadOnly(
          getComputedStyle(node).transform,
        );
        return {
          node,
          y: (rect.top - box.top) / scale + root.scrollTop - transform.m42,
          offset: transform.m42,
        };
      });
    const next = new Map<HTMLElement, number>();
    for (const { node, y } of measured) next.set(node, y);
    const oldScroll = root.scrollTop;
    let newScroll = oldScroll;
    if (oldScroll > 0 && !document.documentElement.dataset.sdDragging) {
      const anchor = [...previous.current]
        .filter(([node, y]) => next.has(node) && y >= oldScroll - 1)
        .sort((a, b) => a[1] - b[1])[0];
      if (anchor)
        newScroll = Math.max(
          0,
          Math.min(
            root.scrollHeight - root.clientHeight,
            oldScroll + next.get(anchor[0])! - anchor[1],
          ),
        );
    }
    if (newScroll !== oldScroll) root.scrollTop = newScroll;
    for (const { node, y, offset } of measured) {
      const old = previous.current.get(node);
      motions.current.get(node)?.cancel();
      motions.current.delete(node);
      const delta = (old ?? y) + offset - y + newScroll - oldScroll;
      if (
        old === undefined ||
        node.hasAttribute("data-sd-flip-skip") ||
        motionDisabled() ||
        Math.abs(delta) < 0.5
      )
        continue;
      const animation = node.animate(
        [
          { transform: `translateY(${delta}px)` },
          { transform: "translateY(0)" },
        ],
        { duration: 190, easing: "cubic-bezier(0.2,0,0,1)" },
      );
      motions.current.set(node, animation);
      void animation.finished.then(
        () => {
          if (motions.current.get(node) === animation)
            motions.current.delete(node);
        },
        () => {},
      );
    }
    for (const [node, animation] of motions.current)
      if (!next.has(node)) {
        animation.cancel();
        motions.current.delete(node);
      }
    previous.current = next;
  }, [ref, version]);
  useLayoutEffect(
    () => () => {
      for (const animation of motions.current.values()) animation.cancel();
      motions.current.clear();
    },
    [],
  );
}
