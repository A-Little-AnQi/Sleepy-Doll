import { uiStylesheetUrl, uiMotionStylesheetUrl } from "./runtime.mjs";
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

// 真实 ChatPage/Session，假 IPC；不调用模型或游戏。runtime.mjs 管理临时目录。
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const fixture = `
import React from 'react';
import ReactDOM from 'react-dom/client';
import {ChatPage} from '/src/pages/chat/ChatPage.tsx';
import {useSession} from '/src/session/index.ts';
import {PREVIEW_PERMISSION} from '/src/ipc/types.ts';
import '${uiStylesheetUrl}';
import '${uiMotionStylesheetUrl}';
const bootstrap={models:[{id:'m',name:'Model',model:'fixture',active:true,contextWindow:256000}],
 conversations:[{id:'usage',title:'Usage',modelId:'m'},{id:'legacy',title:'Legacy',modelId:'m'}],
 tools:[],skills:[],tasks:[],permission:PREVIEW_PERMISSION,runtimeToolLabels:{}};
function Fixture(){
 const [id,setId]=React.useState('usage');
 window.showConversation=setId;
 window.usageSnapshot=useSession(id);
 return React.createElement('div',{style:{height:'100vh'}},React.createElement(ChatPage,
  {bootstrap,conversationId:id,onConversation:setId,reload:async()=>{}}));
}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));`;
const makeRun = (
  id,
  state,
  createdAt,
  contextTokens = 0,
  cacheReadTokens = 0,
) => ({
  id,
  conversationId: "usage",
  prompt: "测试",
  state,
  createdAt,
  updatedAt: createdAt,
  revision: 1,
  contextTokens,
  cacheReadTokens,
  contextWindow: 256000,
  source: { kind: "agent" },
});
const runs = [makeRun("old", "answered", "2026-10-22T00:00:00Z", 6000, 5000)];
const events = [];
let sequence = 0;
let usageReads = 0;
const push = (kind, run, data = {}) =>
  events.push({
    sequence: ++sequence,
    kind,
    runId: run.id,
    conversationId: "usage",
    data,
  });
