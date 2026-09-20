import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { readTheme, subscribeTheme, writeTheme } from ".";

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

it("crossfades the whole interface as one theme layer", async () => {
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
  document.documentElement.animate =
    animate as typeof document.documentElement.animate;
  document.documentElement.dataset.theme = "light";
  await writeTheme("dark", { x: 12, y: 34 });
  expect(startViewTransition).toHaveBeenCalledTimes(1);
  expect(document.documentElement.dataset.theme).toBe("dark");
  await ready;
  expect(animate).toHaveBeenCalled();
  const [keyframes, options] = animate.mock.calls[0] as [
    { opacity: number[] },
    KeyframeAnimationOptions,
  ];
  expect(keyframes.opacity).toEqual([0, 1]);
  expect(options.duration).toBe(160);
  expect(options.pseudoElement).toBe("::view-transition-new(root)");
  await finished;
  expect(document.documentElement.dataset.themeReveal).toBeUndefined();
});

/** `web/index.html` 在首帧之前自己读一次 localStorage，两份解码规则必须给出同一个结果。 */
it("applies the stored theme before the first paint", () => {
  // `npm test` 在仓库根下执行，路径按根算。
  const html = readFileSync("web/index.html", "utf8");
  const [, bootstrap] = /<script>([\s\S]*?)<\/script>/.exec(html) ?? [];
  if (!bootstrap) throw new Error("web/index.html 里没有首帧主题引导");

  for (const stored of [null, "dark", "light", "改过的值"]) {
    if (stored === null) localStorage.removeItem("sleepy-doll-theme");
    else localStorage.setItem("sleepy-doll-theme", stored);
    delete document.documentElement.dataset.theme;

    (0, eval)(bootstrap);

    expect(document.documentElement.dataset.theme, String(stored)).toBe(
      readTheme(),
    );
  }
});

it("tells every reader about the new theme", async () => {
  const seen: string[] = [];
  const stop = subscribeTheme(() => seen.push(readTheme()));

  await writeTheme("dark");
  stop();
  await writeTheme("light");

  expect(seen).toEqual(["dark"]);
});
