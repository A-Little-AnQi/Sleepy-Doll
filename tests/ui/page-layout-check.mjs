import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const scratch =
  process.env.SLEEPY_DOLL_UI_CAPTURE_DIR ??
  path.join(root, "target/.tmp/ui-readability");
const temporary =
  process.env.SLEEPY_DOLL_UI_TEMP_DIR ??
  path.join(root, "target/tmp/ui-readability");
fs.mkdirSync(scratch, { recursive: true });
fs.mkdirSync(temporary, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = temporary;
const { preview } = await import("vite");
const { chromium } = await import("playwright");
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json")),
).version;
const frontendStamp = () => {
  const hash = createHash("sha256");
  const names = [
    "index.html",
    "tray.html",
    ...fs
      .readdirSync(path.join(root, "target/ui/assets"))
      .sort()
      .filter((name) => /\.(js|css)$/.test(name))
      .map((name) => "assets/" + name),
  ];
  for (const name of names)
    hash
      .update(name)
      .update(fs.readFileSync(path.join(root, "target/ui", name)));
  return hash.digest("hex");
};
const expectedFrontend = frontendStamp();
const theme = process.argv.includes("--dark") ? "dark" : "light";
const native = process.argv.includes("--webview2");
let kind = "single",
  browser,
  server,
  ipc,
  page,
  passed = false;
const errors = [],
  report = [];
const count = () => (kind === "empty" ? 0 : kind === "long" ? 5 : 1);
const text = () =>
  kind === "long"
    ? "较长的配置名称与执行说明，需要能够自然折行并保留字号。".repeat(5)
    : "执行已保存的配置，完成后等待十秒再执行下一项。";
const task = (i) => ({
  id: `shortcut-${i}`,
  name: i ? `任务 ${i}` : "跑千星脚本",
  description: text(),
  state: "readyUnverified",
  stateLabel: "可运行",
  actionLabel: "运行",
  runnable: true,
  pinned: false,
  sourceDeleted: false,
  publishedRevision: 1,
  nodeCount: 3,
  zeroToken: true,
  modelUsage: "none",
  updatedAt: "2026-11-08T00:00:00Z",
  shortcut: {
    applicationName: "BetterGI",
    targetName: "千星配置组",
    steps: [
      { title: "配置一", action: { tool: "fixture.run", arguments: {} } },
      { title: "等待", waitSeconds: 10 },
      { title: "配置二", action: { tool: "fixture.run", arguments: {} } },
    ],
  },
});
const schedules = () =>
  Array.from({ length: count() }, (_, i) => ({
    id: `schedule-${i}`,
    name: i ? `定时任务 ${i}` : "跑千星脚本",
    revision: 1,
    enabled: false,
    registration: "verified",
    trigger: { kind: "daily", localTimes: ["20:00"] },
    entries: [
      {
        entryId: `entry-${i}`,
        title: "跑千星脚本",
        binding: {
          kind: "shortcut",
          shortcutId: `shortcut-${i}`,
          expectedPublishedRevision: 1,
        },
      },
    ],
  }));
const bootstrap = () => ({
  configPath: "fixture",
  models: [
    {
      id: "demo",
      name: "演示模型",
      model: "demo",
      active: true,
      baseUrl: "https://demo.invalid/v1",
      protocol: "openai-chat",
    },
  ],
  skills: Array.from({ length: count() }, (_, i) => ({
    name: i ? `技能 ${i}` : "创建快捷任务",
    description: text(),
    source: "product",
    enabled: true,
    available: true,
  })),
  plugins: [
    {
      manifest: { id: "windows-scheduler", name: "定时任务", version: "0.1.3" },
      status: "enabled",
      configuredEnabled: true,
    },
  ],
  tools: [],
  conversations: [],
  workflows: Array.from({ length: count() }, (_, i) => task(i)),
  tasks: Array.from({ length: count() }, (_, i) => ({
    id: `run-${i}`,
    prompt: "跑千星脚本",
    state: "cancelled",
    createdAt: "2026-11-08T00:00:00Z",
    updatedAt: "2026-11-08T00:00:00Z",
    source: {
      kind: "savedWorkflow",
      workflowId: `shortcut-${i}`,
      workflowRevision: 1,
    },
    conversationId: `shortcut-run-${i}`,
  })),
  strategies: [],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  conversationGroups: { groups: [], membership: {}, order: [] },
  permission: { mode: "fullAccess", label: "允许执行", levels: [] },
  bridge: { enabled: false, connected: false, baseUrl: "fixture" },
});
const memories = () =>
  Array.from({ length: count() }, (_, i) => ({
    compatible: true,
    entry: {
      id: `memory-${i}`,
      revision: 1,
      userEdited: false,
      validationState: "candidate",
      sourceConversationId: "",
      updatedAt: "2026-11-08T00:00:00Z",
      experience: {
        intent: `配置经验 ${i + 1}`,
        summary: text(),
        keywords: ["配置"],
        conditions: [],
        steps: [],
        verification: [],
        pitfalls: [],
        anchors: [],
      },
    },
  }));
