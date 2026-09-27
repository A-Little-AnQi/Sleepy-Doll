// Sidebar regression with fake IPC; never deletes real conversations or calls BGI.
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
    JSON.stringify({ port, cleanup: "finally closes Vite and Chromium" }),
  );
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 760 },
  });
  const title = "帮我把植绒草那个配置组删一下，以后我不采了，留着碍事";
  let conversations = ["first", "second", "nested"].map((id) => ({
    id,
    title,
    createdAt: "2026-09-27T00:00:00Z",
    updatedAt: "2026-09-27T00:00:00Z",
  }));
  let mode = "fullAccess";
  let failPreview = false;
  const deletionRequests = [];
  const taskDeletionRequests = [];
  let workflows = [
    {
      id: "task",
      name: "保留的快捷任务",
      description: "fixture",
      state: "draft",
      stateLabel: "草稿",
      actionLabel: "查看",
      runnable: false,
      pinned: false,
      sourceConversationId: "second",
      sourceTitleSnapshot: title,
      sourceDeleted: false,
      modelUsage: "none",
      zeroToken: false,
      nodeCount: 1,
      updatedAt: "2026-09-27T00:00:00Z",
    },
  ];
  const bootstrap = () => ({
    configPath: "fixture/config.json",
    models: [],
    skills: [],
    tools: [],
    runtimeToolLabels: {},
    plugins: [],
    conversations,
    tasks: [],
    strategies: [],
    workflows,
    operations: [],
    resources: [],
    diagnostics: [],
    notifications: [],
    conversationGroups: {
      groups: [{ id: "group", name: "分组", collapsed: false }],
      membership: { nested: "group" },
      order: ["first", "second", "nested"],
    },
    permission: {
      mode,
      label: "允许执行",
      levels: [{ value: mode, label: mode, description: "fixture" }],
    },
    bridge: {
      enabled: false,
      connected: false,
      baseUrl: "fixture",
      launchSilently: true,
    },
  });
  await page.addInitScript(() => {
    localStorage.setItem("sleepy-doll-version", "0.1.0");
    localStorage.setItem("sleepy-doll-locale", "zh");
    localStorage.setItem("sleepy-doll-theme", "dark");
    localStorage.setItem("sleepy-doll-sidebar-width", "248");
  });
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    let result = {};
    if (request.method === "bootstrap") result = bootstrap();
    if (request.method === "workflow.list") result = workflows;
    if (request.method === "workflow.delete") {
      taskDeletionRequests.push(request.params);
      workflows = [];
      result = { deleted: true };
    }
    if (request.method === "conversation.get")
      result = { id: request.params.id, messages: [] };
    if (request.method === "events.read") result = { events: [] };
    if (request.method === "conversation.delete") {
      deletionRequests.push(request.params);
      if (failPreview && !request.params.confirmed) {
        failPreview = false;
        await route.fulfill({
          contentType: "application/json",
          body: JSON.stringify({
            id: request.id,
            ok: false,
            error: { message: "预览删除失败，请重试" },
          }),
        });
        return;
      }
      if (request.params.confirmed) {
        conversations = conversations.filter(
          (entry) => entry.id !== request.params.id,
        );
        result = { deleted: true };
      } else {
        result = {
          requiresConfirmation: true,
          affects: { title, taskCount: 2 },
          keeps: "快捷任务保留",
        };
      }
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.goto(`http://127.0.0.1:${port}`);
  const row = (id) =>
    page.locator(`[data-sd-slot="conversation"][data-sd-id="${id}"]`);
  await row("first").waitFor();
  const measure = (id) =>
    row(id).evaluate((node) => {
      const rect = (element) => {
        const box = element.getBoundingClientRect();
        return {
          x: box.x,
          y: box.y,
          width: box.width,
          height: box.height,
          right: box.right,
          bottom: box.bottom,
        };
      };
      return {
        row: rect(node),
        title: rect(node.querySelector(".app-conversation-title")),
        actions: rect(node.querySelector(".app-conversation-actions")),
        buttons: [
          ...node.querySelectorAll(".app-conversation-actions button"),
        ].map(rect),
      };
    });
  const geometryCases = [];
  for (const width of [248, 200, 480]) {
    await page.evaluate(
      (value) =>
        localStorage.setItem("sleepy-doll-sidebar-width", String(value)),
      width,
    );
    await page.reload();
    await row("first").waitFor();
    for (const id of ["first", "second", "nested"]) {
      await page.locator("textarea").hover();
      const before = await measure(id);
      await row(id).hover();
      await page.waitForTimeout(220);
      const after = await measure(id);
      if (
        width === 248 &&
        id === "first" &&
        process.env.SIDEBAR_SCREENSHOT_PATH
      ) {
        await page
          .locator(".app-conversations")
          .screenshot({ path: process.env.SIDEBAR_SCREENSHOT_PATH });
      }
      assert.equal(
        before.title.width,
        after.title.width,
        "hover changed title width",
      );
      assert.equal(
        before.row.height,
        after.row.height,
        "hover changed row height",
      );
      assert(after.title.right <= after.actions.x, "actions cover the title");
      for (const button of after.buttons) {
        assert(
          button.x >= after.row.x && button.right <= after.row.right,
          "action outside row horizontally",
        );
        assert(
          button.y >= after.row.y && button.bottom <= after.row.bottom,
          "action outside row vertically",
        );
      }
      geometryCases.push({
        width,
        id,
        gap: after.actions.x - after.title.right,
      });
    }
  }
  console.log(JSON.stringify({ geometryCases }));
  await row("first").hover();
  await row("first")
    .getByRole("button", { name: "删除对话", exact: true })
    .click();
  const dialog = page.getByRole("dialog", { name: "删除对话", exact: true });
  await dialog.waitFor({ timeout: 3000 });
  assert.equal(
    deletionRequests.filter((request) => request.confirmed).length,
    0,
    "clicked delete must only preview, even in fullAccess",
  );
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.waitForTimeout(300);
  assert.equal(await row("first").count(), 1);
  assert.equal(
    deletionRequests.filter((request) => request.confirmed).length,
    0,
  );
  await row("first").hover();
  await row("first")
    .getByRole("button", { name: "删除对话", exact: true })
    .click();
  await dialog.waitFor();
  await dialog.getByRole("button", { name: "删除对话", exact: true }).click();
  await row("first").waitFor({ state: "detached" });
  assert.equal(
    deletionRequests.filter((request) => request.confirmed).length,
    1,
  );
  assert.equal(await row("second").count(), 1);
  mode = "fullAccess";
  await page.reload();
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page
    .locator(".app-view")
    .getByRole("button", { name: "保留的快捷任务 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "删除任务", exact: true }).click();
  const taskDialog = page.getByRole("dialog", {
    name: "删除快捷任务",
    exact: true,
  });
  await taskDialog.waitFor();
  assert.equal(
    taskDeletionRequests.length,
    0,
    "task page bypassed confirmation",
  );
  await taskDialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.waitForTimeout(300);
  await row("second").locator(":scope > button").click();
  await page.locator("[data-details-trigger]").click();
  await page
    .locator(".details-panel")
    .getByRole("button", { name: "保留的快捷任务 的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "删除任务", exact: true }).click();
  await taskDialog.waitFor();
  assert.equal(
    taskDeletionRequests.length,
    0,
    "details panel bypassed confirmation",
  );
  await taskDialog
    .getByRole("button", { name: "删除任务", exact: true })
    .click();
  await page.waitForFunction(
    () => !document.querySelector(".details-panel .task-card"),
  );
  assert.equal(taskDeletionRequests.length, 1);
  assert.equal(await row("nested").count(), 1);
  for (const nextMode of ["askEach", "planOnly"]) {
    mode = nextMode;
    await page.reload();
    await row("nested").hover();
    await row("nested")
      .getByRole("button", { name: "删除对话", exact: true })
      .click();
    await dialog.waitFor();
    assert.equal(
      deletionRequests.filter((request) => request.confirmed).length,
      1,
    );
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
    assert.equal(await row("nested").count(), 1);
  }
  failPreview = true;
  await row("second").hover();
  await row("second")
    .getByRole("button", { name: "删除对话", exact: true })
    .click();
  await page.getByText("预览删除失败，请重试", { exact: true }).waitFor();
  assert.equal(
    deletionRequests.filter((request) => request.confirmed).length,
    1,
  );
  assert.equal(await row("second").count(), 1);
  console.log(
    JSON.stringify({
      deletion:
        "all permission modes confirm; cancel and Escape preserve chat; explicit confirmation deletes only the selected chat; preview errors visible",
      realDataTouched: false,
      taskDeletion:
        "task page and details panel also require explicit confirmation in fullAccess",
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
