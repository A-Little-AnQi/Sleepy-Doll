// Run after npm run build. Uses built UI and fake IPC; never installs or updates files.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ui = fileURLToPath(new URL("../../target/ui/", import.meta.url));
const baseInfo = {
  version: "0.2.0",
  directory: "E:\\Apps\\Custom Agent\\",
  defaultDirectory: "D:\\Sleepy Doll",
  installed: true,
  installedVersion: "0.1.0",
  uninstallMode: false,
};
const browser = await chromium.launch({ headless: true });
try {
  for (const scenario of [
    { name: "fresh", info: { installed: false }, method: "setup.install" },
    { name: "update", info: {}, method: "setup.update" },
    {
      name: "same version",
      info: { installedVersion: "0.2.0" },
      method: "setup.update",
    },
    {
      name: "unknown version",
      info: { installedVersion: null },
      method: "setup.update",
    },
    { name: "English", info: {}, locale: "en", method: "setup.update" },
    {
      name: "uninstall",
      info: { uninstallMode: true },
      method: "setup.uninstall",
    },
    { name: "info delayed", info: {}, delayed: true, method: "setup.update" },
    { name: "info failed", info: {}, failed: true },
  ]) {
    const info = { ...baseInfo, ...scenario.info };
    const context = await browser.newContext({
      viewport: { width: 720, height: 460 },
    });
    try {
      const page = await context.newPage();
      page.setDefaultTimeout(5000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.route("http://127.0.0.1/**", async (route) => {
        const requested = new URL(route.request().url()).pathname;
        const file = path.join(
          ui,
          requested === "/" ? "setup.html" : requested,
        );
        const contentType =
          {
            ".html": "text/html",
            ".js": "application/javascript",
            ".css": "text/css",
            ".webp": "image/webp",
            ".jpg": "image/jpeg",
          }[path.extname(file)] ?? "application/octet-stream";
        await route.fulfill({ body: await readFile(file), contentType });
      });
      await page.addInitScript(
        ({ info, scenario }) => {
          localStorage.setItem("sleepy-doll-locale", scenario.locale ?? "zh");
          window.__SLEEPY_DOLL_DESKTOP__ = true;
          window.__SLEEPY_DOLL_FRAMELESS__ = true;
          window.setupRequests = [];
          window.ipc = {
            postMessage(message) {
              const request = JSON.parse(message);
              if (!request.method.startsWith("setup.")) return;
              window.setupRequests.push(request);
              const reply = () =>
                window.__setupReceive?.({
                  id: request.id,
                  result: request.method === "setup.info" ? info : {},
                  error: scenario.failed
                    ? { message: "fixture info unavailable" }
                    : null,
                });
              if (request.method === "setup.info" && scenario.delayed) {
                window.releaseSetupInfo = reply;
              } else {
                queueMicrotask(reply);
              }
            },
          };
        },
        { info, scenario },
      );
      await page.goto("http://127.0.0.1/setup.html");
      const primary = page.locator(".setup-foot .primary-action");
      if (scenario.delayed) {
        await page.waitForFunction(
          () => typeof window.releaseSetupInfo === "function",
        );
        assert.equal(await primary.isDisabled(), true);
        assert.equal(await page.locator("#setup-directory").isDisabled(), true);
        await page.evaluate(() => window.releaseSetupInfo());
      }
      if (scenario.failed) {
        await page.getByText("fixture info unavailable").waitFor();
        assert.equal(await primary.isDisabled(), true);
        assert.equal(await page.locator("#setup-directory").isDisabled(), true);
        assert.equal(
          await page.evaluate(() =>
            window.setupRequests.some((r) => r.method !== "setup.info"),
          ),
          false,
        );
        console.log(`PASS ${scenario.name}`);
        continue;
      }
      const action = info.uninstallMode
        ? "卸载"
        : info.installed
          ? scenario.locale === "en"
            ? "Update"
            : "更新"
          : "安装";
      await page
        .getByRole("heading", { name: `${action} Sleepy Doll`, exact: true })
        .waitFor();
      await page.waitForFunction(
        () => !document.querySelector(".setup-foot .primary-action").disabled,
      );
      assert.equal(await primary.textContent(), action);
      if (info.installed) {
        assert.equal(await page.locator("#setup-directory").count(), 0);
        assert.equal(
          await page.getByRole("button", { name: "浏览", exact: true }).count(),
          0,
        );
        assert.equal(
          await page.getByText("创建桌面快捷方式", { exact: true }).count(),
          0,
        );
        assert.equal(
          await page.locator(".setup-meta .setup-mono").textContent(),
          info.directory,
        );
        assert.equal(
          await page.locator(".setup-meta-row").count(),
          info.uninstallMode ? 2 : 3,
        );
      } else {
        assert.equal(
          await page.locator("#setup-directory").inputValue(),
          info.defaultDirectory,
        );
        await page.locator("#setup-directory").fill("E:\\Apps");
        await page.getByLabel("创建桌面快捷方式").uncheck();
      }
      const bounds = await primary.boundingBox();
      assert.ok(
        bounds && bounds.y + bounds.height <= 460,
        "action must fit the fixed window",
      );
      await primary.click();
      await page.waitForFunction(
        (method) => window.setupRequests.some((r) => r.method === method),
        scenario.method,
      );
      const request = await page.evaluate(
        (method) => window.setupRequests.find((r) => r.method === method),
        scenario.method,
      );
      assert.deepEqual(
        request.params,
        info.uninstallMode
          ? { removeUserData: false }
          : info.installed
            ? {}
            : { directory: "E:\\Apps", desktopShortcut: false },
      );
      await page.evaluate(() =>
        window.__setupState({
          phase: "failed",
          progress: 0,
          message: "",
          error: "fixture write failed",
        }),
      );
      await page
        .getByRole("heading", { name: `${action}失败`, exact: true })
        .waitFor();
      await page.getByRole("button", { name: "返回重试", exact: true }).click();
      assert.equal(await primary.textContent(), action);
      await primary.click();
      await page.evaluate(() =>
        window.__setupState({
          phase: "done",
          progress: 1,
          message: "",
          error: null,
        }),
      );
      await page
        .getByRole("heading", { name: `${action}完成`, exact: true })
        .waitFor();
      assert.equal(
        await page.locator(".setup-target").textContent(),
        info.installed ? info.directory : "E:\\Apps\\Sleepy Doll",
      );
      assert.deepEqual(errors, []);
      console.log(`PASS ${scenario.name}`);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
