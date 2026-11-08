import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { animateVisual, cancelVisual } from "./visual";

export function usePresence(
  open: boolean,
  ref: RefObject<HTMLElement | null>,
  options: {
    y?: number;
    scale?: number;
    content?: RefObject<HTMLElement | null>;
  } = {},
) {
  const [retained, setRetained] = useState(open);
  const present = open || retained;
  const generation = useRef(0);
  const previous = useRef<HTMLElement | null>(null);
  const previousContent = useRef<HTMLElement | null>(null);
  const { y = 5, scale = 0.985, content } = options;
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) {
      cancelVisual(previous.current);
      cancelVisual(previousContent.current);
      previous.current = null;
      previousContent.current = null;
      return;
    }
    const token = ++generation.current;
    const panel = content?.current;
    previousContent.current = panel ?? null;
    if (previous.current !== node) {
      node.style.opacity = "0";
      (panel ?? node).style.transform = `translateY(${y}px) scale(${scale})`;
      previous.current = node;
    }
    if (open && !retained) setRetained(true);
    const motions = panel
      ? [
          animateVisual(node, { opacity: open ? 1 : 0 }),
          animateVisual(panel, {
            opacity: open ? 1 : 0,
            y: open ? 0 : y,
            scale: open ? 1 : scale,
          }),
        ]
      : [
          animateVisual(node, {
            opacity: open ? 1 : 0,
            y: open ? 0 : y,
            scale: open ? 1 : scale,
          }),
        ];
    void Promise.all(motions).then(
      () => {
        if (generation.current === token && !open) setRetained(false);
      },
      () => {
        /* 取消不是退场完成，不能卸载重新打开的内容。 */
      },
    );
  }, [open, present, retained, ref, content, y, scale]);
  useEffect(
    () => () => {
      generation.current++;
      cancelVisual(previous.current);
      cancelVisual(previousContent.current);
    },
    [ref, content],
  );
  return present;
}

/** 需要真实布局空间的折叠区，按 CSS 动画的实际完成时间卸载。 */
export function useCssPresence(
  open: boolean,
  ref: RefObject<HTMLElement | null>,
) {
  const [retained, setRetained] = useState(open);
  const present = open || retained;
  const generation = useRef(0);
  useLayoutEffect(() => {
    const token = ++generation.current;
    if (open) {
      if (!retained) setRetained(true);
      return;
    }
    if (!present) return;
    let frame = 0;
    const waitCurrent = () => {
      frame = 0;
      if (generation.current !== token) return;
      const animations =
        ref.current
          ?.getAnimations()
          .filter(
            (animation) =>
              animation.effect?.getComputedTiming().iterations !== Infinity,
          ) ?? [];
      void Promise.all(animations.map((animation) => animation.finished)).then(
        () => {
          if (generation.current === token) setRetained(false);
        },
        () => {
          // 被 CSS 新过渡或减少动态效果替代时，重新核对当前动画；重开会使代次失效。
          if (generation.current === token)
            frame = requestAnimationFrame(waitCurrent);
        },
      );
    };
    waitCurrent();
    return () => {
      generation.current++;
      cancelAnimationFrame(frame);
    };
  }, [open, present, retained, ref]);
  return present;
}
