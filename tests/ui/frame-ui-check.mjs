// 本次组件抽取的页面基准与 Frame 集成回归。只用演示 IPC，不访问模型或游戏。
// 资源：target/.tmp/frame-integration；finally 关闭本脚本的服务器和浏览器。
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { checkFrame } from "./frame-preview-check.mjs";
import { collectProductSamples } from "./frame-product-samples.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const scratch = path.join(root, "target/.tmp/frame-integration");
const baseline = process.argv.includes("--baseline");
const production = process.argv.includes("--production");
const frameOnly = process.argv.includes("--frame-only");
const captureDirectory = baseline
  ? "before"
  : production
    ? "production"
    : "after";
fs.mkdirSync(scratch, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = scratch;
const { createServer, preview } = await import("vite");
const { chromium } = await import("playwright");
const { PNG } = createRequire(import.meta.url)(
  "playwright-core/lib/utilsBundle",
);
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json")),
).version;
const task = {
  id: "demo",
  name: "采集路线",
  description: "运行已配置的采集路线",
  state: "readyUnverified",
  stateLabel: "可运行 · 尚未实机验证",
  actionLabel: "运行",
  runnable: true,
  pinned: true,
  sourceTitleSnapshot: "路线配置",
  sourceDeleted: false,
  sourceConversationId: "demo-chat",
  publishedRevision: 1,
  modelUsage: "none",
  zeroToken: true,
  nodeCount: 1,
  updatedAt: "2026-10-01T00:00:00Z",
  shortcut: {
    applicationName: "BetterGI",
    targetName: "采集路线组",
    prepare: [],
    action: { tool: "demo.run", arguments: {} },
  },
};
const bootstrap = {
  configPath: "E:\\Sleepy Doll\\user\\config.json",
  models: [
    {
      id: "demo-model",
      name: "演示模型",
      model: "demo",
      baseUrl: "https://demo.invalid/v1",
      protocol: "openai-chat",
      active: true,
    },
  ],
  skills: [
    {
      name: "create-shortcut",
      description: "将已有配置保存为快捷任务。",
      source: "product",
      tags: [],
      enabled: true,
      available: true,
      instructions: "演示技能说明。",
    },
  ],
  tools: [],
  runtimeToolLabels: {},
  plugins: [],
  conversations: [],
  tasks: [],
  strategies: [],
  workflows: [task],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  conversationGroups: { groups: [], membership: {}, order: [] },
  permission: { mode: "fullAccess", label: "允许执行", levels: [] },
  bridge: { enabled: false, connected: false, baseUrl: "demo" },
};
let server, browser, crossBrowser;
const captures = [];

