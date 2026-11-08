import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const scratch = path.join(root, "target/.tmp/scheduler-ui");
const temporary = path.join(root, "target/tmp/scheduler-ui");
fs.mkdirSync(scratch, { recursive: true });
fs.mkdirSync(temporary, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = temporary;
const { createServer, preview } = await import("vite");
const { chromium } = await import("playwright");
const production = process.argv.includes("--production");
const theme = process.argv.includes("--dark") ? "dark" : "light";
const native = process.argv.includes("--webview2");
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json")),
).version;
const actions = Array.from({ length: 18 }, (_, i) => ({
  id: `task-${i}`,
  title: i === 0 ? "每日委托" : i === 1 ? "领取奖励" : `配置任务 ${i + 1}`,
  content: i === 1 ? "地下洞窟内容关键词" : "已有配置组",
  publishedRevision: 3,
  available: true,
}));
let libraryFixture = actions;
const defaultActions = [
  {
    id: "default:CloseGame",
    title: "关闭游戏",
    content: "",
    binding: { kind: "closeGame" },
    available: true,
  },
  {
    id: "default:CloseToolSoftware",
    title: "关闭工具软件",
    content: "",
    binding: { kind: "closeToolSoftware" },
    available: true,
  },
];
const task = {
  id: "task-0",
  name: "每日委托",
  description: "已有配置组",
  state: "readyUnverified",
  stateLabel: "可运行",
  actionLabel: "运行",
  runnable: true,
  pinned: false,
  sourceDeleted: false,
  publishedRevision: 3,
  modelUsage: "none",
  zeroToken: true,
  nodeCount: 1,
  updatedAt: "2026-11-01T00:00:00Z",
  shortcut: {
    applicationName: "BetterGI",
    targetName: "委托配置组",
    prepare: [],
    action: { tool: "bgi.run", arguments: {} },
  },
};
const bootstrap = {
  configPath: "E:\\Sleepy Doll\\user\\config.json",
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
  skills: [],
  tools: [],
  plugins: [
    {
      manifest: { id: "windows-scheduler", name: "定时任务", version: "0.1.0" },
      status: "disabled",
      configuredEnabled: false,
    },
  ],
  conversations: [
    {
      id: "normal-chat-1",
      title: "普通对话",
      createdAt: "2026-11-07T12:00:00Z",
      updatedAt: "2026-11-07T12:00:00Z",
      modelId: "demo",
    },
    {
      id: "normal-chat-2",
      title: "另一个对话",
      createdAt: "2026-11-07T12:00:00Z",
      updatedAt: "2026-11-07T12:00:00Z",
      modelId: "demo",
    },
  ],
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
let server, browser, nativeIpc;
let schedules = [];
let historyFixture = [];
const calls = [];
let rejectCreate = false;
let rejectHistory = false;
let toggleDelay = 0;
let passed = false;
const report = {},
  errors = [];
const fillTiming = async (field, value) => {
  if ((await field.getAttribute("readonly")) !== null)
    await field.dblclick({ delay: 70 });
  await field.fill(value);
};
try {
  server = production
    ? await preview({
        configFile: path.join(root, "vite.config.ts"),
        configLoader: "runner",
        logLevel: "error",
        preview: { host: "127.0.0.1", port: 0 },
      })
    : await createServer({
        configFile: path.join(root, "vite.config.ts"),
        configLoader: "runner",
        cacheDir: path.join(temporary, "vite"),
        logLevel: "error",
        server: { host: "127.0.0.1", port: 0, hmr: false },
      });
  if (!production) await server.listen();
  console.log(
    JSON.stringify({
      resource: "scheduler-ui",
      pid: process.pid,
      port: server.httpServer.address().port,
    }),
  );
  browser = native
    ? await chromium.connectOverCDP(
        `http://127.0.0.1:${fs.readFileSync(path.join(temporary, "webview2/cdp.port"), "utf8").trim()}`,
      )
    : await chromium.launch({ channel: "msedge", headless: true });
  const context = native
    ? browser.contexts()[0]
    : await browser.newContext({
        viewport: { width: 1400, height: 900 },
        locale: "zh-CN",
      });
  await context.route(
    (url) => !["localhost", "127.0.0.1"].includes(url.hostname),
    (route) => route.fulfill({ body: "" }),
  );
  const page = native ? context.pages()[0] : await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  if (native)
    page.on("console", (message) => {
      if (message.type() === "error")
        console.error("WebView2 console:", message.text());
    });
  await page.addInitScript(
    ({ version, theme, native }) => {
      localStorage.setItem("sleepy-doll-version", version);
      localStorage.setItem("sleepy-doll-theme", theme);
      localStorage.setItem("sleepy-doll-locale", "zh");
      window.__SLEEPY_DOLL_DESKTOP__ = true;
      window.__SLEEPY_DOLL_FRAMELESS__ = true;
      window.__commits = 0;
      window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
        supportsFiber: true,
        renderers: new Map(),
        inject() {
          return 1;
        },
        onCommitFiberRoot() {
          window.__commits++;
        },
        onCommitFiberUnmount() {},
      };
    },
    { version, theme, native },
  );
  const dispatch = (q) => {
    calls.push(q);
    let result = {},
      ok = true,
      error;
    switch (q.method) {
      case "bootstrap":
        result = bootstrap;
        break;
      case "events.read":
        result = { events: [] };
        break;
      case "task.list":
        result = [];
        break;
      case "plugin.setEnabled":
        bootstrap.plugins[0].configuredEnabled = q.params.enabled;
        bootstrap.plugins[0].status = q.params.enabled ? "enabled" : "disabled";
        break;
      case "scheduler.actions.list": {
        const filtered = (
          q.params.category === "defaults" ? defaultActions : libraryFixture
        ).filter((a) =>
          `${a.title} ${a.content}`.includes(q.params.query || ""),
        );
        result = { items: filtered, total: filtered.length, nextOffset: null };
        break;
      }
      case "scheduler.list":
        result = { items: schedules };
        break;
      case "workflow.delete":
        bootstrap.workflows = bootstrap.workflows.filter(
          (task) => task.id !== q.params.id,
        );
        result = { deleted: true, activeRuns: [], historyKept: true };
        break;
      case "scheduler.preview":
        result = { nextRuns: [new Date(Date.now() + 86400000).toISOString()] };
        break;
      case "scheduler.create":
        if (rejectCreate) {
          rejectCreate = false;
          ok = false;
          error = { message: "注册失败，请重试" };
          break;
        }
        result = {
          ...q.params,
          id: crypto.randomUUID(),
          revision: 1,
          enabled: true,
          registration: "verified",
          entries: q.params.entries.map((e, i) => ({
            ...e,
            entryId: crypto.randomUUID(),
            title:
              defaultActions.find((a) => a.binding.kind === e.binding.kind)
                ?.title ??
              actions.find((a) => a.id === e.binding.shortcutId).title,
          })),
        };
        schedules.push(result);
        break;
      case "scheduler.update":
        result = {
          ...schedules.find((s) => s.id === q.params.id),
          ...q.params.schedule,
          revision: q.params.expectedRevision + 1,
          entries: q.params.schedule.entries.map((e) => ({
            ...e,
            entryId: e.entryId || crypto.randomUUID(),
            title:
              defaultActions.find((a) => a.binding.kind === e.binding.kind)
                ?.title ??
              actions.find((a) => a.id === e.binding.shortcutId).title,
          })),
        };
        schedules = schedules.map((s) => (s.id === result.id ? result : s));
        break;
      case "scheduler.set_enabled":
        result = {
          ...schedules.find((s) => s.id === q.params.id),
          enabled: q.params.enabled,
          revision: q.params.expectedRevision + 1,
        };
        schedules = schedules.map((s) => (s.id === result.id ? result : s));
        break;
      case "scheduler.delete":
        schedules = schedules.filter((s) => s.id !== q.params.id);
        result = { deleted: true };
        break;
      case "scheduler.history":
        if (rejectHistory) {
          rejectHistory = false;
          ok = false;
          error = { message: "读取运行记录失败" };
        } else result = { items: historyFixture };
        break;
    }
    return { id: q.id, ok, result, error };
  };
  if (native) {
    nativeIpc = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      response.writeHead(200, { "content-type": "application/json" });
      const requestBody = JSON.parse(Buffer.concat(chunks).toString());
      const result = dispatch(requestBody);
      if (requestBody.method === "scheduler.set_enabled" && toggleDelay)
        await new Promise((resolve) => setTimeout(resolve, toggleDelay));
      response.end(JSON.stringify(result));
    });
    await new Promise((resolve) => nativeIpc.listen(0, "127.0.0.1", resolve));
    fs.writeFileSync(
      path.join(temporary, "native-ipc.port"),
      String(nativeIpc.address().port),
    );
    console.log(
      JSON.stringify({
        resource: "native-ipc",
        port: nativeIpc.address().port,
      }),
    );
  }
  await page.route("**/ipc", async (route) => {
    const headers = {
      "access-control-allow-origin": "*",
      "access-control-allow-headers": "content-type",
      "access-control-allow-methods": "POST,OPTIONS",
    };
    if (route.request().method() === "OPTIONS") {
      await route.fulfill({ status: 204, headers });
      return;
    }
    const requestBody = route.request().postDataJSON();
    const result = dispatch(requestBody);
    if (requestBody.method === "scheduler.set_enabled" && toggleDelay)
      await new Promise((resolve) => setTimeout(resolve, toggleDelay));
    await route.fulfill({
      headers,
      contentType: "application/json",
      body: JSON.stringify(result),
    });
  });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  assert.ok(
    await page.evaluate(() => window.__commits > 0),
    "提交次数检查需要已激活的 React Hook",
  );
  assert.equal(
    await page.getByRole("button", { name: "已归档", exact: true }).count(),
    0,
  );
  await page
    .getByRole("button", { name: "每日委托 的更多操作", exact: true })
    .click();
  assert.equal(
    await page.getByRole("menuitem", { name: /归档|恢复/ }).count(),
    0,
  );
  await page.keyboard.press("Escape");
  await page.waitForTimeout(220);
  await page.screenshot({
    path: path.join(scratch, `saved-tasks-${theme}.png`),
  });
  assert.equal(
    await page
      .locator(".tasks-tabs-toolbar")
      .getByRole("button", { name: "定时任务", exact: true })
      .count(),
    0,
  );
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "定时任务", exact: true })
    .click();
  await page.getByRole("heading", { name: "定时任务", exact: true }).waitFor();
  await page.locator(".app-account").click();
  const themeControl = page.getByRole("switch", { name: "主题", exact: true });
  await themeControl.waitFor();
  await page.waitForTimeout(180);
  const themeBox = await themeControl.boundingBox();
  await themeControl.click();
  await page.waitForTimeout(150);
  await page.waitForFunction(() =>
    document.body
      .getAnimations({ subtree: true })
      .some((animation) =>
        animation.effect
          .getKeyframes()
          .some((frame) => frame.clipPath?.startsWith("circle")),
      ),
  );
  const ripple = await page.evaluate(async () => {
    const animation = document.body
      .getAnimations({ subtree: true })
      .find((animation) =>
        animation.effect
          .getKeyframes()
          .some((frame) => frame.clipPath?.startsWith("circle")),
      );
    animation.pause();
    animation.currentTime =
      Number(animation.effect.getComputedTiming().duration) / 4;
    await new Promise(requestAnimationFrame);
    return {
      clip: getComputedStyle(document.body, "::view-transition-new(root)")
        .clipPath,
      protected: document
        .querySelector(".app-account-menu")
        .matches(":popover-open"),
    };
  });
  assert.ok(ripple.clip.startsWith("circle("));
  assert.equal(ripple.protected, true);
  assert.equal(
    await page
      .locator(".app-conversations li")
      .first()
      .evaluate((node) => getComputedStyle(node).viewTransitionName),
    "none",
  );
  await page.screenshot({
    path: path.join(scratch, `theme-ripple-${theme}.png`),
  });
  await page.evaluate(() =>
    document.body
      .getAnimations({ subtree: true })
      .forEach((animation) => animation.finish()),
  );
  await page.waitForFunction(
    () => document.body.dataset.themeReveal !== "true",
  );
  await page.mouse.click(
    themeBox.x + themeBox.width / 2,
    themeBox.y + themeBox.height / 2,
  );
  await page.waitForFunction(
    () => document.body.dataset.themeReveal !== "true",
  );
  report.themeRipple = ripple;
  const targets = [];
  await page.exposeFunction("recordThemeTarget", (target) =>
    targets.push(target),
  );
  await page.evaluate(() =>
    document.addEventListener(
      "pointerdown",
      (event) => {
        if (event.clientY > innerHeight - 260)
          window.recordThemeTarget(event.target.nodeName);
      },
      true,
    ),
  );
  for (let index = 0; index < 6; index++) {
    await page.mouse.click(
      themeBox.x + themeBox.width / 2,
      themeBox.y + themeBox.height / 2,
    );
    await page.waitForTimeout(45);
  }
  for (let index = 0; index < 10; index++) {
    await page.mouse.click(
      themeBox.x + themeBox.width / 2,
      themeBox.y + themeBox.height / 2,
    );
    await page.waitForTimeout(8);
  }
  await page.waitForFunction(
    () => document.body.dataset.themeReveal !== "true",
  );
  assert.equal(
    await page.locator(".app-account-menu[data-open=true]").isVisible(),
    true,
    JSON.stringify(targets),
  );
  assert.equal(
    await page.evaluate(() => document.documentElement.dataset.theme),
    theme,
  );
  assert.equal(
    await page
      .getByRole("heading", { name: "定时任务", exact: true })
      .isVisible(),
    true,
  );
  await page.locator(".app-account").click();
  report.rapidThemeSwitch = { targets: targets.slice(), menuRetained: true };
  await page.locator(".app-account").click();
  await page.getByRole("menuitem", { name: "设置", exact: true }).click();
  const settingsTheme = page.locator(".settings-general .app-theme-switch");
  await settingsTheme.waitFor();
  await page.waitForTimeout(250);
  const settingsBox = await settingsTheme.boundingBox();
  for (let index = 0; index < 6; index++) {
    await page.mouse.click(
      settingsBox.x + settingsBox.width / 2,
      settingsBox.y + settingsBox.height / 2,
    );
    await page.waitForTimeout(45);
  }
  await page.waitForFunction(
    () => document.body.dataset.themeReveal !== "true",
  );
  assert.equal(await page.locator(".settings-general").isVisible(), true);
  assert.equal(
    await page.evaluate(() => document.documentElement.dataset.theme),
    theme,
  );
  assert.equal(await page.locator(".settings-group[popover]").count(), 0);
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "定时任务", exact: true })
    .click();
  await page.getByRole("heading", { name: "定时任务", exact: true }).waitFor();
  report.settingsThemeRipple = true;
  assert.equal(await page.locator(".schedules-page .sd-tabs").count(), 0);
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(scratch, `empty-${theme}.png`) });
  report.emptyLayout = await page
    .locator(".schedule-list-region")
    .evaluate((region) => {
      const empty = region
        .querySelector(".empty-state")
        .getBoundingClientRect();
      const icon = region
        .querySelector(".empty-state svg")
        .getBoundingClientRect();
      const action = region
        .querySelector(".empty-state h3")
        .getBoundingClientRect();
      return {
        centerDrift: Math.abs(
          (icon.top + action.bottom) / 2 - (empty.top + empty.bottom) / 2,
        ),
        isolatedActions: region.querySelectorAll(".schedule-plugin-enable")
          .length,
      };
    });
  assert.ok(
    report.emptyLayout.centerDrift < 2,
    JSON.stringify(report.emptyLayout),
  );
  assert.equal(report.emptyLayout.isolatedActions, 0);
  const toolbar = await page
    .locator(".schedule-panel > .schedule-toolbar")
    .boundingBox();
  const createButton = await page
    .getByRole("button", { name: "新建定时任务", exact: true })
    .boundingBox();
  assert.ok(
    Math.abs(createButton.x + createButton.width - toolbar.x - toolbar.width) <
      2,
    "新建操作应跟内容区右边缘对齐",
  );
  assert.equal(
    await page
      .getByRole("button", { name: "启用定时任务插件", exact: true })
      .count(),
    0,
  );
  await page.getByRole("button", { name: "新建定时任务", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "新建定时任务",
    exact: true,
  });
  await dialog
    .getByRole("button", { name: "添加 每日委托", exact: true })
    .waitFor();
  await page.waitForFunction(() =>
    document.activeElement?.classList.contains("schedule-dialog"),
  );
  await page.waitForTimeout(250);
  assert.equal(
    await dialog.evaluate((node) => node === document.activeElement),
    true,
  );
  await page.screenshot({
    path: path.join(scratch, `initial-focus-${theme}.png`),
  });
  await page.keyboard.press("Tab");
  assert.equal(
    await dialog
      .getByRole("button", { name: "关闭", exact: true })
      .evaluate((node) => node === document.activeElement),
    true,
  );
  await dialog.evaluate((node) => node.focus());
  await page.keyboard.press("Shift+Tab");
  assert.equal(
    await dialog
      .getByRole("button", { name: "创建并启用", exact: true })
      .evaluate((node) => node === document.activeElement),
    true,
  );
  await page.keyboard.press("Tab");
  assert.equal(
    await dialog
      .getByRole("button", { name: "关闭", exact: true })
      .evaluate((node) => node === document.activeElement),
    true,
  );
  report.noInitialFieldFocus = true;
  const hourInput = dialog.getByRole("textbox", {
    name: "时刻 1 小时",
    exact: true,
  });
  await hourInput.click();
  await page
    .getByRole("group", { name: "循环选择时间", exact: true })
    .waitFor();
  assert.equal(await hourInput.getAttribute("readonly"), "");
  const minuteInput = dialog.getByRole("textbox", {
    name: "时刻 1 分钟",
    exact: true,
  });
  const initialTime = [
    await hourInput.inputValue(),
    await minuteInput.inputValue(),
  ];
  await minuteInput.dblclick({ delay: 70 });
  await page
    .getByRole("group", { name: "循环选择时间", exact: true })
    .waitFor({ state: "hidden" });
  await page.screenshot({
    path: path.join(scratch, `time-input-${theme}.png`),
  });
  assert.equal(
    await minuteInput.evaluate(
      (node) =>
        !node.readOnly &&
        node === document.activeElement &&
        node.selectionEnd - node.selectionStart === node.value.length,
    ),
    true,
  );
  await hourInput.fill("24");
  await hourInput.press("Enter");
  assert.equal(await hourInput.getAttribute("aria-invalid"), "true");
  assert.equal(await hourInput.isEditable(), true);
  await hourInput.fill("23");
  await minuteInput.fill("60");
  await minuteInput.press("Enter");
  assert.equal(await minuteInput.getAttribute("aria-invalid"), "true");
  await minuteInput.press("Escape");
  assert.deepEqual(
    [await hourInput.inputValue(), await minuteInput.inputValue()],
    initialTime,
  );
  assert.equal(await minuteInput.getAttribute("readonly"), "");
  assert.equal(await dialog.isVisible(), true);
  report.timeClickAndDoubleClick = true;
  const queue = dialog.locator(".schedule-queue");
  const names = () => dialog.locator(".schedule-entry-title").allTextContents();
  const drag = async (source, target) => {
    const from = await source.boundingBox();
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      from.x + from.width / 2 + 8,
      from.y + from.height / 2,
    );
    await page.mouse.move(target.x, target.y, { steps: 10 });
    await page.waitForTimeout(50);
    await page.mouse.up();
    await page.waitForTimeout(240);
  };
  const append = async (title) => {
    const box = await queue.boundingBox();
    const rows = await queue.locator(".schedule-queue-row").all();
    const last = rows.length ? await rows.at(-1).boundingBox() : null;
    await drag(
      dialog.getByRole("button", { name: `添加 ${title}`, exact: true }),
      {
        x: box.x + box.width / 2,
        y: last
          ? Math.min(box.y + box.height - 12, last.y + last.height + 12)
          : box.y + box.height / 2,
      },
    );
  };
  await append("每日委托");
  await append("领取奖励");
  await append("每日委托");
  assert.deepEqual(await names(), ["每日委托", "领取奖励", "每日委托"]);
  assert.equal(await dialog.locator(".schedule-library-row").count(), 18);
  const ids = await queue
    .locator("[data-entry-key]")
    .evaluateAll((nodes) => nodes.map((n) => n.dataset.entryKey));
  assert.equal(new Set(ids).size, 3);
  report.duplicateDrag = true;
  await dialog.getByRole("button", { name: "默认", exact: true }).click();
  await dialog
    .getByRole("button", { name: "添加 关闭游戏", exact: true })
    .waitFor();
  await append("关闭游戏");
  await dialog
    .getByRole("button", { name: "添加一份 关闭工具软件", exact: true })
    .click();
  await dialog
    .getByRole("button", { name: "添加一份 关闭游戏", exact: true })
    .click();
  assert.deepEqual((await names()).slice(-3), [
    "关闭游戏",
    "关闭工具软件",
    "关闭游戏",
  ]);
  assert.equal(await dialog.locator(".schedule-library-row").count(), 2);
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(scratch, `defaults-${theme}.png`) });
  for (const [index, title] of [
    [6, "关闭游戏"],
    [5, "关闭工具软件"],
    [4, "关闭游戏"],
  ]) {
    await dialog
      .getByRole("button", { name: `移除第 ${index} 项 ${title}`, exact: true })
      .click();
  }
  await dialog.getByRole("button", { name: "快捷任务", exact: true }).click();
  await dialog
    .getByRole("button", { name: "添加 每日委托", exact: true })
    .waitFor();
  report.defaultActionsDragAndRepeat = true;
  const first = await queue
    .locator(".schedule-queue-row")
    .first()
    .boundingBox();
  await drag(
    dialog.getByRole("button", { name: "移动第 3 项 每日委托", exact: true }),
    { x: first.x + 90, y: first.y + 2 },
  );
  assert.deepEqual(await names(), ["每日委托", "每日委托", "领取奖励"]);
  assert.equal(
    await queue
      .locator("[data-entry-key]")
      .first()
      .getAttribute("data-entry-key"),
    ids[2],
  );
  await dialog
    .getByRole("button", { name: "移动第 2 项 每日委托", exact: true })
    .press("ArrowDown");
  assert.deepEqual(await names(), ["每日委托", "领取奖励", "每日委托"]);
  const from = await dialog
    .getByRole("button", { name: "添加 领取奖励", exact: true })
    .boundingBox();
  await page.mouse.move(from.x + 60, from.y + 20);
  await page.mouse.down();
  await page.mouse.move(from.x + 90, from.y + 20);
  await page.waitForTimeout(80);
  const before = await page.evaluate(() => window.__commits);
  for (let i = 0; i < 24; i++) {
    await page.mouse.move(from.x + 90 + i, from.y + 20);
    await page.evaluate(() => new Promise(requestAnimationFrame));
  }
  report.dragMoveCommits =
    (await page.evaluate(() => window.__commits)) - before;
  assert.ok(report.dragMoveCommits <= 2);
  const ghostBox = await page.locator(".schedule-drag-ghost").boundingBox();
  report.dragPointerDrift = Math.abs(ghostBox.x - (from.x + 53));
  assert.ok(report.dragPointerDrift < 1, "拖拽影像必须保留抓取位置");
  await page.keyboard.press("Escape");
  await page.mouse.up();
  assert.deepEqual(await names(), ["每日委托", "领取奖励", "每日委托"]);
  assert.equal(await dialog.count(), 1);
  report.executionLayoutJump = await dialog.evaluate(async (panel) => {
    const execution = panel.querySelector(".schedule-execution"),
      before = execution.getBoundingClientRect().top;
    [...panel.querySelectorAll('[aria-label="重复方式"] button')]
      .find((button) => button.textContent === "每周")
      .click();
    await new Promise(requestAnimationFrame);
    panel.getAnimations({ subtree: true }).forEach((animation) => {
      animation.pause();
      animation.currentTime = 0;
    });
    const difference = Math.abs(execution.getBoundingClientRect().top - before);
    panel
      .getAnimations({ subtree: true })
      .forEach((animation) => animation.finish());
    return difference;
  });
  assert.ok(
    report.executionLayoutJump < 1,
    "切换时间模式时执行区应从当前位置移动",
  );
  const search = dialog.getByRole("textbox", { name: "搜索标题或内容" });
  await search.fill("地下洞窟");
  await page.waitForTimeout(300);
  assert.equal(await dialog.locator(".schedule-library-row").count(), 1);
  assert.ok(
    calls.some(
      (c) =>
        c.method === "scheduler.actions.list" && c.params.query === "地下洞窟",
    ),
  );
  await dialog
    .getByRole("button", { name: "添加一份 领取奖励", exact: true })
    .click();
  assert.equal((await names()).length, 4);
  await dialog
    .getByRole("button", { name: "移除第 4 项 领取奖励", exact: true })
    .click();
  await search.fill("");
  const hours = dialog.getByRole("textbox", {
      name: "时刻 1 小时",
      exact: true,
    }),
    minutes = dialog.getByRole("textbox", { name: "时刻 1 分钟", exact: true });
  await fillTiming(hours, "23");
  await fillTiming(minutes, "59");
  await dialog.getByRole("button", { name: "选择时刻 1", exact: true }).click();
  const hw = page.getByRole("spinbutton", { name: "小时", exact: true }),
    mw = page.getByRole("spinbutton", { name: "分钟", exact: true });
  await hw.focus();
  await hw.press("ArrowDown");
  await page.waitForTimeout(600);
  assert.equal(await hours.inputValue(), "00");
  for (let index = 0; index < 5; index++) await hw.press("ArrowDown");
  await page.waitForTimeout(600);
  assert.equal(
    await hours.inputValue(),
    "05",
    "连续按键必须累加目标，不能被尚未完成的动画吞掉",
  );
  await hw.press("Home");
  await page.waitForTimeout(600);
  assert.equal(await minutes.inputValue(), "59");
  await mw.focus();
  await mw.press("ArrowDown");
  await page.waitForTimeout(600);
  assert.equal(await minutes.inputValue(), "00");
  assert.equal(await hours.inputValue(), "00");
  const wbox = await hw.boundingBox();
  await page.mouse.move(wbox.x + 40, wbox.y + 95);
  await page.mouse.down();
  const wheelCommits = await page.evaluate(() => window.__commits);
  await page.mouse.move(wbox.x + 40, wbox.y + 171, { steps: 12 });
  report.wheelMoveCommits =
    (await page.evaluate(() => window.__commits)) - wheelCommits;
  assert.equal(report.wheelMoveCommits, 0, "拨轮拖动不能逐帧触发 React 提交");
  await page.waitForTimeout(110);
  await page.mouse.up();
  await page.waitForTimeout(600);
  assert.equal(await hours.inputValue(), "22");
  report.cyclicWheel = true;
  report.continuousWheelInput = true;
  await hw.focus();
  await page.mouse.move(wbox.x + 40, wbox.y + 95);
  await page.mouse.down();
  await page.mouse.move(wbox.x + 40, wbox.y + 57, { steps: 5 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await page.waitForTimeout(250);
  assert.equal(await hours.inputValue(), "23");
  assert.equal(await minutes.inputValue(), "59");
  assert.equal(await dialog.count(), 1);
  await dialog.getByRole("button", { name: "每月", exact: true }).click();
  await dialog.getByRole("button", { name: "1日", exact: true }).click();
  const monthGrid = page.getByRole("group", { name: "每月日期", exact: true });
  await monthGrid.getByRole("button", { name: "31", exact: true }).click();
  await monthGrid.getByRole("button", { name: "月末", exact: true }).click();
  const lastDate = await monthGrid
    .getByRole("button", { name: "31", exact: true })
    .boundingBox();
  const lastDay = await monthGrid
    .getByRole("button", { name: "月末", exact: true })
    .boundingBox();
  assert.ok(Math.abs(lastDate.y - lastDay.y) < 1 && lastDay.x > lastDate.x);
  await page.waitForTimeout(220);
  await page.screenshot({
    path: path.join(scratch, `monthly-picker-${theme}.png`),
  });
  await dialog.getByRole("button", { name: "按天", exact: true }).click();
  await dialog.getByRole("button", { name: "每月", exact: true }).click();
  await dialog.getByRole("button", { name: /1日.*31日.*月末/ }).click();
  assert.equal(
    await monthGrid
      .getByRole("button", { name: "31", exact: true })
      .getAttribute("aria-pressed"),
    "true",
  );
  await dialog.getByRole("button", { name: /1日.*31日.*月末/ }).click();
  await monthGrid.waitFor({ state: "hidden" });
  assert.equal(await dialog.locator(".schedule-time-input").count(), 1);
  assert.equal(
    await dialog.getByRole("button", { name: "添加时间", exact: true }).count(),
    0,
  );
  assert.equal(
    await dialog.getByRole("button", { name: /^移除时刻/ }).count(),
    0,
  );
  report.singleTimeOnly = true;
  await page.screenshot({
    path: path.join(scratch, `monthly-single-time-${theme}.png`),
  });
  await page.screenshot({ path: path.join(scratch, "wide.png") });
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.waitForTimeout(100);
  await page.screenshot({ path: path.join(scratch, "narrow.png") });
  const bounds = await dialog.boundingBox();
  assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 1001);
  assert.ok(bounds.y + bounds.height <= 761);
  await page.setViewportSize({ width: 560, height: 780 });
  await page.waitForTimeout(100);
  await dialog.getByRole("button", { name: "任务库", exact: true }).click();
  await dialog
    .getByRole("button", { name: "添加一份 每日委托", exact: true })
    .click();
  assert.equal((await names()).length, 4);
  await dialog
    .getByRole("button", { name: "移除第 4 项 每日委托", exact: true })
    .click();
  await page.screenshot({ path: path.join(scratch, "mobile.png") });
  await page.setViewportSize({ width: 1400, height: 900 });
  rejectCreate = true;
  await dialog.getByRole("button", { name: "创建并启用", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "注册失败" }).waitFor();
  assert.deepEqual(await names(), ["每日委托", "领取奖励", "每日委托"]);
  await dialog.getByRole("button", { name: "创建并启用", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(schedules.length, 1);
  assert.equal(
    await page
      .getByRole("button", { name: "新建定时任务", exact: true })
      .evaluate((node) => node === document.activeElement),
    true,
  );
  report.focusRestored = true;
  assert.equal(
    bootstrap.plugins[0].configuredEnabled,
    true,
    "保存时接通插件，不能要求额外启用操作",
  );
  assert.equal(schedules[0].trigger.kind, "monthly");
  assert.deepEqual(schedules[0].trigger.daysOfMonth, [1, 31]);
  assert.equal(schedules[0].trigger.lastDayOfMonth, true);
  const create = calls.filter((c) => c.method === "scheduler.create").at(-1);
  assert.deepEqual(
    create.params.entries.map((e) => e.binding.shortcutId),
    ["task-0", "task-1", "task-0"],
  );
  assert.ok(create.params.entries.every((e) => !e.entryId));
  report.savedOrder = true;
  const row = page.locator(".schedule-row").first();
  const openScheduleEditor = async () => {
    await row.getByRole("button", { name: /的更多操作/ }).click();
    await page
      .getByRole("menu", { name: /的更多操作/ })
      .getByRole("menuitem", { name: "编辑", exact: true })
      .click();
    await page
      .getByRole("dialog", { name: "编辑定时任务", exact: true })
      .waitFor();
  };
  const control = row.getByRole("switch");
  const toggles = () =>
    calls.filter((call) => call.method === "scheduler.set_enabled").length;
  const waitChecked = (value) =>
    page.waitForFunction(
      (value) =>
        document
          .querySelector(".schedule-row [role=switch]")
          ?.getAttribute("aria-checked") === String(value),
      value,
    );
  let toggleCount = toggles();
  await row.click({ position: { x: 8, y: 30 } });
  await waitChecked(false);
  assert.equal(toggles(), ++toggleCount);
  await control.click();
  await waitChecked(true);
  assert.equal(toggles(), ++toggleCount);
  await row.locator(".schedule-row-title").click();
  await waitChecked(false);
  assert.equal(toggles(), ++toggleCount);
  await control.click();
  await waitChecked(true);
  assert.equal(toggles(), ++toggleCount);
  await row.locator(".schedule-row-time").click();
  await waitChecked(false);
  assert.equal(toggles(), ++toggleCount);
  const more = row.getByRole("button", { name: /的更多操作/ });
  await more.click();
  const menuActions = page.getByRole("menu", { name: /的更多操作/ });
  assert.equal(
    await menuActions.evaluate(
      (node) => node.getAnimations({ subtree: true }).length > 0,
    ),
    true,
  );
  assert.equal(toggles(), toggleCount);
  const moreBox = await more.boundingBox();
  for (let index = 0; index < 4; index++) {
    await page.mouse.click(
      moreBox.x + moreBox.width / 2,
      moreBox.y + moreBox.height / 2,
    );
    await page.waitForTimeout(12);
  }
  await page.waitForTimeout(240);
  assert.equal(
    await menuActions.isVisible(),
    true,
    "中途重开的菜单不得被旧退场回调卸载",
  );
  assert.equal(toggles(), toggleCount);
  await row.click({ position: { x: 8, y: 30 } });
  await menuActions.waitFor({ state: "hidden" });
  assert.equal(toggles(), toggleCount, "关闭菜单的点击不得切换行状态");
  await row.click({ position: { x: 8, y: 30 } });
  await waitChecked(true);
  assert.equal(toggles(), ++toggleCount);
  await control.click();
  await waitChecked(false);
  assert.equal(toggles(), ++toggleCount);
  const rowBox = await row.boundingBox();
  await row.evaluate((node) => {
    const selection = getSelection();
    selection.selectAllChildren(node.querySelector(".schedule-row-title"));
  });
  await row.click({ button: "right", position: { x: 8, y: 30 } });
  const contextBox = await menuActions.boundingBox();
  assert.ok(contextBox.x < rowBox.x + 64);
  assert.equal(await page.locator(".context-menu").count(), 0);
  assert.equal(toggles(), toggleCount);
  await page.waitForTimeout(260);
  assert.equal(
    await menuActions.evaluate((node) => getComputedStyle(node).opacity),
    "1",
  );
  await page.screenshot({
    path: path.join(scratch, `schedule-context-${theme}.png`),
  });
  await page.keyboard.press("Escape");
  const closing = page.locator('.schedule-action-menu[aria-hidden="true"]');
  assert.equal(await closing.count(), 1);
  const closingBox = await closing.boundingBox();
  await page.mouse.click(closingBox.x + 20, closingBox.y + 12);
  assert.equal(toggles(), toggleCount, "退场中的菜单仍要挡住点击");
  await closing.waitFor({ state: "detached" });
  await page.evaluate(() => getSelection()?.removeAllRanges());
  await more.click();
  const scheduleSearch = page.getByRole("textbox", {
    name: "搜索定时任务",
    exact: true,
  });
  await scheduleSearch.fill("无匹配筛选");
  await menuActions.waitFor({ state: "hidden" });
  await scheduleSearch.fill("");
  await row.waitFor();
  assert.equal(
    await page.locator('.schedule-action-menu[aria-hidden="false"]').count(),
    0,
  );
  assert.equal(toggles(), toggleCount);
  toggleDelay = 180;
  const rowBounds = await row.boundingBox();
  await page.mouse.dblclick(rowBounds.x + 8, rowBounds.y + 30, { delay: 20 });
  await waitChecked(true);
  assert.equal(toggles(), ++toggleCount, "请求未结束时不重复写入");
  toggleDelay = 0;
  await control.click();
  await waitChecked(false);
  assert.equal(toggles(), ++toggleCount);
  await more.click();
  await menuActions
    .getByRole("menuitem", { name: "编辑", exact: true })
    .click();
  const menuEdit = page.getByRole("dialog", {
    name: "编辑定时任务",
    exact: true,
  });
  await menuEdit.waitFor();
  assert.equal(toggles(), toggleCount);
  await menuEdit.getByRole("button", { name: "取消", exact: true }).click();
  await menuEdit.waitFor({ state: "hidden" });
  report.rowToggleAndMenu = {
    exactlyOnce: true,
    rightClick: true,
    exitShield: true,
    outsideDismissOnly: true,
  };
  assert.equal(schedules[0].enabled, false);
  const priorTrigger = schedules[0].trigger;
  schedules[0] = {
    ...schedules[0],
    trigger: { kind: "once", at: new Date(Date.now() - 60000).toISOString() },
  };
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "定时任务", exact: true })
    .click();
  await page.waitForTimeout(250);
  const beforeMessage = await row.boundingBox();
  const beforeEnable = toggles();
  await row.click({ position: { x: 8, y: 30 } });
  await page.getByRole("alert").filter({ hasText: "执行时间已过" }).waitFor();
  await page.waitForTimeout(260);
  assert.equal(
    await page
      .getByRole("alert")
      .evaluate((node) => getComputedStyle(node).opacity),
    "1",
  );
  assert.equal(toggles(), beforeEnable);
  assert.ok(Math.abs((await row.boundingBox()).y - beforeMessage.y) < 1);
  await page.screenshot({
    path: path.join(scratch, `schedule-message-${theme}.png`),
  });
  await page.getByRole("alert").waitFor({ state: "hidden", timeout: 5000 });
  assert.ok(Math.abs((await row.boundingBox()).y - beforeMessage.y) < 1);
  report.actionMessageAutoDismiss = true;
  await row.click({ position: { x: 8, y: 30 } });
  await page
    .getByRole("alert")
    .getByRole("button", { name: "修改时间", exact: true })
    .click();
  const expiredEditor = page.getByRole("dialog", {
    name: "编辑定时任务",
    exact: true,
  });
  await expiredEditor.waitFor();
  await expiredEditor
    .getByRole("button", { name: "取消", exact: true })
    .click();
  await expiredEditor.waitFor({ state: "hidden" });
  schedules[0] = { ...schedules[0], trigger: priorTrigger };
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "定时任务", exact: true })
    .click();
  await page.waitForTimeout(250);
  report.expiredScheduleMessage = true;
  await openScheduleEditor();
  const edit = page.getByRole("dialog", { name: "编辑定时任务", exact: true });
  await page.waitForFunction(() =>
    document.activeElement?.classList.contains("schedule-dialog"),
  );
  await edit.getByRole("button", { name: "指定日期", exact: true }).click();
  const future = new Date(Date.now() + 86400000 * 2);
  await fillTiming(
    edit.getByLabel("执行日期", { exact: true }),
    `${future.getFullYear()}-${String(future.getMonth() + 1).padStart(2, "0")}-${String(future.getDate()).padStart(2, "0")}`,
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 小时", exact: true }),
    "18",
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 分钟", exact: true }),
    "30",
  );
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await edit.waitFor({ state: "hidden" });
  assert.equal(schedules[0].trigger.kind, "once");
  const once = new Date(schedules[0].trigger.at);
  assert.equal(once.getHours(), 18);
  assert.equal(once.getMinutes(), 30);
  assert.ok(
    calls
      .filter((c) => c.method === "scheduler.update")
      .at(-1)
      .params.schedule.entries.every((e) => e.entryId),
  );
  report.onceDate = true;
  await openScheduleEditor();
  await edit.getByRole("button", { name: "按天", exact: true }).click();
  const intervalInput = edit.getByRole("textbox", {
    name: "间隔天数",
    exact: true,
  });
  await intervalInput.fill("2");
  const startInput = edit.getByLabel("开始日期", { exact: true });
  await startInput.waitFor();
  await fillTiming(startInput, `${new Date().getFullYear()}-02-30`);
  await startInput.press("Escape");
  const anchorDate = new Date(Date.now() + 86400000 * 2);
  const anchorText = `${anchorDate.getFullYear()}-${String(anchorDate.getMonth() + 1).padStart(2, "0")}-${String(anchorDate.getDate()).padStart(2, "0")}`;
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 小时", exact: true }),
    "04",
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 分钟", exact: true }),
    "30",
  );
  await page.getByRole("heading", { name: "执行时间", exact: true }).click();
  const upcoming = new Date();
  upcoming.setHours(4, 30, 0, 0);
  if (upcoming.getTime() <= Date.now())
    upcoming.setDate(upcoming.getDate() + 1);
  const upcomingText = `${upcoming.getFullYear()}-${String(upcoming.getMonth() + 1).padStart(2, "0")}-${String(upcoming.getDate()).padStart(2, "0")}`;
  assert.equal(
    await startInput.inputValue(),
    upcomingText,
    "撤销手输日期后仍按下次所选时刻自动填入开始日",
  );
  await fillTiming(startInput, anchorText);
  await startInput.press("Enter");
  const intervalUpdates = calls.filter(
    (c) => c.method === "scheduler.update",
  ).length;
  for (const bad of ["0", "2.5", "366", ""]) {
    await intervalInput.fill(bad);
    await edit.getByRole("button", { name: "保存更改", exact: true }).click();
    await page.getByRole("alert").filter({ hasText: "天数间隔" }).waitFor();
    assert.equal(
      calls.filter((c) => c.method === "scheduler.update").length,
      intervalUpdates,
    );
  }
  await intervalInput.fill("2");
  await page.waitForTimeout(250);
  const clockNode = await edit
    .getByRole("textbox", { name: "时刻 1 小时", exact: true })
    .elementHandle();
  await intervalInput.fill("1");
  await page.waitForTimeout(30);
  await intervalInput.fill("2");
  await page.waitForTimeout(260);
  assert.equal(
    await clockNode.evaluate((node) => node.isConnected),
    true,
    "改变天数时保留时间控件身份",
  );
  assert.equal(await startInput.inputValue(), anchorText);
  for (const width of [1456, 1000, 360]) {
    await page.setViewportSize({ width, height: 876 });
    await page.waitForTimeout(220);
    const dateBox = await startInput.locator("..").boundingBox();
    const clockBox = await edit.locator(".schedule-time-field").boundingBox();
    assert.ok(Math.abs(dateBox.width - clockBox.width) < 1);
    if (width > 380) assert.ok(Math.abs(dateBox.y - clockBox.y) < 1);
    await page.screenshot({
      path: path.join(scratch, `daily-interval-${theme}-${width}.png`),
    });
  }
  await page.setViewportSize({ width: 1400, height: 900 });
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await edit.waitFor({ state: "hidden" });
  assert.deepEqual(schedules[0].trigger, {
    kind: "daily",
    intervalDays: 2,
    startDate: anchorText,
    localTimes: ["04:30"],
  });
  await openScheduleEditor();
  assert.equal(await intervalInput.inputValue(), "2");
  assert.equal(await startInput.inputValue(), anchorText);
  await edit.getByRole("button", { name: "选择时刻 1", exact: true }).click();
  await page
    .getByRole("group", { name: "循环选择时间", exact: true })
    .waitFor();
  await intervalInput.fill("1");
  await page
    .getByRole("group", { name: "循环选择时间", exact: true })
    .waitFor({ state: "hidden" });
  await startInput.waitFor({ state: "hidden" });
  await page.waitForTimeout(220);
  const timingBox = await edit
    .locator(".schedule-primary-timing")
    .boundingBox();
  const dailyClock = await edit.locator(".schedule-time-field").boundingBox();
  assert.ok(Math.abs(timingBox.x - dailyClock.x) < 1);
  report.dayIntervalSaveReopenAndAnimation = true;
  await edit.getByRole("button", { name: "取消", exact: true }).click();
  const intervalDiscard = page.getByRole("dialog", {
    name: "放弃更改？",
    exact: true,
  });
  await intervalDiscard
    .getByRole("button", { name: "放弃更改", exact: true })
    .click();
  await edit.waitFor({ state: "hidden" });
  assert.equal(schedules[0].trigger.intervalDays, 2);
  await openScheduleEditor();
  await edit.getByRole("button", { name: "每周", exact: true }).click();
  await edit.getByRole("button", { name: "周二", exact: true }).click();
  await edit.getByRole("button", { name: "周四", exact: true }).click();
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 小时", exact: true }),
    "20",
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 分钟", exact: true }),
    "00",
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 小时", exact: true }),
    "99",
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 分钟", exact: true }),
    "00",
  );
  const updatesBefore = calls.filter(
    (c) => c.method === "scheduler.update",
  ).length;
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "请输入有效时间" }).waitFor();
  assert.equal(
    calls.filter((c) => c.method === "scheduler.update").length,
    updatesBefore,
  );
  await fillTiming(
    edit.getByRole("textbox", { name: "时刻 1 小时", exact: true }),
    "08",
  );
  await edit.getByRole("button", { name: "选择时刻 1", exact: true }).click();
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(scratch, "wheel.png") });
  const wheelBox = await page.locator(".schedule-wheel").boundingBox();
  const footerBox = await edit.locator(".sd-dialog-foot").boundingBox();
  assert.ok(wheelBox.y + wheelBox.height <= footerBox.y + 1);
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await edit.waitFor({ state: "hidden" });
  assert.deepEqual(schedules[0].trigger, {
    kind: "weekly",
    weekdays: [1, 3, 5],
    localTimes: ["08:00"],
  });
  report.weeklySingleTime = true;
  schedules[0] = {
    ...schedules[0],
    trigger: { ...schedules[0].trigger, localTimes: ["08:00", "20:00"] },
  };
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "定时任务", exact: true })
    .click();
  await openScheduleEditor();
  assert.equal(await edit.locator(".schedule-time-input").count(), 1);
  const updatesBeforeReplacement = calls.filter(
    (c) => c.method === "scheduler.update",
  ).length;
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  const replacement = page.getByRole("dialog", {
    name: "更新执行时间？",
    exact: true,
  });
  await replacement.waitFor();
  assert.equal(
    calls.filter((c) => c.method === "scheduler.update").length,
    updatesBeforeReplacement,
  );
  assert.deepEqual(schedules[0].trigger.localTimes, ["08:00", "20:00"]);
  assert.ok((await replacement.innerText()).includes("08:00"));
  assert.ok((await replacement.boundingBox()).width <= 480);
  await page.waitForTimeout(220);
  await page.screenshot({
    path: path.join(scratch, `time-replacement-${theme}.png`),
  });
  await replacement.getByRole("button", { name: "取消", exact: true }).click();
  await replacement.waitFor({ state: "hidden" });
  assert.deepEqual(schedules[0].trigger.localTimes, ["08:00", "20:00"]);
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await replacement
    .getByRole("button", { name: "保存更改", exact: true })
    .click();
  await edit.waitFor({ state: "hidden" });
  assert.deepEqual(schedules[0].trigger.localTimes, ["08:00"]);
  report.multipleTimeReplacementExplicit = true;
  await page
    .getByRole("button", {
      name: `${schedules[0].name} 的更多操作`,
      exact: true,
    })
    .click();
  const menuBox = await page.getByRole("menu").boundingBox();
  assert.ok(
    menuBox.x >= 0 && menuBox.y >= 0 && menuBox.y + menuBox.height <= 901,
  );
  rejectHistory = true;
  await page.getByRole("menuitem", { name: "运行记录", exact: true }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "读取运行记录失败" })
    .waitFor();
  await page
    .getByRole("dialog", { name: /运行记录/ })
    .waitFor({ state: "hidden" });
  await page
    .getByRole("alert")
    .getByRole("button", { name: "重新读取", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: /运行记录/ })
    .getByRole("heading", { name: "还没有运行记录" })
    .waitFor();
  const historyDialog = page.getByRole("dialog", { name: /运行记录/ });
  for (const width of [1456, 1000, 360]) {
    await page.setViewportSize({ width, height: 876 });
    await page.waitForTimeout(250);
    const drift = await historyDialog.evaluate((panel) => {
      const body = panel
        .querySelector(".sd-dialog-body")
        .getBoundingClientRect();
      const icon = panel
        .querySelector(".empty-state svg")
        .getBoundingClientRect();
      const title = panel
        .querySelector(".empty-state h3")
        .getBoundingClientRect();
      return {
        x: Math.abs(
          (title.left + title.right) / 2 - (body.left + body.right) / 2,
        ),
        y: Math.abs(
          (icon.top + title.bottom) / 2 - (body.top + body.bottom) / 2,
        ),
      };
    });
    assert.ok(drift.x < 1 && drift.y < 1, JSON.stringify(drift));
    await page.screenshot({
      path: path.join(scratch, `history-empty-${theme}-${width}.png`),
    });
  }
  await page
    .getByRole("dialog", { name: /运行记录/ })
    .getByRole("button", { name: "关闭", exact: true })
    .click();
  await historyDialog.waitFor({ state: "hidden" });
  await page.setViewportSize({ width: 1456, height: 876 });
  for (const count of [1, 2, 12]) {
    historyFixture = Array.from({ length: count }, (_, index) => ({
      id: crypto.randomUUID(),
      schedule: schedules[0],
      state: count === 1 ? "cancelled" : "partial",
      createdAt: "2026-11-07T12:00:00Z",
      results:
        count === 1
          ? [
              {
                entryId: crypto.randomUUID(),
                title: "每日委托",
                state: "cancelled",
                stopState: "confirmed",
              },
            ]
          : [
              {
                entryId: crypto.randomUUID(),
                title: "每日委托",
                state: "succeeded",
              },
              {
                entryId: crypto.randomUUID(),
                title:
                  index === count - 1
                    ? "很长的任务标题".repeat(15)
                    : "领取奖励",
                state: "failed",
                error: "目标暂时不可用，下一个执行项仍会继续尝试。",
              },
            ],
    }));
    await page
      .getByRole("button", {
        name: `${schedules[0].name} 的更多操作`,
        exact: true,
      })
      .click();
    await page.getByRole("menuitem", { name: "运行记录", exact: true }).click();
    await historyDialog.locator(".schedule-occurrence").first().waitFor();
    await page.waitForTimeout(250);
    assert.equal(
      await historyDialog.locator(".schedule-occurrence").count(),
      count,
    );
    assert.ok((await historyDialog.boundingBox()).width <= 560);
    if (count === 1) {
      assert.ok((await historyDialog.boundingBox()).height < 300);
      assert.equal(
        await historyDialog.getByText("已停止", { exact: true }).count(),
        1,
      );
    }
    const layout = await historyDialog.evaluate((panel) => {
      const body = panel.querySelector(".sd-dialog-body");
      const first = body
        .querySelector(".schedule-occurrence")
        .getBoundingClientRect();
      return {
        inset: first.top - body.getBoundingClientRect().top,
        scrollable: body.scrollHeight > body.clientHeight,
      };
    });
    assert.ok(layout.inset >= 0 && layout.inset < 24);
    if (count > 2) assert.equal(layout.scrollable, true);
    await page.screenshot({
      path: path.join(scratch, `history-list-${theme}-${count}.png`),
    });
    if (count > 2) {
      for (const width of [1456, 360]) {
        await page.setViewportSize({ width, height: 876 });
        const bottom = await historyDialog
          .locator(".sd-dialog-body")
          .evaluate((body) => {
            body.scrollTop = body.scrollHeight;
            const last = body.lastElementChild.getBoundingClientRect();
            const bounds = body.getBoundingClientRect();
            return {
              visible: last.bottom <= bounds.bottom && last.bottom > bounds.top,
              overflow: body.scrollWidth > body.clientWidth,
            };
          });
        assert.equal(bottom.visible, true);
        assert.equal(bottom.overflow, false);
        await page.screenshot({
          path: path.join(scratch, `history-bottom-${theme}-${width}.png`),
        });
      }
    }
    await historyDialog
      .getByRole("button", { name: "关闭", exact: true })
      .click();
    await historyDialog.waitFor({ state: "hidden" });
  }
  historyFixture = [];
  await page.setViewportSize({ width: 1456, height: 876 });
  report.historyEmptyCentered = true;
  report.historyListScroll = true;
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .getByRole("button", { name: "每日委托 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "安排时间", exact: true }).click();
  await page
    .getByRole("dialog", { name: "新建定时任务", exact: true })
    .waitFor();
  assert.deepEqual(await names(), ["每日委托"]);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  await openScheduleEditor();
  await edit.getByRole("button", { name: "默认", exact: true }).click();
  await edit
    .getByRole("button", { name: "添加一份 关闭游戏", exact: true })
    .click();
  await edit
    .getByRole("button", { name: "添加一份 关闭工具软件", exact: true })
    .click();
  await edit.getByRole("button", { name: "保存更改", exact: true }).click();
  await edit.waitFor({ state: "hidden" });
  assert.deepEqual(
    schedules[0].entries.slice(-2).map((entry) => entry.binding),
    [{ kind: "closeGame" }, { kind: "closeToolSoftware" }],
  );
  await openScheduleEditor();
  assert.deepEqual(
    (await edit.locator(".schedule-entry-title").allTextContents()).slice(-2),
    ["关闭游戏", "关闭工具软件"],
  );
  await edit.getByRole("button", { name: "取消", exact: true }).click();
  await edit.waitFor({ state: "hidden" });
  report.defaultActionsSavedAndRestored = true;
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page.locator(".task-card-name").filter({ hasText: "每日委托" }).click();
  const details = page.getByRole("complementary", {
    name: "快捷任务",
    exact: true,
  });
  await details
    .getByRole("button", { name: "每日委托 的更多操作", exact: true })
    .click();
  await details
    .getByRole("menuitem", { name: "安排时间", exact: true })
    .click();
  await dialog.waitFor();
  assert.deepEqual(await names(), ["每日委托"]);
  report.detailsShortcut = true;
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  libraryFixture = [
    {
      ...actions[0],
      title: "跑千星脚本",
      content: "后台启动 BetterGI「千星」配置组，运行千星奇域每周脚本",
    },
  ];
  await page.getByRole("button", { name: "新建定时任务", exact: true }).click();
  await dialog
    .getByRole("button", { name: "添加 跑千星脚本", exact: true })
    .waitFor();
  for (const width of [1456, 1000, 360]) {
    await page.setViewportSize({ width, height: width === 1456 ? 876 : 760 });
    await dialog.getByRole("button", { name: "指定日期", exact: true }).click();
    await page.waitForTimeout(250);
    const layout = await dialog.evaluate((panel) => {
      const rect = (node) => {
        const r = node.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      };
      const date = rect(panel.querySelector(".schedule-date-field"));
      const time = rect(panel.querySelector(".schedule-time-input"));
      const tabs = rect(panel.querySelector('[aria-label="重复方式"]'));
      const settings = panel.querySelector(".schedule-settings");
      const style = getComputedStyle(settings);
      return {
        date,
        time,
        tabs,
        settingsInnerWidth:
          settings.clientWidth -
          parseFloat(style.paddingLeft) -
          parseFloat(style.paddingRight),
        mobileAdd: getComputedStyle(panel.querySelector(".schedule-mobile-add"))
          .display,
        libraryCount: panel.querySelectorAll(".schedule-library-row").length,
        queueBorder: getComputedStyle(panel.querySelector(".schedule-queue"))
          .borderTopWidth,
      };
    });
    assert.ok(
      Math.abs(layout.date.width - layout.time.width) < 1,
      JSON.stringify(layout),
    );
    if (width > 380) {
      assert.ok(
        Math.abs(layout.date.y - layout.time.y) < 1 &&
          layout.time.x >= layout.date.x + layout.date.width,
        JSON.stringify(layout),
      );
      assert.ok(layout.date.width > 140, JSON.stringify(layout));
    } else {
      assert.ok(
        layout.time.y >= layout.date.y + layout.date.height &&
          Math.abs(layout.time.x - layout.date.x) < 1,
        JSON.stringify(layout),
      );
    }
    assert.ok(
      Math.abs(layout.tabs.width - layout.settingsInnerWidth) < 2,
      JSON.stringify(layout),
    );
    if (width > 620) assert.equal(layout.mobileAdd, "none");
    else assert.notEqual(layout.mobileAdd, "none");
    assert.equal(layout.libraryCount, 1);
    assert.equal(layout.queueBorder, "0px");
    await dialog
      .getByRole("button", { name: "选择执行日期", exact: true })
      .click();
    const calendar = page.getByRole("group", {
      name: "选择执行日期日历",
      exact: true,
    });
    await calendar.waitFor();
    await page.waitForTimeout(220);
    assert.equal(
      await dialog
        .getByRole("button", { name: "选择执行日期", exact: true })
        .evaluate((node) => getComputedStyle(node).cursor),
      "pointer",
    );
    const dateButtons = calendar.getByRole("button");
    assert.ok((await dateButtons.count()) >= 30);
    await page.screenshot({
      path: path.join(scratch, `date-picker-${theme}-${width}.png`),
    });
    await page.keyboard.press("Escape");
    await calendar.waitFor({ state: "hidden" });
    assert.equal(await dialog.isVisible(), true);
    await dialog
      .getByRole("button", { name: "选择执行日期", exact: true })
      .click();
    const monthLabel = await calendar.locator("strong").textContent();
    await calendar.getByRole("button", { name: "下个月", exact: true }).click();
    assert.notEqual(await calendar.locator("strong").textContent(), monthLabel);
    await calendar.getByRole("button", { name: "上个月", exact: true }).click();
    const date = new Date(Date.now() + 86400000 * 2);
    const chosen = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    await calendar.getByRole("button", { name: chosen, exact: true }).click();
    assert.equal(
      await dialog.getByLabel("执行日期", { exact: true }).inputValue(),
      chosen,
    );
    const dateInput = dialog.getByLabel("执行日期", { exact: true });
    await dateInput.click();
    await calendar.waitFor();
    assert.equal(await dateInput.getAttribute("readonly"), "");
    await dateInput.dblclick({ delay: 70 });
    await calendar.waitFor({ state: "hidden" });
    assert.equal(await dateInput.isEditable(), true);
    if (width === 1456)
      await page.screenshot({
        path: path.join(scratch, `date-input-${theme}.png`),
      });
    assert.equal(
      await dateInput.evaluate(
        (node) =>
          node === document.activeElement &&
          node.selectionEnd - node.selectionStart === node.value.length,
      ),
      true,
    );
    await dateInput.fill(`${new Date().getFullYear() + 1}/2/3`);
    await dateInput.press("Enter");
    assert.equal(
      await dateInput.inputValue(),
      `${new Date().getFullYear() + 1}-02-03`,
    );
    assert.equal(await dateInput.getAttribute("readonly"), "");
    let leapYear = Math.ceil((new Date().getFullYear() + 1) / 4) * 4;
    if (leapYear % 100 === 0 && leapYear % 400 !== 0) leapYear += 4;
    await fillTiming(dateInput, `${leapYear}-02-29`);
    await dateInput.press("Enter");
    assert.equal(await dateInput.getAttribute("aria-invalid"), null);
    await fillTiming(dateInput, `${leapYear + 1}-02-29`);
    await dateInput.press("Enter");
    assert.equal(await dateInput.getAttribute("aria-invalid"), "true");
    await dateInput.press("Escape");
    assert.equal(await dateInput.inputValue(), `${leapYear}-02-29`);
    await fillTiming(dateInput, chosen);
    await dateInput.press("Enter");
    await fillTiming(dateInput, "2000-01-01");
    await dateInput.press("Enter");
    assert.equal(await dateInput.getAttribute("aria-invalid"), "true");
    await dateInput.press("Escape");
    assert.equal(await dateInput.inputValue(), chosen);
    assert.equal(await dialog.isVisible(), true);
    report.dateClickAndDoubleClick = true;
    if (width === 1456) {
      await dialog
        .getByRole("button", { name: "添加一份 跑千星脚本", exact: true })
        .click();
      await fillTiming(dateInput, `${new Date().getFullYear() + 1}-02-30`);
      const count = calls.filter(
        (call) => call.method === "scheduler.preview",
      ).length;
      await dialog
        .getByRole("button", { name: "创建并启用", exact: true })
        .click();
      assert.equal(
        calls.filter((call) => call.method === "scheduler.preview").length,
        count,
      );
      await page.getByRole("alert").filter({ hasText: "日期和时间" }).waitFor();
      await fillTiming(dateInput, chosen);
      await dialog
        .getByRole("button", { name: "移除第 1 项 跑千星脚本", exact: true })
        .click();
    }
    await page.screenshot({
      path: path.join(scratch, `single-${theme}-${width}.png`),
    });
  }
  report.theme = theme;
  report.singleTaskLayout = true;
  await page.setViewportSize({ width: 1000, height: 760 });
  await dialog.evaluate((panel) => {
    panel.style.transform = "translateY(4px) scale(0.99)";
  });
  await dialog.getByRole("button", { name: "选择时刻 1", exact: true }).click();
  await page.waitForTimeout(220);
  const anchorBox = await dialog
    .locator(".schedule-time-field")
    .first()
    .boundingBox();
  const popupBox = await page.locator(".schedule-wheel").boundingBox();
  assert.ok(
    Math.abs(anchorBox.x - popupBox.x) < 1,
    "父弹窗运动中，拨轮不得发生坐标叠加",
  );
  await page.keyboard.press("Escape");
  await page.waitForTimeout(220);
  const movingSource = await dialog
    .getByRole("button", { name: "添加 跑千星脚本", exact: true })
    .boundingBox();
  await page.mouse.move(movingSource.x + 60, movingSource.y + 20);
  await page.mouse.down();
  await page.mouse.move(movingSource.x + 80, movingSource.y + 20);
  await page.evaluate(() => new Promise(requestAnimationFrame));
  const movingGhost = await page.locator(".schedule-drag-ghost").boundingBox();
  report.transformedParentDragDrift = Math.abs(
    movingGhost.x - movingSource.x - 20,
  );
  assert.ok(
    report.transformedParentDragDrift < 1,
    "父弹窗运动中，拖拽影像仍需贴合抓取点",
  );
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await dialog.evaluate((panel) => {
    panel.style.transform = "none";
  });
  libraryFixture = [
    {
      ...actions[0],
      title: `长标题${"千星奇域任务".repeat(12)}`,
      content: "已有配置组的较长说明。".repeat(16),
    },
  ];
  await dialog.getByRole("textbox", { name: "搜索标题或内容" }).fill("长标题");
  await dialog
    .getByRole("button", {
      name: `添加一份 ${libraryFixture[0].title}`,
      exact: true,
    })
    .click();
  const longLayout = await dialog.evaluate((panel) => {
    const library = panel.querySelector(".schedule-library-row");
    const queue = panel.querySelector(".schedule-queue-row");
    const bounds = (node) => {
      const r = node.getBoundingClientRect();
      return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
    };
    return {
      library: bounds(library),
      libraryParent: bounds(library.parentElement),
      queue: bounds(queue),
      queueParent: bounds(queue.parentElement),
      titleOverflow: getComputedStyle(
        queue.querySelector(".schedule-entry-title"),
      ).textOverflow,
    };
  });
  assert.ok(longLayout.library.right <= longLayout.libraryParent.right + 1);
  assert.ok(longLayout.queue.right <= longLayout.queueParent.right + 1);
  assert.equal(longLayout.titleOverflow, "ellipsis");
  await page.screenshot({ path: path.join(scratch, `long-${theme}.png`) });
  report.longContentLayout = true;
  report.engine = native ? "WebView2" : "Edge";
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  const discard = page.getByRole("dialog", { name: "放弃更改？", exact: true });
  await discard.waitFor();
  for (const width of [1456, 1000, 360]) {
    await page.setViewportSize({ width, height: 876 });
    await page.waitForTimeout(250);
    const bounds = await discard.boundingBox();
    assert.ok(bounds.width <= 400 && bounds.width <= width - 48);
    assert.ok(bounds.height < 260);
    await page.screenshot({
      path: path.join(scratch, `discard-${theme}-${width}.png`),
    });
  }
  await discard.getByRole("button", { name: "取消", exact: true }).click();
  await discard.waitFor({ state: "hidden" });
  assert.equal(await dialog.isVisible(), true);
  await page.setViewportSize({ width: 1456, height: 876 });
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await discard.getByRole("button", { name: "放弃更改", exact: true }).click();
  report.compactConfirmation = true;
  await dialog.waitFor({ state: "hidden" });
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .getByRole("button", { name: "每日委托 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "删除任务", exact: true }).click();
  const remove = page.getByRole("dialog", {
    name: "删除快捷任务",
    exact: true,
  });
  await remove
    .getByText("永久删除「每日委托」及其全部修订？", { exact: true })
    .waitFor();
  await page.waitForTimeout(220);
  await page.screenshot({
    path: path.join(scratch, `delete-task-${theme}.png`),
  });
  assert.ok((await remove.boundingBox()).width <= 400);
  await remove.locator(".sd-dialog-body p").evaluate((node) => {
    node.textContent = `永久删除「${"很长的任务名称".repeat(20)}」及其全部修订？`;
  });
  await page.setViewportSize({ width: 360, height: 760 });
  await page.screenshot({
    path: path.join(scratch, `confirm-long-${theme}.png`),
  });
  assert.equal(
    await remove.evaluate((node) => node.scrollWidth <= node.clientWidth),
    true,
  );
  const deleteBounds = await remove
    .getByRole("button", { name: "删除", exact: true })
    .boundingBox();
  assert.ok(deleteBounds.x >= 0 && deleteBounds.x + deleteBounds.width <= 360);
  await remove.getByRole("button", { name: "删除", exact: true }).click();
  await page
    .getByRole("heading", { name: "还没有快捷任务", exact: true })
    .waitFor();
  assert.ok(
    calls.some(
      (call) => call.method === "workflow.delete" && call.params.id === task.id,
    ),
  );
  report.shortcutDeleteUi = true;
  assert.deepEqual(errors, []);
  report.errors = errors;
  passed = true;
  console.log(JSON.stringify(report));
} catch (error) {
  if (browser) {
    const pages = browser.contexts().flatMap((c) => c.pages());
    if (pages[0]) {
      if (native)
        console.error(
          "Native probe state",
          await pages[0].evaluate(() => ({
            receiver: typeof window.__sleepyDollReceive,
            ipc: typeof window.ipc,
            location: location.href,
          })),
          "handled",
          calls.map((call) => call.method),
        );
      await pages[0]
        .screenshot({ path: path.join(scratch, "failure.png") })
        .catch(() => {});
      console.error((await pages[0].locator("body").innerText()).slice(-6000));
    }
  }
  throw error;
} finally {
  if (native && browser)
    await browser
      .contexts()[0]
      ?.pages()[0]
      ?.evaluate(
        (success) =>
          window.chrome.webview.postMessage(
            JSON.stringify({ method: "probe.done", success }),
          ),
        passed,
      )
      .catch(() => {});
  await browser?.close();
  if (nativeIpc) await new Promise((resolve) => nativeIpc.close(resolve));
  if (production)
    await new Promise((resolve) => server?.httpServer.close(resolve));
  else await server?.close();
}
