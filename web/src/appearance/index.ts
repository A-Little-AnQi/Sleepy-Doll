import { preference } from "./preference";

export type ThemeId = "light" | "dark";
export type ThemeOrigin = { x: number; y: number };

const THEME_KEY = "sleepy-doll-theme";

const theme = preference<ThemeId>(
  THEME_KEY,
  (stored) => (stored === "dark" ? "dark" : "light"),
  (value) => {
    document.documentElement.dataset.theme = value;
  },
);

export const readTheme = theme.read;
export const subscribeTheme = theme.subscribe;
export const restoreTheme = theme.restore;

/** 当前主题。 */
export function useTheme(): ThemeId {
  return theme.use();
}

let lastPointer: ThemeOrigin | undefined;

if (typeof window !== "undefined") {
  window.addEventListener(
    "pointerdown",
    (event) => {
      lastPointer = { x: event.clientX, y: event.clientY };
    },
    true,
  );
}

function resolveOrigin(origin?: ThemeOrigin): ThemeOrigin {
  if (origin) return origin;
  if (lastPointer) return lastPointer;
  const el = document.activeElement;
  if (el instanceof HTMLElement) {
    const box = el.getBoundingClientRect();
    if (box.width || box.height) {
      return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
    }
  }
  return { x: window.innerWidth / 2, y: window.innerHeight / 2 };
}

function revealRadius(x: number, y: number) {
  return Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y),
  );
}

export function writeTheme(next: ThemeId, origin?: ThemeOrigin): Promise<void> {
  const start = document.startViewTransition?.bind(document);
  // 已经是这个主题时不放动画，仍走一次写入。
  if (!start || document.documentElement.dataset.theme === next) {
    theme.write(next);
    return Promise.resolve();
  }
  const point = resolveOrigin(origin);
  const radius = revealRadius(point.x, point.y);
  const root = document.documentElement;
  root.dataset.themeReveal = "true";
  const transition = start(() => theme.write(next));
  void transition.ready
    .then(() => {
      root.animate(
        {
          clipPath: [
            `circle(0px at ${point.x}px ${point.y}px)`,
            `circle(${radius}px at ${point.x}px ${point.y}px)`,
          ],
        },
        {
          duration: 560,
          easing: "cubic-bezier(0.22, 0.08, 0.16, 1)",
          pseudoElement: "::view-transition-new(root)",
        },
      );
    })
    .catch(() => undefined);
  return transition.finished.finally(() => {
    delete root.dataset.themeReveal;
  });
}
