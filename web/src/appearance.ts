export type ThemeId = "light" | "dark";

const THEME_KEY = "sleepy-doll-theme";
const MOTION_KEY = "sleepy-doll-reduced-motion";

export function readTheme(): ThemeId {
  return localStorage.getItem(THEME_KEY) === "dark" ? "dark" : "light";
}

export function writeTheme(theme: ThemeId) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
}

export function readReducedMotion() {
  return localStorage.getItem(MOTION_KEY) === "true";
}

export function writeReducedMotion(on: boolean) {
  document.documentElement.dataset.reducedMotion = String(on);
  localStorage.setItem(MOTION_KEY, String(on));
}
