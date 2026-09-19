export const SIDEBAR_DEFAULT_WIDTH = 248;
export const SIDEBAR_MIN_WIDTH = 200;
export const SIDEBAR_MAX_WIDTH = 480;
export const SIDEBAR_MAIN_RESERVE = 360;
export const SIDEBAR_WIDTH_KEY = "sleepy-doll-sidebar-width";

export function clampPreferredSidebarWidth(width: number) {
  if (!Number.isFinite(width)) return SIDEBAR_DEFAULT_WIDTH;
  return Math.min(
    SIDEBAR_MAX_WIDTH,
    Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)),
  );
}

export function clampSidebarWidth(width: number, viewport: number) {
  const preferred = clampPreferredSidebarWidth(width);
  const max = Math.min(
    SIDEBAR_MAX_WIDTH,
    Math.max(SIDEBAR_MIN_WIDTH, viewport - SIDEBAR_MAIN_RESERVE),
  );
  return Math.min(max, preferred);
}

export function readSidebarWidth(viewport: number, stored?: string | null) {
  const raw =
    stored === undefined ? localStorage.getItem(SIDEBAR_WIDTH_KEY) : stored;
  return clampSidebarWidth(Number(raw), viewport);
}