try {
  if (frameOnly) {
    browser = await chromium.launch({
      headless: true,
      args: ["--disable-lcd-text", "--force-color-profile=srgb"],
    });
    const saved = await collectProductSamples(
      browser,
      "http://127.0.0.1:5173",
      bootstrap,
      version,
    );
    await checkFrame({
      browser,
      root,
      scratch,
      components: saved,
      PNG,
    });
  } else {
    server = production
      ? await preview({
          configFile: path.join(root, "vite.config.ts"),
          logLevel: "error",
          preview: { host: "127.0.0.1", port: 0 },
        })
      : await createServer({
          configFile: path.join(root, "vite.config.ts"),
          logLevel: "error",
          server: {
            host: "127.0.0.1",
            port: 0,
            hmr: false,
            forwardConsole: false,
          },
        });
    if (!production) await server.listen();
    const appUrl = `http://127.0.0.1:${server.httpServer.address().port}`;
    browser = await chromium.launch({ headless: true });
    for (const width of [1400, 1000])
      for (const theme of ["light", "dark"]) {
        const context = await browser.newContext({
          viewport: { width, height: 900 },
          locale: "zh-CN",
        });
        const page = await context.newPage();
        const errors = [];
        page.on("pageerror", (e) => errors.push(e.message));
        await context.route(
          (url) => !["127.0.0.1", "localhost"].includes(url.hostname),
          (route) => route.fulfill({ body: "", contentType: "text/plain" }),
        );
        await page.route("**/ipc", async (route) => {
          const q = route.request().postDataJSON();
          let result = {};
          if (q.method === "bootstrap") result = bootstrap;
          if (q.method === "memory.list") result = { entries: [] };
          if (q.method === "events.read") result = { events: [] };
          if (q.method === "task.list") result = [];
          if (q.method === "config.read")
            result = {
              path: bootstrap.configPath,
              content: '{\n  "models": []\n}',
            };
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
          },
          { version, theme },
        );
        const capture = async (name) => {
          await page.mouse.move(width - 2, 2);
          await page.waitForTimeout(650);
          const filename = `${width}-${theme}-${name}.png`;
          const output = path.join(scratch, captureDirectory, filename);
          fs.mkdirSync(path.dirname(output), { recursive: true });
          await page.screenshot({
            path: output,
            animations: "disabled",
            caret: "hide",
          });
          captures.push(filename);
        };
        await page.goto(appUrl);
        await page.locator(".app-nav").waitFor();
        await capture("chat");
        await page.locator(".app-account").click();
        await capture("account");
        await page
          .locator(".app-account-menu")
          .getByRole("menuitem", { name: "设置", exact: true })
          .click();
        await page.locator(".settings-general").waitFor();
        await capture("settings");
        await page
          .locator(".settings-general [data-ui='select-trigger']")
          .first()
          .click();
        await capture("settings-select");
        await page.keyboard.press("Escape");
        await page
          .getByRole("button", { name: "编辑配置", exact: true })
          .click();
        await page.locator(".sd-dialog").waitFor();
        await capture("dialog");
        await page.keyboard.press("Escape");
        await page.evaluate(() =>
          window.dispatchEvent(new CustomEvent("sleepy-doll:open-models")),
        );
        await page.locator(".model-settings").waitFor();
        await capture("models");
        await page
          .getByRole("button", { name: "添加模型", exact: true })
          .click();
        await page.locator(".model-form").waitFor();
        await capture("model-form");
        await page
          .locator(".model-form [data-ui='select-trigger']")
          .first()
          .click();
        await capture("model-select");
        await page.keyboard.press("Escape");
        await page
          .locator(".app-nav")
          .getByRole("button", { name: "工具与扩展", exact: true })
          .click();
        await page.locator(".extensions-page").waitFor();
        await capture("extensions");
        await page.locator(".extension-summary").click();
        await page.locator("dialog[open]").waitFor();
        await capture("extension-dialog");
        await page.keyboard.press("Escape");
        await page
          .locator(".app-nav")
          .getByRole("button", { name: "快捷任务", exact: true })
          .click();
        await page.locator(".task-card").waitFor();
        await capture("tasks");
        await page.locator(".task-card-menu > button").click();
        await capture("task-menu");
        await page
          .getByRole("menuitem", { name: "重命名", exact: true })
          .click();
        await page.locator(".sd-dialog").waitFor();
        await capture("task-rename");
        await page.keyboard.press("Escape");
        await page.getByRole("button", { name: "已归档", exact: true }).click();
        await capture("empty-tasks");
        assert.deepEqual(errors, [], `${width}/${theme} 页面运行错误`);
        await context.close();
      }
    fs.writeFileSync(
      path.join(scratch, baseline ? "baseline.json" : "pages.json"),
      JSON.stringify(captures, null, 2),
    );
    if (!baseline) {
      const comparisons = [];
      for (const filename of captures) {
        const before = path.join(scratch, "before", filename);
        if (!fs.existsSync(before)) continue;
        const a = PNG.sync.read(fs.readFileSync(before));
        const b = PNG.sync.read(
          fs.readFileSync(path.join(scratch, captureDirectory, filename)),
        );
        assert.equal(a.width, b.width);
        assert.equal(a.height, b.height);
        let changed = 0,
          maxChannelDelta = 0;
        for (let i = 0; i < a.data.length; i += 4) {
          const delta = Math.max(
            ...[0, 1, 2, 3].map((c) => Math.abs(a.data[i + c] - b.data[i + c])),
          );
          if (delta) changed++;
          maxChannelDelta = Math.max(maxChannelDelta, delta);
        }
        comparisons.push({ filename, changed, maxChannelDelta });
        assert.ok(
          changed <= 64 && maxChannelDelta <= 1,
          `${filename} 外观发生变化：${changed} pixels / ${maxChannelDelta} levels`,
        );
      }
      fs.writeFileSync(
        path.join(
          scratch,
          production ? "comparison-production.json" : "comparison.json",
        ),
        JSON.stringify(comparisons, null, 2),
      );
      console.log(
        JSON.stringify({
          phase: production ? "visual-production" : "visual",
          checked: comparisons.length,
          identical: comparisons.filter((c) => !c.changed).length,
          maxChannelDelta: Math.max(
            0,
            ...comparisons.map((c) => c.maxChannelDelta),
          ),
        }),
      );
      if (!production) {
        crossBrowser = await chromium.launch({
          headless: true,
          args: ["--disable-lcd-text", "--force-color-profile=srgb"],
        });
        const sharedSamples = await collectProductSamples(
          crossBrowser,
          appUrl,
          bootstrap,
          version,
        );
        await checkFrame({
          browser: crossBrowser,
          root,
          scratch,
          components: sharedSamples,
          PNG,
        });
      }
    }
    console.log(
      JSON.stringify({
        phase: baseline ? "baseline" : "pages",
        captures: captures.length,
        errors: 0,
      }),
    );
  }
} finally {
  await crossBrowser?.close();
  await browser?.close();
  await server?.close();
}
