import "./runtime.mjs";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = fileURLToPath(new URL("../../", import.meta.url));
const captures = path.join(root, "target/.tmp/update-dialog");
fs.mkdirSync(captures, { recursive: true });
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
).version;
let server, browser;
try {
  server = await createServer({
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  });
  for (const narrow of [false, true]) {
    const context = await browser.newContext({
      viewport: narrow
        ? { width: 390, height: 740 }
        : { width: 1280, height: 900 },
    });
    const page = await context.newPage();
    page.setDefaultTimeout(7000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    let downloaded = false,
      downloads = 0,
      installs = 0,
      failDownload = true;
    const state = () => ({
      currentVersion: version,
      currentChannel: "stable",
      channel: "stable",
      analyticsEnabled: false,
      downloaded,
      release: {
        version: narrow ? version : "9.9.9",
        channel: "stable",
        url: "https://fixture.invalid/setup.exe",
        size: 8388608,
        sha256: "0".repeat(64),
        publishedAt: "2026-10-19T00:00:00Z",
        notes:
          "## 本次修复\n\n" +
          Array.from(
            { length: 45 },
            (_, i) => `- 修复项目 ${i + 1}：保留配置，校验下载，避免重复执行。`,
          ).join("\n"),
      },
    });
    await page.addInitScript((narrow) => {
      localStorage.setItem("sleepy-doll-version", "previous");
      localStorage.setItem("sleepy-doll-theme", narrow ? "dark" : "light");
      localStorage.setItem("sleepy-doll-locale", "zh");
      window.__SLEEPY_DOLL_DESKTOP__ = true;
    }, narrow);
    await page.route("**/ipc", async (route) => {
      const q = route.request().postDataJSON();
      let result = {};
      if (q.method === "bootstrap")
        result = {
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
          permission: { mode: "fullAccess", label: "允许执行", levels: [] },
          bridge: { enabled: false, connected: false, baseUrl: "fixture" },
        };
      if (q.method === "release.check" || q.method === "release.state")
        result = state();
      if (q.method === "release.download") {
        downloads++;
        await new Promise((resolve) => setTimeout(resolve, 180));
        if (failDownload) {
          failDownload = false;
          await route.fulfill({
            contentType: "application/json",
            body: JSON.stringify({
              id: q.id,
              ok: false,
              error: { message: "网络暂不可用，请重试" },
            }),
          });
          return;
        }
        downloaded = true;
        result = state();
      }
      if (q.method === "release.install") {
        installs++;
        result = { installing: true };
      }
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({ id: q.id, ok: true, result }),
      });
    });
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
    const notes = page
      .getByRole("dialog")
      .filter({ has: page.locator(".update-dialog") });
    await notes.waitFor();
    await page.waitForTimeout(350);
    assert.equal(
      await page.locator('.sd-dialog-layer[data-visible="true"]').count(),
      1,
    );
    await page.screenshot({
      path: path.join(captures, narrow ? "notes-narrow.png" : "notes-wide.png"),
    });
    await notes.getByRole("button", { name: "开始使用", exact: true }).click();
    const update = page.locator(".release-update-dialog");
    await update.waitFor();
    await page.waitForTimeout(350);
    assert.equal(
      await page.getByRole("dialog").count(),
      1,
      "更新弹窗不应叠在说明弹窗上",
    );
    assert.ok(
      (await update.innerText()).includes(
        narrow ? "当前版本有修订" : "发现新版本",
      ),
    );
    assert.equal(await update.locator(".release-notes li").count(), 45);
    const checkLayout = async () => {
      const geometry = await update.evaluate((dialog) => {
        const box = dialog.getBoundingClientRect();
        const action = dialog
          .querySelector(".release-action-buttons")
          .getBoundingClientRect();
        const notes = dialog.querySelector(".release-notes");
        return {
          left: box.left,
          right: box.right,
          bottom: box.bottom,
          actionBottom: action.bottom,
          width: innerWidth,
          height: innerHeight,
          scrollable: notes.scrollHeight > notes.clientHeight,
          overflow: dialog.scrollWidth > dialog.clientWidth,
        };
      });
      assert.ok(
        geometry.left >= 0 &&
          geometry.right <= geometry.width &&
          geometry.bottom <= geometry.height,
        JSON.stringify(geometry),
      );
      assert.ok(
        geometry.actionBottom <= geometry.height &&
          geometry.scrollable &&
          !geometry.overflow,
        JSON.stringify(geometry),
      );
    };
    await checkLayout();
    await page.screenshot({
      path: path.join(
        captures,
        narrow ? "update-narrow.png" : "update-wide.png",
      ),
    });
    await update.getByRole("button", { name: "下载更新", exact: true }).click();
    await update.getByRole("alert").waitFor();
    await update.getByRole("button", { name: "下载更新", exact: true }).click();
    const install = update.getByRole("button", {
      name: "安装并重启",
      exact: true,
    });
    await install.waitFor();
    assert.equal(downloads, 2);
    assert.ok(
      (await update.getByRole("status").innerText()).includes("校验已通过"),
    );
    await install.click();
    await page.waitForTimeout(250);
    assert.equal(installs, 1);
    assert.equal(
      await update.locator(".primary-action").isDisabled(),
      true,
      "已接受安装后保持禁用，防止重复提交",
    );
    assert.deepEqual(errors, []);
    await context.close();
  }
  console.log(
    JSON.stringify({
      wideAndNarrow: true,
      singleDialog: true,
      sameVersionRevision: true,
      markdownNotes: true,
      downloadRecovery: true,
      installOnce: true,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