const dispatch = (q) => {
  let result = {};
  switch (q.method) {
    case "bootstrap":
      result = bootstrap();
      break;
    case "events.read":
      result = { events: [] };
      break;
    case "task.list":
      result = bootstrap().tasks;
      break;
    case "scheduler.list":
      result = { items: schedules() };
      break;
    case "memory.state":
      result = { enabled: true, revision: kind, reviewing: false };
      break;
    case "memory.list":
      result = { entries: memories() };
      break;
    case "bridge.status":
      result = { connected: false };
      break;
    case "release.state":
      result = {};
      break;
  }
  return { id: q.id, ok: true, result };
};
try {
  server = await preview({
    configFile: path.join(root, "vite.config.ts"),
    configLoader: "runner",
    logLevel: "error",
    preview: { host: "127.0.0.1", port: 0 },
  });
  if (native) {
    ipc = http.createServer(async (req, res) => {
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(dispatch(JSON.parse(Buffer.concat(chunks)))));
    });
    await new Promise((resolve) => ipc.listen(0, "127.0.0.1", resolve));
    fs.writeFileSync(
      path.join(temporary, "native-ipc.port"),
      String(ipc.address().port),
    );
    browser = await chromium.connectOverCDP(
      `http://127.0.0.1:${fs.readFileSync(path.join(temporary, "webview2/cdp.port"), "utf8").trim()}`,
    );
  } else browser = await chromium.launch({ channel: "msedge", headless: true });
  const context = native
    ? browser.contexts()[0]
    : await browser.newContext({ locale: "zh-CN" });
  page = native ? context.pages()[0] : await context.newPage();
  page.setDefaultTimeout(5000);
  page.on("pageerror", (error) => errors.push(error.message));
  await context.route(
    (url) => !["localhost", "127.0.0.1"].includes(url.hostname),
    (route) => route.fulfill({ body: "" }),
  );
  await page.route("**/ipc", (route) =>
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify(dispatch(route.request().postDataJSON())),
    }),
  );
  await page.addInitScript(
    ({ version, theme, native }) => {
      localStorage.setItem("sleepy-doll-version", version);
      localStorage.setItem("sleepy-doll-theme", theme);
      localStorage.setItem("sleepy-doll-locale", "zh");
      window.__SLEEPY_DOLL_DESKTOP__ = true;
      window.__SLEEPY_DOLL_FRAMELESS__ = true;
      if (!native)
        window.ipc = {
          postMessage(message) {
            fetch("/ipc", { method: "POST", body: message })
              .then((r) => r.json())
              .then((value) =>
                window.__sleepyDollReceive({ kind: "response", ...value }),
              );
          },
        };
    },
    { version, theme, native },
  );
  console.log(
    JSON.stringify({
      resource: "page-layout",
      pid: process.pid,
      previewPort: server.httpServer.address().port,
      ipcPort: ipc?.address().port,
    }),
  );
  for (const width of [1456, 1000, 560]) {
    await page.setViewportSize({ width, height: 900 });
    for (kind of ["empty", "single", "long"]) {
      await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
      await page.locator(".app-nav").waitFor({ state: "attached" });
      const geometries = [];
      for (const [name, selector] of [
        ["快捷任务", ".tasks-page"],
        ["定时任务", ".schedules-page"],
        ["记忆", ".memory-page"],
        ["工具与扩展", ".extensions-page"],
      ]) {
        if (
          !(await page
            .locator(".app-nav")
            .getByRole("button", { name, exact: true })
            .isVisible())
        )
          await page
            .getByRole("button", { name: "展开侧栏", exact: true })
            .click();
        await page
          .locator(".app-nav")
          .getByRole("button", { name, exact: true })
          .click();
        if (width < 900) {
          const close = page
            .getByRole("button", { name: "关闭侧栏", exact: true })
            .first();
          if (await close.isVisible()) await close.click();
        }
        const panel = page.locator(selector);
        if (name !== "记忆")
          await panel.locator(".page-list-toolbar .search-field").waitFor();
        if (name === "记忆")
          await panel
            .getByRole("heading", { name: "记忆", exact: true })
            .waitFor();
        await page.waitForTimeout(220);
        const geometry = await panel.evaluate((root) => {
          const title = root.querySelector(".page-title"),
            toolbar = root.querySelector(".page-list-toolbar"),
            input = toolbar?.querySelector(".search-field"),
            tabs = toolbar?.querySelector(".sd-tabs");
          const rect = (n) => {
            if (!n) return null;
            const r = n.getBoundingClientRect();
            return {
              x: r.x,
              y: r.y,
              right: r.right,
              bottom: r.bottom,
              width: r.width,
              height: r.height,
            };
          };
          const fonts = [],
            walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
          while (walker.nextNode()) {
            const node = walker.currentNode,
              el = node.parentElement;
            if (
              !node.textContent.trim() ||
              !el ||
              el.closest(".sr-only,[aria-hidden=true],[inert],svg") ||
              !el.getClientRects().length
            )
              continue;
            const style = getComputedStyle(el);
            if (style.visibility !== "visible" || style.display === "none")
              continue;
            const size = parseFloat(style.fontSize);
            if (size < 14)
              fonts.push({ text: node.textContent.trim().slice(0, 70), size });
          }
          return {
            title: rect(title),
            toolbar: rect(toolbar),
            search: rect(input),
            tabs: tabs ? rect(tabs) : null,
            padding: getComputedStyle(root).padding,
            fonts,
            overflow: root.scrollWidth > root.clientWidth + 1,
          };
        });
        assert.deepEqual(
          geometry.fonts,
          [],
          `${name} has unreadable text: ${JSON.stringify(geometry.fonts)}`,
        );
        assert.equal(geometry.overflow, false, `${name} horizontal overflow`);
        if (kind === "empty") {
          const empty = await panel.locator(".empty-state").evaluate((node) => {
            const box = node.getBoundingClientRect(),
              root = node.closest(".page-sheet,.memory-page"),
              r = root.getBoundingClientRect(),
              style = getComputedStyle(root);
            const children = [...node.children]
              .filter((el) => el.getBoundingClientRect().height > 0)
              .map((el) => el.getBoundingClientRect());
            return {
              drift: Math.abs(
                (children[0].top + children.at(-1).bottom) / 2 -
                  (box.top + box.bottom) / 2,
              ),
              bottom: box.bottom,
              expectedBottom:
                r.top + root.clientHeight - parseFloat(style.paddingBottom),
            };
          });
          assert.ok(
            empty.drift < 2 &&
              Math.abs(empty.bottom - empty.expectedBottom) < 8,
            JSON.stringify({ name, empty }),
          );
        }
        if (!geometry.toolbar) {
          assert.equal(name, "记忆");
          assert.equal(geometry.search, null);
        } else {
          assert.ok(
            Math.abs(geometry.toolbar.y - geometry.title.bottom - 24) < 1,
            JSON.stringify({ name, geometry }),
          );
          assert.ok(
            Math.abs(geometry.search.right - geometry.toolbar.right) < 1,
            JSON.stringify({ name, geometry }),
          );
          if (width >= 1000) {
            assert.ok(
              Math.abs(geometry.search.width - 300) < 1,
              JSON.stringify({ name, geometry }),
            );
            if (geometry.tabs)
              assert.ok(
                Math.abs(
                  geometry.tabs.y +
                    geometry.tabs.height / 2 -
                    geometry.search.y -
                    geometry.search.height / 2,
                ) < 1,
                JSON.stringify({ name, geometry }),
              );
          }
          if (width <= 700)
            assert.ok(
              Math.abs(geometry.search.x - geometry.toolbar.x) < 1 &&
                Math.abs(geometry.search.width - geometry.toolbar.width) < 1,
              JSON.stringify({ name, geometry }),
            );
        }
        if (name === "快捷任务") {
          const before = await panel
            .locator(".page-list-toolbar")
            .boundingBox();
          await panel
            .getByRole("button", { name: "运行记录", exact: true })
            .click();
          await panel
            .getByText(
              "在对话里输入「创建快捷任务」并描述目标，保存后在这里一键运行。",
              { exact: true },
            )
            .waitFor();
          const after = await panel.locator(".page-list-toolbar").boundingBox();
          assert.ok(Math.abs(before.y - after.y) < 1);
          const smallFonts = await panel
            .locator(".record small")
            .evaluateAll((nodes) =>
              nodes.map((n) => parseFloat(getComputedStyle(n).fontSize)),
            );
          assert.ok(smallFonts.every((size) => size >= 14));
          if (width === 1456 && kind === "long") {
            await page.waitForTimeout(220);
            await page.screenshot({
              path: path.join(scratch, `运行记录-${theme}.png`),
            });
          }
          await panel
            .getByRole("button", { name: "已保存任务", exact: true })
            .click();
          if (kind !== "empty")
            assert.ok(
              await panel.locator(".task-card-application").first().isVisible(),
            );
          if (width === 1456 && kind === "single") {
            await panel.locator(".task-card-name").first().click();
            const details = page.getByRole("complementary", {
              name: "快捷任务",
              exact: true,
            });
            await details
              .locator(".shortcut-plan")
              .getByText("等待 10 秒", { exact: true })
              .waitFor();
            assert.equal(await details.locator(".shortcut-plan li").count(), 3);
            await page.waitForTimeout(350);
            await page.screenshot({
              path: path.join(scratch, `快捷任务等待链-${theme}.png`),
            });
            await details
              .getByRole("button", { name: "关闭快捷任务侧栏", exact: true })
              .click();
            await details.waitFor({ state: "hidden" });
            await page.waitForTimeout(350);
          }
        }
        if (name === "记忆")
          assert.equal(await panel.locator(".memory-page-header p").count(), 0);
        if (name === "工具与扩展" && kind !== "empty") {
          const sizes = await panel
            .locator(".extension-summary")
            .first()
            .evaluate((node) => ({
              title: parseFloat(
                getComputedStyle(node.querySelector("strong")).fontSize,
              ),
              description: parseFloat(
                getComputedStyle(node.querySelector("p")).fontSize,
              ),
            }));
          assert.ok(
            sizes.title >= sizes.description && sizes.description >= 15,
            JSON.stringify(sizes),
          );
        }
        geometries.push({ name, ...geometry });
        await page.screenshot({
          path: path.join(scratch, `${name}-${theme}-${width}-${kind}.png`),
        });
      }
      for (const item of geometries) {
        assert.ok(
          Math.abs(item.title.x - geometries[0].title.x) < 1,
          JSON.stringify(geometries),
        );
        assert.equal(item.padding, geometries[0].padding);
      }
      report.push({
        width,
        kind,
        pages: geometries.map(({ name, title, search }) => ({
          name,
          left: title.x,
          searchWidth: search?.width ?? null,
        })),
      });
    }
  }
  assert.deepEqual(errors, []);
  assert.equal(
    frontendStamp(),
    expectedFrontend,
    "验证期间生产前端发生变更，必须重新验证",
  );
  passed = true;
  console.log(
    JSON.stringify({
      engine: native ? "WebView2" : "Edge",
      theme,
      checks: report,
      errors,
    }),
  );
} catch (error) {
  await page
    ?.screenshot({ path: path.join(scratch, "failure.png") })
    .catch(() => {});
  throw error;
} finally {
  if (native && page)
    await page
      .evaluate(
        (success) =>
          window.chrome.webview.postMessage(
            JSON.stringify({ method: "probe.done", success }),
          ),
        passed,
      )
      .catch(() => {});
  await browser?.close();
  if (ipc) await new Promise((resolve) => ipc.close(resolve));
  if (server) await new Promise((resolve) => server.httpServer.close(resolve));
}
