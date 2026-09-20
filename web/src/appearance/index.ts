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

export function writeTheme(next: ThemeId, _origin?: ThemeOrigin): Promise<void> {
  const start = document.startViewTransition?.bind(document);
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  // 已经是这个主题、浏览器不支持统一过渡或用户要求减少动态时直接写入。
  if (!start || reduced || document.documentElement.dataset.theme === next) {
    theme.write(next);
    return Promise.resolve();
  }
  const root = document.documentElement;
  root.dataset.themeReveal = "true";
  const transition = start(() => theme.write(next));
  void transition.ready
    .then(() => {
      root.animate(
        { opacity: [0, 1] },
        {
          duration: 160,
          easing: "ease-out",
          pseudoElement: "::view-transition-new(root)",
        },
      );
    })
    .catch(() => undefined);
  return transition.finished.finally(() => {
    delete root.dataset.themeReveal;
  });
}
