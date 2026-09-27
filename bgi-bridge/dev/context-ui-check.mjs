// Real session, transcript and model settings, with fake IPC and no model calls.
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const fixture = `
import React from 'react';
import ReactDOM from 'react-dom/client';
const {useState}=React;
const {createRoot}=ReactDOM;
import {useSession,isRunning,phaseLabel,forgetSession} from '/src/session/index.ts';
import {Transcript} from '/src/components/chat/Transcript.tsx';
import {ContextMeter} from '/src/components/chat/ContextMeter.tsx';
import {ModelsPage} from '/src/pages/settings/ModelsPage.tsx';
import {suggestedContextWindow} from '/src/models/presets.ts';
import '/src/product.css';
function Fixture(){
 const [models,setModels]=useState(null);
 window.showModels=setModels;
 window.suggestedContextWindow=suggestedContextWindow;
 window.forgetFixture=()=>forgetSession('context-fixture');
 const snapshot=useSession(models ? undefined : 'context-fixture');
 window.contextSnapshot=snapshot;
 const bootstrap={models:models||[]};
 return React.createElement('div',{style:{padding:32,width:800}},models
   ? React.createElement(ModelsPage,{key:models.length,bootstrap,reload:async()=>{}})
   : React.createElement(React.Fragment,null,
     React.createElement(Transcript,{...snapshot,currentTask:snapshot.task,phase:phaseLabel(snapshot.task),running:isRunning(snapshot.task),seconds:1,toolLabels:{}}),
     React.createElement(ContextMeter,{used:50000,window:256000})));
}
createRoot(document.getElementById('root')).render(React.createElement(Fixture));
`;
let server;
let browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false },
  });
  await server.listen();
  const entry = await server.transformRequest("/src/main.tsx");
  const reactUrl = entry.code.match(/from "([^"]*\/react\.js\?[^"]*)"/)[1];
  const domUrl = entry.code.match(
    /from "([^"]*\/react-dom_client\.js\?[^"]*)"/,
  )[1];
  const port = server.httpServer.address().port;
  console.log(
    JSON.stringify({ port, cleanup: "finally closes Vite and Chromium" }),
  );
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 900 },
  });
  page.setDefaultTimeout(5000);
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem("sleepy-doll-locale", "zh");
    localStorage.setItem("sleepy-doll-theme", "dark");
    window.eventReadStarts = 0;
    const originalFetch = window.fetch;
    window.fetch = function (...args) {
      try {
        if (JSON.parse(args[1]?.body ?? "{}").method === "events.read")
          window.eventReadStarts++;
      } catch {}
      return originalFetch.apply(this, args);
    };
  });
  const task = {
    id: "run",
    conversationId: "context-fixture",
    prompt: "继续任务",
    state: "deciding",
    createdAt: "2026-09-27T00:00:00Z",
    updatedAt: "2026-09-27T00:00:01Z",
  };
  const event = (sequence, kind, data = {}) => ({
    sequence,
    kind,
    runId: "run",
    conversationId: "context-fixture",
    data: ["run.created", "run.changed"].includes(kind)
      ? { ...data, revision: sequence }
      : data,
  });
  const history = [
    { role: "user", content: "继续任务", runId: "run" },
    {
      role: "assistant",
      content: "先读取文件。",
      toolCalls: [
        { id: "read", name: "workspace.read", arguments: { path: "fixture" } },
      ],
      runId: "run",
      streamBoundary: 2,
    },
  ];
  const events = [
    event(1, "run.created", task),
    event(5, "context.compaction.started"),
  ];
  let catalogRequests = 0;
  let eventBatchLimit = 256;
  let eventReads = 0;
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    let result = {};
    if (request.method === "conversation.get") {
      const contextActivities = [];
      let contextActivityBoundary = 0;
      for (const item of events) {
        if (item.kind === "context.compaction.started") {
          contextActivityBoundary = item.sequence;
          contextActivities.push({
            id: item.sequence,
            runId: item.runId,
            state: "running",
          });
        } else if (
          item.kind === "context.compacted" &&
          item.data.mode === "summary"
        ) {
          contextActivityBoundary = item.sequence;
          contextActivities.findLast(
            (activity) => activity.state === "running",
          ).state = "completed";
        }
      }
      result = {
        id: "context-fixture",
        messages: history,
        runs: [
          events.findLast((item) =>
            ["run.created", "run.changed"].includes(item.kind),
          )?.data ?? task,
        ],
        contextActivities,
        contextActivityBoundary,
      };
    }
    if (request.method === "events.read") {
      eventReads++;
      result = {
        events: events
          .filter((item) => item.sequence > request.params.after)
          .slice(0, eventBatchLimit),
      };
    }
    if (request.method === "model.list") {
      catalogRequests++;
      result = {
        models: ["custom-model", "large-model"],
        contextWindows: { "custom-model": 1_000_000, "large-model": 1_000_000 },
      };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.route("**/__context_fixture.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(reactUrl))
        .replace("'react-dom/client'", JSON.stringify(domUrl)),
    }),
  );
  const html = await server.transformIndexHtml(
    "/__fixture",
    '<html data-theme="dark"><head></head><body><div id="root"></div><script type="module" src="/__context_fixture.js"></script></body></html>',
  );
  await page.route("**/__fixture", (route) =>
    route.fulfill({ contentType: "text/html", body: html }),
  );
  await page.goto(`http://127.0.0.1:${port}/__fixture`);
  const process = page.locator(".process-disclosure");
  await process.getByText("正在压缩上下文", { exact: true }).waitFor();
  assert.equal(
    await page.locator(".sd-context-hint").getByText(/压缩/).count(),
    0,
  );
  assert.equal(await process.getAttribute("data-expanded"), "true");
  events.push(event(6, "context.compacted", { mode: "summary" }));
  await process.getByText("上下文已压缩", { exact: true }).waitFor();
  events.push(event(7, "context.compacted", { clearedResults: 1 }));
  await page.waitForTimeout(1200);
  assert.equal(
    await page.locator(".context-activity").count(),
    1,
    "minor tool-result clearing duplicated summary notice",
  );
  events.push(event(8, "context.compaction.started"));
  await process.getByText("正在压缩上下文", { exact: true }).waitFor();
  events.push(event(9, "context.compacted", { mode: "summary" }));
  await page.waitForFunction(
    () =>
      window.contextSnapshot.contextActivities.filter(
        (item) => item.state === "completed",
      ).length === 2,
  );
  history.push({
    role: "assistant",
    content: "任务已完成。",
    runId: "run",
    streamBoundary: 12,
  });
  events.push(
    event(12, "assistant.completed"),
    event(13, "run.changed", { ...task, state: "answered" }),
  );
  await page
    .locator("[data-answer]")
    .getByText("任务已完成。", { exact: true })
    .waitFor();
  assert.equal(await process.getAttribute("data-expanded"), "false");
  assert.equal(
    await page.locator(".context-activity").count(),
    0,
    "summary notice remained outside collapsed process",
  );
  await process.locator(".process-disclosure-summary").click();
  assert.equal(
    await process.getByText("上下文已压缩", { exact: true }).count(),
    2,
  );
  eventBatchLimit = 1;
  await page.addInitScript(() => {
    window.historyProcessFlashed = false;
    new MutationObserver(() => {
      if (
        document.querySelector("[data-answer]")?.textContent ===
          "任务已完成。" &&
        document.querySelector(".process-disclosure")?.dataset.expanded ===
          "true"
      )
        window.historyProcessFlashed = true;
    }).observe(document, { childList: true, subtree: true, attributes: true });
  });
  await page.reload();
  await page.waitForFunction(
    () => window.contextSnapshot.contextActivities.length === 2,
  );
  assert.equal(
    await process.getAttribute("data-expanded"),
    "false",
    "history compaction records flashed open",
  );
  assert.equal(await page.locator(".context-activity").count(), 0);
  await page.waitForFunction(
    () => window.contextSnapshot.task?.state === "answered",
  );
  assert.equal(
    await page.evaluate(() => window.historyProcessFlashed),
    false,
    "split historical event replay reopened process",
  );
  await page.evaluate(() => {
    window.forgetFixture();
    window.readsAtDeletion = window.eventReadStarts;
  });
  await page.waitForFunction(
    () => window.contextSnapshot.messages.length === 0,
  );
  await page.waitForTimeout(1200);
  assert.equal(
    await page.evaluate(() => window.eventReadStarts),
    await page.evaluate(() => window.readsAtDeletion),
    "deleted session kept polling or reopened itself",
  );
  assert.equal(
    await page.evaluate(() => window.suggestedContextWindow("unknown-model")),
    256000,
  );
  assert.equal(
    await page.evaluate(() => window.suggestedContextWindow("gpt-4o")),
    128000,
  );
  await page.evaluate(() =>
    window.showModels([
      {
        id: "saved",
        name: "Saved model",
        protocol: "openai-chat",
        model: "custom-model",
        baseUrl: "http://fixture.invalid/v1",
        active: true,
        contextWindow: 200000,
      },
    ]),
  );
  await page.getByRole("button", { name: "高级选项", exact: true }).click();
  const contextInput = page.getByLabel("上下文长度", { exact: true });
  assert.equal(await contextInput.inputValue(), "200000");
  await page.getByRole("button", { name: "获取模型", exact: true }).click();
  await page.getByRole("button", { name: /使用建议值 · 1000k/ }).waitFor();
  assert.equal(
    await contextInput.inputValue(),
    "200000",
    "fetch overwrote saved capacity",
  );
  await page.getByRole("button", { name: /使用建议值 · 1000k/ }).click();
  assert.equal(await contextInput.inputValue(), "1000000");
  await contextInput.fill("750000");
  await page.getByRole("button", { name: "获取模型", exact: true }).click();
  await page.waitForTimeout(200);
  assert.equal(
    await contextInput.inputValue(),
    "750000",
    "fetch overwrote manual capacity",
  );
  await page.evaluate(() => window.showModels([]));
  await page.getByRole("button", { name: "高级选项", exact: true }).click();
  assert.equal(await contextInput.inputValue(), "256000");
  await page.getByRole("combobox", { name: "服务商", exact: true }).click();
  await page.getByRole("option", { name: "自定义", exact: true }).click();
  await page
    .getByLabel("API 地址", { exact: true })
    .fill("http://fixture.invalid/v1");
  await page.getByRole("button", { name: "获取模型", exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('input[value="1000000"]'),
  );
  assert.equal(
    await contextInput.inputValue(),
    "1000000",
    "new model did not adopt reported capacity",
  );
  assert(catalogRequests >= 3);
  assert.deepEqual(pageErrors, []);
  console.log(
    JSON.stringify({
      compactionInProcess: true,
      summaryClosesIntoDisclosure: true,
      historyStartsCollapsed: true,
      authoritativeRunSnapshotPreventsOldEventRegression: true,
      usageTooltipHasNoCompaction: true,
      newFallback: 256000,
      reportedCapacityAdopted: true,
      savedAndManualCapacityPreserved: true,
      realUserDataTouched: false,
      modelRequests: 0,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
