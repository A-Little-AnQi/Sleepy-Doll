// Styled tray menu only; native actions are captured, never executed.
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
let server, browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0 },
  });
  await server.listen();
  const port = server.httpServer.address().port;
  console.log(
    JSON.stringify({ port, cleanup: "finally closes browser and Vite" }),
  );
  browser = await chromium.launch({ headless: true });
  for (const dark of [true, false]) {
    const page = await browser.newPage({
      viewport: { width: 288, height: 600 },
    });
    page.setDefaultTimeout(5000);
    await page.addInitScript(
      ({ dark }) => {
        window.__TRAY_STATE__ = { dark, bridge: true, active: true };
        window.sentActions = [];
        window.ipc = {
          postMessage(value) {
            window.sentActions.push(JSON.parse(value));
          },
        };
        Object.defineProperty(document.fonts, "ready", {
          configurable: true,
          value: new Promise((resolve) => (window.releaseFonts = resolve)),
        });
      },
      { dark },
    );
    await page.goto(`http://127.0.0.1:${port}/tray.html`);
    const menu = page.getByRole("menu", { name: "Sleepy Doll" });
    await menu.waitFor();
    await page.waitForTimeout(100);
    assert.equal(
      await page.evaluate(() => window.sentActions.length),
      0,
      "menu ready before fonts/first frame",
    );
    await page.evaluate(() => window.releaseFonts());
    await page.waitForFunction(() =>
      window.sentActions.some((item) => item.action === "ready"),
    );
    const geometry = await menu.evaluate((node) => {
      const style = getComputedStyle(node),
        rect = node.getBoundingClientRect();
      const quit = node
        .querySelector("button:last-child")
        .getBoundingClientRect();
      return {
        border: style.borderTopWidth,
        radius: style.borderRadius,
        height: rect.height,
        bottom: rect.bottom,
        quitBottom: quit.bottom,
        background: style.backgroundColor,
      };
    });
    assert.equal(geometry.border, "0px");
    assert.equal(geometry.radius, "0px");
    assert(
      geometry.quitBottom <= geometry.bottom,
      "last item outside measured popup",
    );
    assert.equal(
      geometry.background,
      dark ? "rgb(27, 27, 27)" : "rgb(255, 255, 255)",
    );
    await page.evaluate(() =>
      window.showMenu({ dark: true, bridge: true, active: true }, 11),
    );
    await page.evaluate(() =>
      window.showMenu({ dark: false, bridge: true, active: true }, 12),
    );
    await page.waitForFunction(() =>
      window.sentActions.some(
        (item) => item.action === "shown" && item.generation === 12,
      ),
    );
    const frames = await page.evaluate(() =>
      window.sentActions.filter((item) => item.action === "shown"),
    );
    assert(frames.every((item) => item.height === Math.ceil(geometry.height)));
    await page
      .getByRole("menuitem", { name: "打开主窗口", exact: true })
      .click();
    await page.getByRole("menuitem", { name: "设置", exact: true }).click();
    await page
      .getByRole("menuitemcheckbox", { name: "BetterGI 连接", exact: true })
      .click();
    await page.getByRole("menuitem", { name: /停止所有任务/ }).click();
    await page
      .getByRole("menuitem", { name: "停止任务并退出", exact: true })
      .click();
    const actions = await page.evaluate(() =>
      window.sentActions.map((item) => item.action),
    );
    for (const action of ["open", "settings", "bridge", "stop", "quit"])
      assert(actions.includes(action), action);
    await page.evaluate(() =>
      window.updateMenu({
        dark: true,
        bridge: false,
        bridgeBusy: true,
        active: false,
      }),
    );
    assert.equal(await page.getByRole("menuitemcheckbox").isDisabled(), true);
    assert.equal(
      await page.getByRole("menuitem", { name: /停止所有任务/ }).isDisabled(),
      true,
    );
    await page.evaluate(() =>
      window.updateMenu({ dark: true, bridge: true, active: true }),
    );
    await page
      .getByRole("menuitem", { name: "打开主窗口", exact: true })
      .focus();
    await page.keyboard.press("ArrowDown");
    assert.equal(
      await page.evaluate(() => document.activeElement.textContent.trim()),
      "设置",
    );
    assert.equal(
      await page.evaluate(
        () => getComputedStyle(document.activeElement).outlineStyle,
      ),
      "none",
      "global focus outline duplicated menu border",
    );
    await page.keyboard.press("Escape");
    assert.equal(
      await page.evaluate(() => window.sentActions.at(-1).action),
      "dismiss",
    );
    await page.evaluate(() => document.activeElement.blur());
    await page.getByRole("menuitem", { name: "设置", exact: true }).hover();
    if (dark && process.env.TRAY_STYLE_SCREENSHOT_PATH)
      await menu.screenshot({ path: process.env.TRAY_STYLE_SCREENSHOT_PATH });
    console.log(
      JSON.stringify({
        dark,
        measuredHeight: geometry.height,
        singleOuterFrame: true,
        actionsAndKeyboard: true,
      }),
    );
    await page.close();
  }
} finally {
  await browser?.close();
  await server?.close();
}
