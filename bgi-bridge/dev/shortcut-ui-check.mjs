// Product-level shortcut regression with fake IPC. No model or BetterGI calls.
import assert from "node:assert/strict";
import path from "node:path";
import os from "node:os";
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
    server: { host: "127.0.0.1", port: 0, hmr: false },
  });
  await server.listen();
  const port = server.httpServer.address().port;
  console.log(
    JSON.stringify({
      port,
      cleanup: "finally closes Vite and Chromium",
      screenshot: path.join(os.tmpdir(), "sleepy-doll-shortcut-ui.png"),
    }),
  );
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1400, height: 900 },
  });
  page.setDefaultTimeout(5000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let workflows = [
    {
      id: "chosen",
      name: "血斛采集",
      description: "运行已配置的血斛路线",
      state: "readyUnverified",
      stateLabel: "可运行 · 尚未实机验证",
      actionLabel: "运行",
      runnable: true,
      pinned: false,
      sourceConversationId: "chat-a",
      publishedRevision: 1,
      modelUsage: "none",
      zeroToken: true,
      nodeCount: 2,
      updatedAt: "2026-09-27T00:00:00Z",
      shortcut: {
        applicationName: "BetterGI",
        targetName: "血斛路线组",
        prepare: [],
        action: { tool: "fixture.run", arguments: { name: "血斛路线组" } },
      },
    },
  ];
  let runs = [],
    events = [];
  const requests = [];
  let configurationCount = 0;
  const conversations = [
    { id: "chat-a", title: "原配置对话" },
    { id: "chat-b", title: "另一条对话" },
  ].map((item) => ({
    ...item,
    createdAt: "2026-09-27T00:00:00Z",
    updatedAt: "2026-09-27T00:00:00Z",
  }));
  const bootstrap = () => ({
    configPath: "fixture",
    models: [
      {
        id: "model",
        name: "Fixture",
        model: "fixture",
        baseUrl: "fixture",
        protocol: "openai",
        active: true,
      },
    ],
    skills: [],
    tools: [],
    runtimeToolLabels: {},
    plugins: [],
    conversations,
    tasks: runs,
    strategies: [],
    workflows,
    operations: [],
    resources: [],
    diagnostics: [],
    notifications: [],
    conversationGroups: { groups: [], membership: {}, order: [] },
    permission: { mode: "fullAccess", label: "允许执行", levels: [] },
    bridge: { enabled: false, connected: false, baseUrl: "fixture" },
  });
  await page.addInitScript(() => {
    localStorage.setItem("sleepy-doll-version", "0.1.0");
    localStorage.setItem("sleepy-doll-locale", "zh");
    localStorage.setItem("sleepy-doll-theme", "dark");
    localStorage.setItem("sleepy-doll-details-open", "true");
  });
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    requests.push(q);
    let result = {};
    if (q.method === "bootstrap") result = bootstrap();
    if (q.method === "task.list") result = runs;
    if (q.method === "conversation.get")
      result = { id: q.params.id, messages: [] };
    if (q.method === "events.read")
      result = {
        events: events.filter(
          (e) =>
            e.conversationId === q.params.conversationId &&
            e.sequence > (q.params.after ?? 0),
        ),
      };
    if (q.method === "workflow.run") {
      result = {
        id: "task-run",
        conversationId: "task-task-run",
        prompt: "血斛采集",
        state: "executing",
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        source: {
          kind: "savedWorkflow",
          workflowId: "chosen",
          workflowRevision: 1,
        },
      };
      runs = [result];
      events.push({
        sequence: 1,
        runId: result.id,
        conversationId: result.conversationId,
        kind: "run.created",
        data: result,
      });
    }
    if (q.method === "task.cancel") {
      const run = runs.find((run) => run.id === q.params.id);
      if (run) {
        run.state = "cancelled";
        events.push({
          sequence: events.length + 1,
          runId: run.id,
          conversationId: run.conversationId,
          kind: "run.changed",
          data: run,
        });
      }
      result = { cancelled: true };
    }
    if (q.method === "shortcut.configure") {
      result = {
        id: `configuration-run-${++configurationCount}`,
        conversationId: q.params.conversationId ?? "shortcut-config-fixture",
        prompt: q.params.prompt,
        state: "deciding",
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      runs = [...runs.filter((run) => run.id !== result.id), result];
      events.push({
        sequence: events.length + 1,
        runId: result.id,
        conversationId: result.conversationId,
        kind: "run.created",
        data: result,
      });
    }
    if (q.method === "shortcut.accept") {
      workflows = workflows.map((task) =>
        task.id === "chosen"
          ? { ...task, name: q.params.name, description: q.params.description }
          : task,
      );
      result = { taskId: "chosen" };
    }
    if (q.method === "run.input") {
      const run = runs.find((run) => run.id === q.params.id);
      run.state = "deciding";
      events.push({
        sequence: events.length + 1,
        runId: run.id,
        conversationId: run.conversationId,
        kind: "run.changed",
        data: { ...run },
      });
    }
    if (q.method === "workflow.rename") {
      workflows = workflows.map((task) =>
        task.id === q.params.id ? { ...task, name: q.params.name } : task,
      );
      result = { updated: true };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });
  await page.goto(`http://127.0.0.1:${port}`);
  const shelf = page.locator(".details-panel");
  await shelf.getByRole("button", { name: "血斛采集", exact: true }).waitFor();
  await page
    .locator('[data-sd-id="chat-b"]')
    .locator(":scope > button")
    .click();
  await shelf.getByRole("button", { name: "血斛采集", exact: true }).waitFor();
  assert.equal(await shelf.getByText("原配置对话", { exact: true }).count(), 0);
  const before = requests.filter(
    (q) => q.method === "task.submit" || q.method === "shortcut.configure",
  ).length;
  await shelf.getByRole("button", { name: "运行", exact: true }).click();
  await shelf.getByRole("button", { name: "停止", exact: true }).waitFor();
  assert.equal(
    requests.filter(
      (q) => q.method === "task.submit" || q.method === "shortcut.configure",
    ).length,
    before,
  );
  assert.equal(
    await page.locator('.app-nav button[aria-current="page"]').count(),
    0,
  );
  assert.equal(
    await page
      .locator('[data-sd-id="chat-b"] > button')
      .getAttribute("aria-current"),
    "page",
  );
  await shelf.getByRole("button", { name: "停止", exact: true }).click();
  await shelf.getByRole("button", { name: "运行", exact: true }).waitFor();
  assert(
    requests.some(
      (q) => q.method === "task.cancel" && q.params.id === "task-run",
    ),
  );
  await shelf
    .getByRole("button", { name: "血斛采集 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: "重命名快捷任务" });
  await dialog.getByRole("textbox").fill("每日血斛");
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  await shelf.getByRole("button", { name: "每日血斛", exact: true }).waitFor();
  await page.waitForTimeout(350);
  await shelf
    .getByRole("button", { name: "每日血斛 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "调整入口", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "调整「每日血斛」的入口" });
  assert.equal(
    await dialog
      .locator(
        ".chat-workspace,.conversation-scene,.composer-dock,.context-meter",
      )
      .count(),
    0,
    "Wrapping must use a form, never a chat",
  );
  await dialog
    .getByRole("textbox", { name: "要加入哪一项", exact: true })
    .fill("封装已有的血斛采集配置组");
  await dialog.getByRole("button", { name: "识别已有项", exact: true }).click();
  await dialog.getByText("正在定位已有项", { exact: true }).waitFor();
  const config = requests.find((q) => q.method === "shortcut.configure");
  assert.equal(config.params.shortcutId, "chosen");
  assert.equal(config.params.referenceConversationId, "chat-b");
  assert.equal(config.params.prompt, "封装已有的血斛采集配置组");
  assert.equal(requests.filter((q) => q.method === "task.submit").length, 0);
  const current = runs.find((run) => run.id === "configuration-run-1");
  current.state = "awaitingUser";
  events.push({
    sequence: events.length + 1,
    runId: current.id,
    conversationId: current.conversationId,
    kind: "question",
    data: { question: "要封装哪一个已有组？" },
  });
  events.push({
    sequence: events.length + 1,
    runId: current.id,
    conversationId: current.conversationId,
    kind: "run.changed",
    data: { ...current },
  });
  await dialog
    .getByRole("textbox", { name: "补充信息", exact: true })
    .fill("血斛路线组");
  await dialog.getByRole("button", { name: "补充并继续", exact: true }).click();
  assert(
    requests.some(
      (q) => q.method === "run.input" && q.params.id === current.id,
    ),
  );
  const proposal = {
    id: "chosen",
    name: "已有血斛入口",
    description: "运行现有的血斛路线组",
    binding: workflows[0].shortcut,
  };
  events.push({
    sequence: events.length + 1,
    runId: current.id,
    conversationId: current.conversationId,
    kind: "shortcut.proposed",
    data: proposal,
  });
  current.state = "answered";
  events.push({
    sequence: events.length + 1,
    runId: current.id,
    conversationId: current.conversationId,
    kind: "run.changed",
    data: { ...current },
  });
  await dialog
    .getByRole("textbox", { name: "入口名称", exact: true })
    .waitFor();
  assert.equal(workflows[0].name, "每日血斛", "Preview must not silently save");
  assert.equal(
    requests.filter((q) => q.method === "shortcut.accept").length,
    0,
  );
  assert.equal(
    await dialog.getByText("fixture.run", { exact: false }).count(),
    0,
  );
  await dialog
    .getByRole("textbox", { name: "用途说明", exact: true })
    .fill("一键运行已有配置");
  await page.waitForTimeout(300);
  await page.screenshot({
    path: path.join(os.tmpdir(), "sleepy-doll-shortcut-ui.png"),
  });
  await dialog.getByRole("button", { name: "保存入口", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  await shelf
    .getByRole("button", { name: "已有血斛入口", exact: true })
    .waitFor();
  const accepted = requests.find((q) => q.method === "shortcut.accept");
  assert.equal(accepted.params.runId, current.id);
  assert.equal(accepted.params.description, "一键运行已有配置");
  await shelf.getByRole("button", { name: "添加", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "添加快捷任务", exact: true });
  assert.equal(await dialog.locator(".chat-workspace").count(), 0);
  assert.equal(
    await dialog
      .getByRole("textbox", { name: "要加入哪一项", exact: true })
      .inputValue(),
    "",
  );
  await dialog
    .getByRole("textbox", { name: "要加入哪一项", exact: true })
    .fill("定位另一个已有项");
  await dialog.getByRole("button", { name: "识别已有项", exact: true }).click();
  await dialog.getByText("正在定位已有项", { exact: true }).waitFor();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  assert(
    requests.some(
      (q) =>
        q.method === "task.cancel" && q.params.id === "configuration-run-2",
    ),
  );
  workflows = [
    ...workflows,
    {
      ...workflows[0],
      id: "mint",
      name: "薄荷采集",
      lastRunId: undefined,
      shortcut: { ...workflows[0].shortcut, targetName: "薄荷路线组" },
    },
  ];
  events.push({
    sequence: events.length + 1,
    runId: "chat-save",
    conversationId: "chat-b",
    kind: "shortcut.saved",
    data: { taskId: "mint" },
  });
  await shelf.getByRole("button", { name: "薄荷采集", exact: true }).waitFor();
  await page
    .locator(".app-nav")
    .getByRole("button", { name: /^快捷任务/ })
    .click();
  await page
    .locator(".tasks-page")
    .getByRole("button", { name: "已有血斛入口", exact: true })
    .waitFor();
  assert.equal(
    await shelf
      .getByRole("button", { name: "已有血斛入口", exact: true })
      .count(),
    1,
  );
  await page
    .locator(".tasks-page")
    .getByRole("button", { name: "运行记录", exact: true })
    .click();
  assert.equal(await page.locator(".record").count(), 1);
  assert.equal(
    await page
      .locator(".record")
      .getByText("查看对话", { exact: true })
      .count(),
    0,
  );
  await page
    .locator(".tasks-page")
    .getByRole("button", { name: "已保存任务", exact: true })
    .click();
  await page
    .locator(".tasks-page")
    .getByRole("textbox", { name: "搜索快捷任务" })
    .fill("薄荷");
  assert.equal(await page.locator(".tasks-page .task-card").count(), 1);
  assert.equal(
    await page
      .locator(".tasks-page")
      .getByRole("button", { name: "已有血斛入口", exact: true })
      .count(),
    0,
  );
  await page
    .locator(".tasks-page")
    .getByRole("textbox", { name: "搜索快捷任务" })
    .fill("");
  await page.setViewportSize({ width: 980, height: 700 });
  await page.waitForTimeout(350);
  assert(
    await page
      .locator(".tasks-page")
      .evaluate((node) => node.scrollWidth <= node.clientWidth + 1),
    "Narrow task page overflows horizontally",
  );
  await shelf.getByRole("button", { name: "添加", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "添加快捷任务", exact: true });
  const bounds = await dialog.boundingBox();
  assert(
    bounds.x >= 0 &&
      bounds.y >= 0 &&
      bounds.x + bounds.width <= 980 &&
      bounds.y + bounds.height <= 700,
    "Configuration must fit the window",
  );
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForTimeout(1200);

  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify({
      globalShelf: true,
      chatSaveRefreshesShelf: true,
      searchBySelectedTask: true,
      narrowWindowLayout: true,
      runWithoutModelOrNavigation: true,
      stopExactRun: true,
      inlineRename: true,
      editSelectedTarget: true,
      noChatUiForWrapping: true,
      questionAsFormField: true,
      previewBeforeSave: true,
      closeCancelsConfiguration: true,
      addAndEditDraftsSeparated: true,
      onlyTaskRunsInHistory: true,
      pageErrors: errors,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
