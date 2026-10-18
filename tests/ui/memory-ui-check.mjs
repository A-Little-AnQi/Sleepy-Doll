import "./runtime.mjs";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = fileURLToPath(new URL("../../", import.meta.url));
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json")),
).version;
const captureRoot = process.env.SLEEPY_DOLL_UI_CAPTURE_DIR ?? process.env.TEMP;
let browser, server, page;
let enabled = true,
  failToggle = false,
  failList = false;
let rows = [
  {
    compatible: true,
    entry: {
      id: "memory-1",
      revision: 1,
      userEdited: false,
      validationState: "candidate",
      sourceConversationId: "original",
      updatedAt: "2026-10-17T00:00:00Z",
      experience: {
        intent: "配置采集路线",
        summary: "先读取当前路线，再配置队伍并回读结果。",
        keywords: ["采集", "路线"],
        conditions: ["当前工具提供采集配置能力"],
        steps: [
          {
            tool: "fixture.configure",
            methodId: null,
            action: "读取当前配置后修改",
            inputRoles: {},
          },
        ],
        verification: ["回读实际生效的队伍"],
        pitfalls: ["配置变化后重新读取"],
        anchors: [],
      },
    },
  },
];
let releaseStateRead;
let holdNextState = false;
const requests = [],
  errors = [];
