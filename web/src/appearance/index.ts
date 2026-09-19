export type ThemeId = "light" | "dark";
export type ThemeOrigin = { x: number; y: number };

const THEME_KEY = "sleepy-doll-theme";

export function readTheme(): ThemeId {
  return localStorage.getItem(THEME_KEY) === "dark" ? "dark" : "light";
}

function applyTheme(theme: ThemeId) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
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

export function writeTheme(
  theme: ThemeId,
  origin?: ThemeOrigin,
): Promise<void> {
  if (document.documentElement.dataset.theme === theme) {
    localStorage.setItem(THEME_KEY, theme);
    return Promise.resolve();
  }
  const start = document.startViewTransition?.bind(document);
  if (!start) {
    applyTheme(theme);
    return Promise.resolve();
  }
  const point = resolveOrigin(origin);
  const radius = revealRadius(point.x, point.y);
  const root = document.documentElement;
  root.dataset.themeReveal = "true";
  const transition = start(() => applyTheme(theme));
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
