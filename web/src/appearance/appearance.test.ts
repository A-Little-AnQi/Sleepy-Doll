import { afterEach, expect, it, vi } from "vitest";
import { writeTheme } from ".";

afterEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.themeReveal;
  vi.unstubAllGlobals();
});

it("applies the theme immediately when view transitions are unavailable", async () => {
  await writeTheme("dark", { x: 10, y: 20 });
  expect(document.documentElement.dataset.theme).toBe("dark");
  expect(localStorage.getItem("sleepy-doll-theme")).toBe("dark");
});

it("reveals the new theme from the click point", async () => {
  const animate = vi.fn();
  const ready = Promise.resolve();
  const finished = Promise.resolve();
  const startViewTransition = vi.fn((callback: () => void) => {
    callback();
    return { ready, finished, updateCallbackDone: Promise.resolve() };
  });
  Object.defineProperty(document, "startViewTransition", {
    configurable: true,
    value: startViewTransition,
  });
  document.documentElement.animate = animate as typeof document.documentElement.animate;
  document.documentElement.dataset.theme = "light";
  await writeTheme("dark", { x: 12, y: 34 });
  expect(startViewTransition).toHaveBeenCalledTimes(1);
  expect(document.documentElement.dataset.theme).toBe("dark");
  await ready;
  expect(animate).toHaveBeenCalled();
  const [keyframes, options] = animate.mock.calls[0] as [
    { clipPath: string[] },
    KeyframeAnimationOptions,
  ];
  expect(keyframes.clipPath[0]).toBe("circle(0px at 12px 34px)");
  expect(keyframes.clipPath[1]).toMatch(/^circle\(.+ at 12px 34px\)$/);
  expect(options.pseudoElement).toBe("::view-transition-new(root)");
  await finished;
  expect(document.documentElement.dataset.themeReveal).toBeUndefined();
});
