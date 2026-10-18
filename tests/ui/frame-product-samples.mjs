import assert from "node:assert/strict";
import { inspectElement } from "./frame-preview-check.mjs";

/** 两端组件比较使用统一的灰度文字栅格化；完整页面基准仍使用默认浏览器设置。 */
export async function collectProductSamples(browser, url, bootstrap, version) {
  const samples = [];
  for (const theme of ["light", "dark"]) {
    const context = await browser.newContext({
      viewport: { width: 1400, height: 900 },
      locale: "zh-CN",
    });
    try {
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(e.message));
      await context.route(
        (url) => !["127.0.0.1", "localhost"].includes(url.hostname),
        (route) => route.fulfill({ body: "" }),
      );
      await page.route("**/ipc", async (route) => {
        const q = route.request().postDataJSON();
        let result = {};
        if (q.method === "bootstrap") result = bootstrap;
        if (q.method === "memory.list") result = { entries: [] };
        if (q.method === "events.read") result = { events: [] };
        if (q.method === "task.list") result = [];
        await route.fulfill({
          contentType: "application/json",
          body: JSON.stringify({ id: q.id, ok: true, result }),
        });
      });
      await page.addInitScript(
        ({ theme, version }) => {
          localStorage.setItem("sleepy-doll-version", version);
          localStorage.setItem("sleepy-doll-theme", theme);
          localStorage.setItem("sleepy-doll-locale", "zh");
        },
        { theme, version },
      );
      const sample = async (id, selector) => {
        await page.mouse.move(1398, 2);
        await page.waitForTimeout(500);
        const element = page.locator(selector).first();
        const box = await element.boundingBox();
        const image = await element.screenshot({
          animations: "disabled",
          caret: "hide",
        });
        const { styles, descendants, texts } =
          await element.evaluate(inspectElement);
        samples.push({ id, theme, ...box, image, styles, descendants, texts });
      };
      await page.goto(url);
      await page.locator(".app-nav").waitFor();
      await sample("composer-field", ".sd-composer-field");
      await page.locator(".app-account").click();
      await page
        .locator(".app-account-menu")
        .getByRole("menuitem", { name: "设置", exact: true })
        .click();
      await page.locator(".settings-general").waitFor();
      await sample("theme-switch", ".settings-general .app-theme-switch");
      await page
        .locator(".app-nav")
        .getByRole("button", { name: "工具与扩展", exact: true })
        .click();
      await page.locator(".extension-row").waitFor();
      await sample("extension-row", ".extension-row");
      await page
        .locator(".app-nav")
        .getByRole("button", { name: "快捷任务", exact: true })
        .click();
      await page.locator(".task-card").waitFor();
      await sample("task-card", ".task-card");
      assert.deepEqual(errors, []);
    } finally {
      await context.close();
    }
  }
  return samples;
}
