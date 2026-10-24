import "./runtime.mjs";
// Main entry with fake native IPC; no Agent, BGI or user files.
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
let server;
let browser;
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
    JSON.stringify({ port, cleanup: "finally closes Chromium and Vite" }),
  );
  browser = await chromium.launch({
    headless: true,
    channel: process.env.PLAYWRIGHT_CHANNEL || undefined,
  });
  for (const entry of ["/"]) {
    for (const dark of [true, false]) {
      const page = await browser.newPage({
        viewport: { width: 1100, height: 800 },
      });
      page.setDefaultTimeout(5000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.addInitScript(
        ({ dark }) => {
          localStorage.setItem("sleepy-doll-theme", dark ? "dark" : "light");
          localStorage.setItem("sleepy-doll-locale", "zh");
          localStorage.setItem("sleepy-doll-version", "0.1.0");
          window.__SLEEPY_DOLL_DESKTOP__ = true;
          window.__SLEEPY_DOLL_FRAMELESS__ = true;
          window.readyMessages = [];
          Object.defineProperty(document.fonts, "ready", {
            configurable: true,
            value: new Promise((resolve) => {
              window.releaseFonts = resolve;
            }),
          });
          const bootstrap = {
            configPath: "fixture",
            models: [],
            skills: [],
            tools: [],
            runtimeToolLabels: {},
            plugins: [],
            conversations: [],
            tasks: [],
            strategies: [],
            workflows: [],
            operations: [],
            resources: [],
            diagnostics: [],
            notifications: [],
            conversationGroups: { groups: [], membership: {}, order: [] },
            permission: {
              mode: "fullAccess",
              label: "允许执行",
              levels: [
                {
                  value: "fullAccess",
                  label: "允许执行",
                  description: "fixture",
                },
              ],
            },
            bridge: {
              enabled: false,
              connected: false,
              baseUrl: "fixture",
              launchSilently: false,
            },
          };
          window.ipc = {
            postMessage(message) {
              const request = JSON.parse(message);
              if (request.method === "window.ready") {
                window.readyMessages.push({
                  dark: request.params.dark,
                  color: getComputedStyle(document.documentElement)
                    .backgroundColor,
                  hasContent:
                    document.getElementById("root").childElementCount > 0,
                });
                return;
              }
              if (request.method.startsWith("window.")) return;
              let result = {};
              if (request.method === "bootstrap") result = bootstrap;
              if (request.method === "bridge.status") result = bootstrap.bridge;
              if (request.method === "tray.state") result = { enabled: true };
              queueMicrotask(() => {
                window.__sleepyDollReceive?.({
                  kind: "response",
                  id: request.id,
                  ok: true,
                  result,
                });
              });
            },
          };
        },
        { dark },
      );
      await page.goto(`http://127.0.0.1:${port}${entry}`);
      await page.waitForFunction(
        () => document.getElementById("root")?.childElementCount > 0,
      );
      await page.waitForTimeout(120);
      assert.equal(
        await page.evaluate(() => window.readyMessages.length),
        0,
        "window exposed before fonts settled",
      );
      await page.evaluate(() => window.releaseFonts());
      await page.waitForFunction(() => window.readyMessages.length === 1);
      const ready = await page.evaluate(() => window.readyMessages[0]);
      assert.equal(ready.dark, dark);
      assert.equal(
        ready.color,
        dark ? "rgb(27, 27, 27)" : "rgb(255, 255, 255)",
      );
      assert.equal(ready.hasContent, true);
      await page.getByRole("dialog").waitFor();
      assert.equal(
        await page.evaluate(
          () =>
            document.elementFromPoint(100, 16)?.closest(".title-bar") != null,
        ),
        true,
        "dialog overlay blocks the window caption",
      );
      assert.equal(
        await page
          .locator('.sd-dialog-layer[data-visible="true"]')
          .evaluate((layer) => layer.getBoundingClientRect().top),
        32,
      );
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "开始使用", exact: true })
        .click();
      await page.getByRole("dialog").waitFor({ state: "hidden" });
      const mascot = page.locator(".welcome-mascot");
      await mascot.waitFor();
      assert.equal(await mascot.evaluate((img) => img.draggable), false);
      assert.deepEqual(
        await mascot.evaluate((img) => {
          const style = getComputedStyle(img);
          const bounds = img.getBoundingClientRect();
          return {
            pointerEvents: style.pointerEvents,
            userSelect: style.userSelect,
            userDrag: style.getPropertyValue("-webkit-user-drag"),
            receivesPointer:
              document.elementFromPoint(
                bounds.x + bounds.width / 2,
                bounds.y + bounds.height / 2,
              ) === img,
            pointerInWelcome:
              document
                .elementFromPoint(
                  bounds.x + bounds.width / 2,
                  bounds.y + bounds.height / 2,
                )
                ?.closest(".chat-welcome") != null,
          };
        }),
        {
          pointerEvents: "none",
          userSelect: "none",
          userDrag: "none",
          receivesPointer: false,
          pointerInWelcome: true,
        },
      );
      assert.equal(
        await page
          .locator(".title-bar")
          .evaluate((bar) =>
            getComputedStyle(bar).getPropertyValue("app-region"),
          ),
        "drag",
      );
      assert.equal(
        await page
          .locator(".title-bar-controls")
          .evaluate((bar) =>
            getComputedStyle(bar).getPropertyValue("app-region"),
          ),
        "no-drag",
      );
      await page.evaluate(() => {
        window.mascotDrags = 0;
        document.addEventListener("dragstart", () => window.mascotDrags++);
      });
      const bounds = await mascot.boundingBox();
      await page.mouse.move(
        bounds.x + bounds.width / 2,
        bounds.y + bounds.height / 2,
      );
      await page.mouse.down();
      await page.mouse.move(
        bounds.x + bounds.width / 2 + 70,
        bounds.y + bounds.height / 2 + 40,
        { steps: 10 },
      );
      await page.mouse.up();
      assert.equal(
        await page.evaluate(() => window.mascotDrags),
        0,
        "welcome image started browser dragging",
      );
      assert.deepEqual(errors, []);
      await page.waitForTimeout(80);
      assert.equal(
        await page.evaluate(() => window.readyMessages.length),
        1,
        "StrictMode or rerender duplicated ready event",
      );
      console.log(JSON.stringify({ entry, dark, styledFirstFrame: true }));
      await page.close();
    }
  }
  const failedPage = await browser.newPage();
  failedPage.setDefaultTimeout(5000);
  await failedPage.addInitScript(() => {
    localStorage.setItem("sleepy-doll-theme", "dark");
    window.failureReady = [];
    window.ipc = {
      postMessage(value) {
        window.failureReady.push(JSON.parse(value));
      },
    };
  });
  await failedPage.route("**/src/main.tsx", (route) => route.abort());
  await failedPage.goto(`http://127.0.0.1:${port}/`);
  await failedPage
    .getByText("界面载入失败：页面资源未能载入", { exact: true })
    .waitFor();
  await failedPage.waitForFunction(() =>
    window.failureReady.some((item) => item.method === "window.ready"),
  );
  assert.equal(
    await failedPage.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    ),
    "rgb(27, 27, 27)",
  );
  await failedPage.close();
  console.log(
    JSON.stringify({
      mainReady: true,
      darkAndLightReady: true,
      noEarlyExposure: true,
      assetFailureShowsErrorInsteadOfHiddenWindow: true,
      realUserDataTouched: false,
      modelRequests: 0,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