const bootstrap = {
  configPath: "fixture/config.json",
  models: [
    {
      id: "fixture",
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
try {
  server = await createServer({
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false },
  });
  await server.listen();
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  });
  page = await browser.newPage({
    viewport: { width: 1400, height: 900 },
  });
  page.setDefaultTimeout(6000);
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript((version) => {
    localStorage.setItem("sleepy-doll-version", version);
    localStorage.setItem("sleepy-doll-theme", "dark");
    localStorage.setItem("sleepy-doll-locale", "zh");
  }, version);
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    requests.push(q);
    let result = {};
    let error;
    switch (q.method) {
      case "bootstrap":
        result = bootstrap;
        break;
      case "task.list":
        result = [];
        break;
      case "release.state":
        result = {};
        break;
      case "memory.state":
        result = { enabled };
        if (holdNextState) {
          holdNextState = false;
          await new Promise((resolve) => {
            releaseStateRead = resolve;
          });
        }
        break;
      case "memory.setEnabled":
        if (failToggle) error = { message: "保存开关失败" };
        else {
          enabled = q.params.enabled;
          result = { enabled };
        }
        break;
      case "memory.list":
        if (failList) error = { message: "读取记忆失败" };
        else result = { entries: rows };
        break;
      case "memory.update":
        assert.equal(q.params.revision, rows[0].entry.revision);
        rows[0].entry = {
          ...rows[0].entry,
          experience: q.params.experience,
          revision: q.params.revision + 1,
        };
        break;
      case "memory.delete":
        assert.equal(q.params.revision, rows[0].entry.revision);
        rows = rows.filter((row) => row.entry.id !== q.params.id);
        break;
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: !error, result, error }),
    });
  });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  await page
    .locator(".app-nav")
    .getByRole("button", { name: "记忆", exact: true })
    .click();
  const memory = page.locator(".memory-page");
  await memory.getByRole("heading", { name: "记忆", exact: true }).waitFor();
  await memory
    .getByText("先读取当前路线，再配置队伍并回读结果。", { exact: true })
    .waitFor();
  assert.equal(
    await page.locator(".details-panel").count(),
    0,
    "Memory must not use the shortcut sidebar",
  );
  const toggle = () =>
    page.getByRole("switch", { name: "启用记忆", exact: true });
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "true",
  );
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "false",
  );
  assert.equal(enabled, false);
  assert.equal(
    await memory.locator(".memory-entry").count(),
    1,
    "Disabled memory remains manageable",
  );
  // Both entry points control the same persisted switch.
  await page.evaluate(() =>
    window.__sleepyDollReceive({ kind: "event", id: "openSettings" }),
  );
  await page.getByRole("button", { name: "管理记忆", exact: true }).waitFor();
  assert.equal(
    await page.locator(".settings-general .memory-entry").count(),
    0,
  );
  assert.equal(await toggle().getAttribute("aria-checked"), "false");
  assert.equal(
    await page.getByRole("button", { name: "刷新", exact: true }).count(),
    0,
  );
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "true",
  );
  await page.screenshot({
    path: path.join(captureRoot, "settings.png"),
  });
  holdNextState = true;
  await page.getByRole("button", { name: "管理记忆", exact: true }).click();
  await memory.waitFor();
  assert.equal(await toggle().getAttribute("aria-checked"), "true");
  while (!releaseStateRead) await page.waitForTimeout(10);
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "false",
  );
  releaseStateRead();
  await page.waitForTimeout(150);
  assert.equal(
    await toggle().getAttribute("aria-checked"),
    "false",
    "A stale read must not undo a newer saved choice",
  );
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "true",
  );
  failToggle = true;
  await toggle().click();
  await memory.getByRole("alert").filter({ hasText: "保存开关失败" }).waitFor();
  assert.equal(await toggle().getAttribute("aria-checked"), "true");
  failToggle = false;
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "false",
  );
  await toggle().click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[role="switch"]')
        ?.getAttribute("aria-checked") === "true",
  );
  await memory.getByRole("textbox", { name: "搜索记忆" }).fill("不存在");
  await memory.getByText("没有找到相关记忆", { exact: true }).waitFor();
  await memory.getByRole("textbox", { name: "搜索记忆" }).fill("");
  await memory.locator(".memory-entry > summary").click();
  await memory.getByRole("button", { name: "编辑记忆", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "编辑记忆", exact: true });
  await editor
    .getByRole("textbox", { name: "简介（最多 240 字）", exact: true })
    .fill("");
  assert.equal(
    await editor
      .getByRole("button", { name: "保存", exact: true })
      .isDisabled(),
    true,
  );
  await editor
    .getByLabel("简介（最多 240 字）", { exact: true })
    .fill("根据新事实更新过的简介");
  await editor.getByRole("button", { name: "保存", exact: true }).click();
  await memory.getByText("根据新事实更新过的简介", { exact: true }).waitFor();
  await memory.getByRole("button", { name: "忘记这条", exact: true }).click();
  assert.equal(requests.filter((q) => q.method === "memory.delete").length, 0);
  const confirm = page.getByRole("dialog", {
    name: "忘记这条记忆？",
    exact: true,
  });
  await confirm.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(rows.length, 1);
  await page.setViewportSize({ width: 700, height: 780 });
  await page.waitForTimeout(350);
  assert(
    await memory.evaluate((node) => node.scrollWidth <= node.clientWidth + 1),
    "Narrow memory page overflows",
  );
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForTimeout(350);
  await page.screenshot({
    path: path.join(captureRoot, "memory.png"),
  });
  await memory.getByRole("button", { name: "忘记这条", exact: true }).click();
  await confirm.getByRole("button", { name: "忘记这条", exact: true }).click();
  await memory
    .getByRole("heading", { name: "还没有记忆", exact: true })
    .waitFor();
  failList = true;
  await memory.getByRole("button", { name: "刷新", exact: true }).click();
  await memory.getByRole("alert").filter({ hasText: "读取记忆失败" }).waitFor();
  assert.deepEqual(errors, []);
  assert.equal(requests.filter((q) => q.method === "task.submit").length, 0);
  console.log(
    JSON.stringify({
      navigation: true,
      settingsOnlyControls: true,
      persistedToggle: true,
      lateStateReadCannotRevertToggle: true,
      toggleFailureRollback: true,
      introductionList: true,
      search: true,
      edit: true,
      confirmedForget: true,
      emptyAndErrorStates: true,
      narrowLayout: true,
      noModelCalls: true,
    }),
  );
} catch (error) {
  if (page) {
    console.error(await page.locator("body").innerText());
    await page.screenshot({
      path: path.join(captureRoot, "memory-error.png"),
    });
  }
  throw error;
} finally {
  await browser?.close();
  await server?.close();
}
