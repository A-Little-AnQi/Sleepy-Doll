import { afterEach, expect, it } from "vitest";
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampSidebarWidth,
  readSidebarWidth,
} from "./sidebar-layout";

afterEach(() => {
  localStorage.clear();
});

it("keeps the default width inside the allowed range", () => {
  expect(clampSidebarWidth(SIDEBAR_DEFAULT_WIDTH, 1440)).toBe(
    SIDEBAR_DEFAULT_WIDTH,
  );
});

it("does not shrink past the minimum or grow past the max", () => {
  expect(clampSidebarWidth(80, 1440)).toBe(SIDEBAR_MIN_WIDTH);
  expect(clampSidebarWidth(900, 1440)).toBe(480);
});

it("leaves room for the main pane on a short window", () => {
  expect(clampSidebarWidth(480, 800)).toBeLessThanOrEqual(440);
});

it("reads a stored width and ignores junk", () => {
  localStorage.setItem("sleepy-doll-sidebar-width", "320");
  expect(readSidebarWidth(1440)).toBe(320);
  localStorage.setItem("sleepy-doll-sidebar-width", "nope");
  expect(readSidebarWidth(1440)).toBe(SIDEBAR_DEFAULT_WIDTH);
});