const changed = (run) => {
  run.revision++;
  push("run.changed", run, { ...run });
};
let server, browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false },
  });
  await server.listen();
  const entry = await server.transformRequest("/src/main.tsx");
  const react = entry.code.match(/from "([^"]*\/react\.js\?[^"]*)"/)[1];
  const dom = entry.code.match(
    /from "([^"]*\/react-dom_client\.js\?[^"]*)"/,
  )[1];
  browser = await chromium.launch({
    headless: true,
    ...(process.env.SLEEPY_DOLL_BROWSER_CHANNEL
      ? { channel: process.env.SLEEPY_DOLL_BROWSER_CHANNEL }
      : {}),
  });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 850 },
  });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() =>
    localStorage.setItem("sleepy-doll-locale", "zh"),
  );
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    const id = request.params?.conversationId ?? request.params?.id;
    let result = {};
    if (request.method === "conversation.get") {
      if (id === "usage") usageReads++;
      result = {
        id,
        messages: [{ role: "user", content: "历史文字".repeat(5000) }],
        runs: id === "usage" ? runs : [],
        questionRequests: [],
      };
    }
    if (request.method === "events.read")
      result = {
        events:
          id === "usage"
            ? events.filter((e) => e.sequence > request.params.after)
            : [],
      };
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.route("**/__usage.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(react))
        .replace("'react-dom/client'", JSON.stringify(dom)),
    }),
  );
  const html = await server.transformIndexHtml(
    "/__usage",
    '<html data-theme="dark"><body><div id="root"></div><script type="module" src="/__usage.js"></script></body></html>',
  );
  await page.route("**/__usage", (route) =>
    route.fulfill({ contentType: "text/html", body: html }),
  );
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/__usage`,
  );
  const meter = page.locator(".sd-context");
  const usage = () => meter.getAttribute("aria-label");
  await page.waitForFunction(
    () => window.usageSnapshot.contextUsage?.used === 6000,
  );
  const initial = await usage();
  await page.locator("textarea").fill("输入草稿".repeat(2000));
  assert.equal(
    await usage(),
    initial,
    "Draft typing changed request statistics",
  );
  const next = makeRun("next", "deciding", "2026-10-22T00:01:00Z");
  runs.push(next);
  changed(next);
  push("assistant.delta", next, { text: "流式文字".repeat(2000) });
  push("context.request.profile", next, { totalEstimatedTokens: 90000 });
  push("context.compaction.started", next);
  push("context.compacted", next, { mode: "summary", tokens: 1000 });
  push("tool.completed", next, { result: "大段结果".repeat(2000) });
  await page.waitForFunction(
    () =>
      window.usageSnapshot.task?.id === "next" &&
      window.usageSnapshot.stream.length > 0,
  );
  assert.equal(
    await usage(),
    initial,
    "New request, stream or compaction changed settled statistics",
  );
  next.contextTokens = 8000;
  next.cacheReadTokens = 7000;
  next.state = "executing";
  changed(next);
  await page.waitForFunction(
    () => window.usageSnapshot.contextUsage?.used === 8000,
  );
  assert.match(await usage(), /8k/);
  next.state = "deciding";
  changed(next);
  push("context.request.profile", next, { totalEstimatedTokens: 95000 });
  push("assistant.delta", next, { text: "下一轮文字".repeat(1000) });
  await page.waitForFunction(
    () => window.usageSnapshot.task?.state === "deciding",
  );
  assert.match(
    await usage(),
    /8k/,
    "Next dispatch replaced settled usage with an estimate",
  );
  next.contextTokens = 3000;
  next.cacheReadTokens = 0;
  next.state = "answered";
  changed(next);
  push("assistant.completed", next);
  await page.waitForFunction(
    () => window.usageSnapshot.contextUsage?.used === 3000,
  );
  assert.match(
    await usage(),
    /3k/,
    "Completed compacted request did not commit lower usage",
  );
  const failed = makeRun("failed", "deciding", "2026-10-22T00:02:00Z");
  runs.push(failed);
  changed(failed);
  failed.state = "cancelled";
  changed(failed);
  await page.waitForFunction(() => window.usageSnapshot.task?.id === "failed");
  assert.match(
    await usage(),
    /3k/,
    "Cancelled unmeasured request reset statistics",
  );
  await page.reload();
  await page.waitForFunction(
    () => window.usageSnapshot.contextUsage?.used === 3000,
  );
  assert.match(
    await usage(),
    /3k/,
    "Reopening conversation lost settled statistics",
  );
  // 保持原会话的后台事件循环运行；后端快照变动不附带新事件。
  const background = makeRun("background", "deciding", "2026-10-22T00:03:00Z");
  runs.push(background);
  changed(background);
  await page.waitForFunction(
    () => window.usageSnapshot.task?.state === "deciding",
  );
  await page.evaluate(() => window.showConversation("legacy"));
  await page.waitForFunction(
    () => !window.usageSnapshot.loading && !window.usageSnapshot.task,
  );
  assert.match(
    await usage(),
    /— \/ 256k/,
    "Missing usage was replaced by a history estimate",
  );
  await page.locator("textarea").fill("未知统计的草稿".repeat(1000));
  assert.match(await usage(), /— \/ 256k/);
  background.contextTokens = 4500;
  background.cacheReadTokens = 2000;
  background.revision++;
  const readsBeforeReentry = usageReads;
  await page.evaluate(() => window.showConversation("usage"));
  await page.waitForFunction(
    () => window.usageSnapshot.contextUsage?.used === 4500,
  );
  assert.match(await usage(), /4\.5k/);
  assert.equal(
    usageReads - readsBeforeReentry,
    1,
    "Reentry did not coalesce subscriptions into one fresh snapshot read",
  );
  await page.evaluate(() => window.showConversation(undefined));
  await page.waitForFunction(
    () =>
      document.querySelector(".sd-context")?.getAttribute("aria-label") ===
      "Tokens · 0 / 256k",
  );
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify({
      passed: true,
      draftAndStreamStable: true,
      pendingAndCancelledRunPreservesUsage: true,
      completionCommitsOnce: true,
      compactedUsageCanDecrease: true,
      historyRestoreStable: true,
      reentryReloadsWithoutNewEvents: true,
      unknownUsageNotEstimated: true,
      modelRequests: 0,
      realUserDataTouched: false,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
