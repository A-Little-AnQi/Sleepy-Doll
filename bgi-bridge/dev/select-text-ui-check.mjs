// Select 文本垂直裁切回归：真实 App/SettingsPage/ModelsPage + fake IPC，
// 单流程：选智谱预设 -> 假密钥 -> 获取模型 -> 检查 trigger/menu 文字墨迹
// （Canvas actualBoundingBoxAscent/Descent + DOM 行盒）上下留空，
// 同时覆盖菜单打开/键盘选择/长文本横向省略，及 dsf 1/1.25/1.5 × light/dark。
// 资源登记：本地 vite 服务、Chromium、截图与 TEMP/TMP 固定在
// target/.tmp/select-text，属本任务；finally 关闭 browser/server，截图留查。
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const tempRoot = path.join(root, "target", ".tmp", "select-text");
fs.mkdirSync(tempRoot, { recursive: true });
process.env.TEMP = tempRoot;
process.env.TMP = tempRoot;
const appVersion = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
).version;

const FAKE_MODELS = [
  "glm-4.5",
  "glm-4.5-air",
  "glm-5.3",
  "glm-4.7",
  "gypq清言推理增强版超长模型名称条目" + "一二三四五六七八九十".repeat(6),
];

let server, browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  const port = server.httpServer.address().port;
  browser = await chromium.launch({ headless: true });

  const results = [];
  for (const dsf of [1, 1.25, 1.5]) {
    for (const theme of ["light", "dark"]) {
      const context = await browser.newContext({
        viewport: { width: 1400, height: 900 },
        deviceScaleFactor: dsf,
      });
      const page = await context.newPage();
      page.setDefaultTimeout(8000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
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
        if (q.method === "model.list") result = { models: FAKE_MODELS };
        await route.fulfill({
          contentType: "application/json",
          body: JSON.stringify({ id: q.id, ok: true, result }),
        });
      });
      await page.addInitScript(
        ({ version, theme }) => {
          localStorage.setItem("sleepy-doll-version", version);
          localStorage.setItem("sleepy-doll-locale", "zh");
          localStorage.setItem("sleepy-doll-theme", theme);
          localStorage.setItem("sleepy-doll-sidebar-collapsed", "true");
        },
        { version: appVersion, theme },
      );
      await page.goto(`http://127.0.0.1:${port}`);
      await page.locator(".app-nav").waitFor();
      await page.evaluate(() =>
        window.dispatchEvent(new CustomEvent("sleepy-doll:open-models")),
      );
      await page.locator(".model-settings").waitFor();
      await page.getByRole("button", { name: "添加模型", exact: true }).click();
      await page.locator(".model-form").waitFor();
      await page
        .locator(".model-form [data-ui='select-trigger']")
        .first()
        .click();
      await page
        .getByRole("option", { name: "智谱 GLM", exact: true })
        .click();
      await page.waitForTimeout(150);
      // 假密钥只为过表单必填校验；model.list 由 fake IPC 返回，不出网。
      await page.locator(".key-field input").fill("fixture-key");
      await page.getByRole("button", { name: "获取模型", exact: true }).click();
      const modelSelect = page.locator(".model-pick [data-ui='select-trigger']");
      await modelSelect.waitFor();
      assert.equal(
        (await modelSelect.locator("span").textContent()).trim(),
        "glm-4.5",
        "获取列表后应默认选中第一个模型",
      );

      // 墨迹测量：Canvas 上下伸 + DOM 文本行盒 vs 元素盒，均要求有正余量。
      const measure = (selector) =>
        page.locator(selector).first().evaluate((el) => {
          const cs = getComputedStyle(el);
          const ctx = document.createElement("canvas").getContext("2d");
          ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
          const m = ctx.measureText(el.textContent);
          const range = document.createRange();
          range.selectNodeContents(el);
          const text = range.getBoundingClientRect();
          const box = el.getBoundingClientRect();
          return {
            canvasInk: m.actualBoundingBoxAscent + m.actualBoundingBoxDescent,
            boxHeight: box.height,
            marginTop: text.top - box.top,
            marginBottom: box.bottom - text.bottom,
            overflowX: el.scrollWidth - el.clientWidth,
            textOverflow: cs.textOverflow,
            lineHeight: cs.lineHeight,
            fontSizePx: parseFloat(cs.fontSize),
          };
        });
      const checkVertical = (info, label) => {
        assert.ok(
          info.boxHeight + 0.5 >= info.canvasInk,
          `${label} 元素盒高 ${info.boxHeight} 装不下墨迹 ${info.canvasInk}`,
        );
        assert.ok(
          info.marginTop >= -0.5 && info.marginBottom >= -0.5,
          `${label} 文本行盒被裁：上余量 ${info.marginTop} 下余量 ${info.marginBottom}`,
        );
        return info;
      };
      const triggerInfo = checkVertical(
        await measure(".model-pick [data-ui='select-trigger'] > span"),
        `dsf${dsf}/${theme} trigger`,
      );
      assert.equal(triggerInfo.textOverflow, "ellipsis", "trigger 保留横向省略");

      // 打开菜单：选项文字同样不裁（strong 承载模型名）。
      await modelSelect.click();
      await page.locator("[data-ui='select-menu']").waitFor();
      // 弹层有淡入动画，等它结束再测量与截图，避免半透明帧。
      await page.waitForTimeout(350);
      const optionInfo = checkVertical(
        await measure(
          "[data-ui='select-menu'] [data-ui='select-option']:first-child strong",
        ),
        `dsf${dsf}/${theme} menu option`,
      );
      await page.screenshot({
        path: path.join(tempRoot, `menu-dsf${dsf}-${theme}.png`),
      });

      // 键盘选择：ArrowDown x3 -> Enter 应选中 glm-4.7。
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      assert.equal(
        (await modelSelect.locator("span").textContent()).trim(),
        "glm-4.7",
        "键盘选择应选中 glm-4.7",
      );

      // 长中文模型名：横向省略生效，垂直仍不裁。
      await modelSelect.click();
      await page
        .locator("[data-ui='select-menu'] [data-ui='select-option']")
        .nth(4)
        .click();
      const longInfo = checkVertical(
        await measure(".model-pick [data-ui='select-trigger'] > span"),
        `dsf${dsf}/${theme} long-name trigger`,
      );
      assert.ok(
        longInfo.overflowX > 0,
        "超长模型名应触发横向省略（scrollWidth 超出）",
      );
      await modelSelect.screenshot({
        path: path.join(tempRoot, `trigger-long-dsf${dsf}-${theme}.png`),
      });

      assert.deepEqual(errors, [], `dsf${dsf}/${theme} 出现页面错误`);
      results.push({
        dsf,
        theme,
        triggerLineHeight: triggerInfo.lineHeight,
        triggerBoxVsInk: +(triggerInfo.boxHeight - triggerInfo.canvasInk).toFixed(2),
        optionMarginBottom: +optionInfo.marginBottom.toFixed(2),
        longNameOverflowX: longInfo.overflowX,
        keyboardPick: "glm-4.7",
      });
      await context.close();
    }
  }
  console.log(JSON.stringify({ results, screenshots: tempRoot }, null, 2));
} finally {
  await browser?.close();
  await server?.close();
}
