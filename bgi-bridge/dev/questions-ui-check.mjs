// 结构化问答浮层验收：真实 ChatPage/Session/API，fake IPC 不启动模型或 BetterGI。
// 资源登记：本地 vite 服务、Chromium 与截图目录 target/.tmp/shortcut-runtime/questions-ui
// 均属本任务（运行时 TEMP/TMP 指向 target/.tmp/shortcut-runtime）；
// finally 精确关闭 server/browser，只保留两张截图供主代理查看。
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const shotDir = path.join(root, "target", ".tmp", "shortcut-runtime", "questions-ui");
const fixture = `
import React from 'react';
import ReactDOM from 'react-dom/client';
import {ChatPage} from '/src/pages/chat/ChatPage.tsx';
import {PREVIEW_PERMISSION} from '/src/ipc/types.ts';
import '/src/product.css';
import '/src/motion.css';
const bootstrap={models:[{id:'fixture-model',name:'Fixture Model',model:'fixture',active:true,contextWindow:256000}],
 conversations:[{id:'q-a',title:'A'},{id:'q-b',title:'B'},{id:'q-c',title:'C'},{id:'q-d',title:'D'},{id:'q-e',title:'E'},{id:'q-f',title:'F'},{id:'q-g',title:'G'},{id:'q-z',title:'Z'}],
 tools:[],skills:[],tasks:[],permission:PREVIEW_PERMISSION,runtimeToolLabels:{}};
function Fixture(){
 const [conversationId,setConversation]=React.useState('q-z');
 window.showConv=(id)=>setConversation(id);
 return React.createElement('div',{style:{height:'100vh'}},
  React.createElement(ChatPage,{bootstrap,conversationId,onConversation:setConversation,reload:async()=>{}}));
}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
`;

// ---- fake backend state（Node 侧）----
// 与真实后端一致：open 请求持久化在会话快照里，事件到达后权威快照也能看到；
// ACK 之后快照不再返回它。
const calls = [];
let failNextAnswer = false;
let answerDelayMs = 0;
let staleSnapshot = false;
const staleConvs = new Set();
const answered = new Set();
const convOfRun = {
  "run-a": "q-a",
  "run-b": "q-b",
  "run-c": "q-c",
  "run-d1": "q-d",
  "run-d2": "q-d",
  "run-e": "q-e",
  "run-f": "q-f",
  "run-g": "q-g",
};
const request = (requestId, runId, questions) => ({ requestId, runId, questions });
// 事件 data.request 只有 requestId/questions；runId 以事件归属为权威。
const eventRequest = (requestId, questions) => ({ requestId, questions });
const longText = "请描述脚本要放在哪个目录，包括盘符、各级目录名与文件名，并说明为什么选择这个路径，方便我们排查。";
const multiQuestion = [
  {
    id: "mode",
    header: "运行方式",
    question: "现在**启动**还是只做检查？",
    options: [
      { label: "启动", description: "立即运行" },
      { label: "检查" },
    ],
  },
  { id: "path", header: "路径", question: longText },
];
const singleQuestion = [
  {
    id: "way",
    header: "方式",
    question: longText,
    options: [{ label: "快速" }, { label: "完整" }],
  },
];
// id=constructor 与 label=__free__ 都不是哨兵：选择按结构化下标存储。
const trickyQuestion = [
  {
    id: "constructor",
    header: "标签",
    question: "选择一个标签？",
    options: [{ label: "__free__" }, { label: "正规" }],
  },
  { id: "__proto__", header: "参数", question: "用一句话描述参数。" },
];
const miningQuestion = [
  {
    id: "config",
    header: "采矿配置",
    question: "使用哪个已有配置？",
    options: [{ label: "莉奈娅挖矿" }, { label: "矿产资源批发" }],
  },
];
const openRequests = {
  "q-a": [request("call-1", "run-a", multiQuestion)],
  "q-b": [request("call-b1", "run-b", singleQuestion)],
  "q-c": [],
  // 两个不同 run 的 awaitingUser 请求：一个 run 同时只有一个 open 请求。
  "q-d": [
    request("call-d1", "run-d1", singleQuestion),
    request("call-d2", "run-d2", multiQuestion),
  ],
  "q-e": [],
  "q-f": [request("call-f1", "run-f", singleQuestion)],
  // 正常布局样例：1 题 2 选项、主输入空，用于视觉截图。
  "q-g": [request("call-g1", "run-g", miningQuestion)],
};
const events = {
  "q-a": [],
  "q-b": [],
  "q-c": [],
  "q-d": [],
  "q-e": [],
  "q-f": [],
  "q-g": [],
};
let seq = 10;
const pushEvent = (conv, kind, runId, data) =>
  events[conv].push({
    sequence: ++seq,
    kind,
    runId,
    conversationId: conv,
    data,
  });
const runState = {
  "run-a": "awaitingUser",
  "run-b": "awaitingUser",
  "run-c": "awaitingApproval",
  "run-d1": "awaitingUser",
  "run-d2": "awaitingUser",
  "run-e": "awaitingUser",
  "run-f": "awaitingUser",
  "run-g": "awaitingUser",
};
const runFields = (id, conv) => ({
  id,
  conversationId: conv,
  prompt: "fixture",
  state: runState[id],
  createdAt: "2026-10-01T00:00:00Z",
  updatedAt: "2026-10-01T00:00:01Z",
});
const runsOf = (conv) => runFields(conv.replace("q-", "run-"), conv);
const runById = (id) => runFields(id, convOfRun[id]);
const run = (conv) => runsOf(conv);
const runsFor = (conv) =>
  conv === "q-d" ? [runById("run-d1"), runById("run-d2")] : [run(conv)];
// 压力会话依赖两个唯一且真实等待用户的 run；id 缺失/重复会让后面断言失真。
assert.equal(runById("run-d1").id, "run-d1");
assert.equal(runById("run-d2").id, "run-d2");
assert.notDeepEqual(runById("run-d1"), runById("run-d2"), "run-d1/d2 必须是两条记录");
assert.equal(runById("run-d1").state, "awaitingUser");
assert.equal(runById("run-d2").state, "awaitingUser");
// 浮层两轴居中：中心 = workspace 水平中心、(workspace.top+dock.top)/2。
const centeringError = (page) =>
  page.evaluate(() => {
    const rect = (node) => node.getBoundingClientRect();
    const layer = rect(document.querySelector(".pending-request-layer"));
    const ws = rect(document.querySelector(".chat-workspace"));
    const dock = rect(document.querySelector(".composer-dock"));
    return {
      x: layer.left + layer.width / 2 - (ws.left + ws.width / 2),
      y: layer.top + layer.height / 2 - (ws.top + dock.top) / 2,
    };
  });
// q-b：请求先持久化、再发事件（真实顺序）；权威快照在 ACK 前都能看到它。
pushEvent("q-b", "question", "run-b", {
  question: "怎么运行？",
  request: eventRequest("call-b1", singleQuestion),
});
// q-f：取消场景的起点同样是持久化 + 事件。
pushEvent("q-f", "question", "run-f", {
  question: "怎么运行？",
  request: eventRequest("call-f1", singleQuestion),
});
// q-c 带一条已持久化的助手消息：turn-outcome-note 只认真实消息流。
const convMessages = (conv) =>
  conv === "q-c"
    ? [
        {
          role: "assistant",
          content: "检查完成，等待确认。",
          runId: "run-c",
          streamBoundary: 0,
          createdAt: "2026-10-01T00:00:02Z",
        },
      ]
    : [];
pushEvent("q-c", "approval.requested", "run-c", {
  id: "appr-1",
  runId: "run-c",
  request: {
    methodId: "bridge.call",
    arguments: { target: "tmp" },
    binding: { description: "删除临时文件" },
  },
  expiresAt: Math.floor(Date.now() / 1000) + 600,
});

let server;
let browser;
let report;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    // 纯内存夹具没有 HMR WebSocket：Vite 8 在 agent 环境会默认开启
    // forward-console（把页面错误经 WS 转发给 dev server），连不上时客户端
    // 每次转发都抛 "Failed to send error to Vite server"。显式关闭这套
    // 插桩，让下方 console 错误断言只看产品自身的输出。
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  const entry = await server.transformRequest("/src/main.tsx");
  const reactUrl = entry.code.match(/from "([^"]*\/react\.js\?[^"]*)"/)[1];
  const domUrl = entry.code.match(
    /from "([^"]*\/react-dom_client\.js\?[^"]*)"/,
  )[1];
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 850 },
  });
  page.setDefaultTimeout(8000);
  const errors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    // 只豁免明确来自 Vite @vite/client 的 DEV HMR 连接失败（校验来源 URL 与
    // 固定前缀）；产品 JS 的其它含 hmr/websocket 字样的错误一律记录。
    const source = message.location()?.url ?? "";
    const isViteHmrNoise =
      (source.includes("@vite/client") &&
        /^\[vite\] (failed to connect to websocket|server connection lost)/i.test(
          text,
        )) ||
      // Vite client 注入的 HMR WebSocket 在纯内存夹具里必然连不上；
      // 只豁免明确指向本机夹具 server 的连接失败，其余 websocket 错误照记。
      /^WebSocket connection to 'ws:\/\/127\.0\.0\.1[:/]/.test(text);
    if (!isViteHmrNoise) consoleErrors.push(text);
  });
  await page.addInitScript(() => localStorage.setItem("sleepy-doll-locale", "zh"));
  const answerCalls = () =>
    calls.filter((call) => call.method === "run.question.answer");
  const cardCount = () => page.locator(".pending-request-card").count();

  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    calls.push(request);
    const conv = request.params?.conversationId ?? request.params?.id ?? "";
    let result = {};
    if (request.method === "conversation.get") {
      const source =
        staleConvs.has(conv) || (staleSnapshot && conv === "q-a")
          ? openRequests[conv] ?? []
          : (openRequests[conv] ?? []).filter(
              (entry) => !answered.has(`${entry.runId}:${entry.requestId}`),
            );
      result = {
        id: conv,
        messages: convMessages(conv),
        runs: openRequests[conv] ? runsFor(conv) : [],
        questionRequests: source,
      };
    }
    if (request.method === "events.read")
      result = {
        events: (events[conv] ?? []).filter(
          (event) => event.sequence > request.params.after,
        ),
      };
    if (request.method === "run.question.answer") {
      if (failNextAnswer) {
        failNextAnswer = false;
        await route.fulfill({
          contentType: "application/json",
          body: JSON.stringify({
            id: request.id,
            ok: false,
            error: { message: "Fixture retry" },
          }),
        });
        return;
      }
      if (answerDelayMs) {
        await sleep(answerDelayMs);
        answerDelayMs = 0;
      }
      answered.add(`${request.params.id}:${request.params.requestId}`);
      pushEvent(convOfRun[request.params.id] ?? "q-z", "question.answered", request.params.id, {
        requestId: request.params.requestId,
      });
      result = { accepted: true, recorded: true };
    }
    if (request.method === "approval.respond") {
      runState["run-c"] = "running";
      result = { id: "appr-1", state: "approved" };
    }
    if (request.method === "task.submit")
      result = {
        ...run(conv || "q-z"),
        id: "new-run",
        prompt: request.params.prompt,
        state: "queued",
      };
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.route("**/__questions_fixture.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(reactUrl))
        .replace("'react-dom/client'", JSON.stringify(domUrl)),
    }),
  );
  const html = await server.transformIndexHtml(
    "/__fixture",
    '<html data-theme="dark"><body><div id="root"></div><script type="module" src="/__questions_fixture.js"></script></body></html>',
  );
  await page.route("**/__fixture", (route) =>
    route.fulfill({ contentType: "text/html", body: html }),
  );
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/__fixture`,
  );
  const composer = page.getByRole("textbox", { name: "消息" });
  await composer.waitFor();

  // ---- 几何基线：主输入按命名取，绝不误取浮层内 textarea ----
  const geometry = () =>
    page.evaluate(() => {
      const box = (node) => {
        const rect = node.getBoundingClientRect();
        return {
          top: Math.round(rect.top * 10) / 10,
          left: Math.round(rect.left * 10) / 10,
          width: Math.round(rect.width * 10) / 10,
          height: Math.round(rect.height * 10) / 10,
          scroll: node.scrollHeight,
        };
      };
      return {
        composer: box(document.querySelector('textarea[aria-label="消息"]')),
        dock: box(document.querySelector(".composer-dock")),
        flow: box(document.querySelector(".conversation-flow")),
      };
    });
  const before = await geometry();

  // ---- A. 快照恢复：多题、无自动提交、几何不变、重试同 key ----
  await page.evaluate(() => window.showConv("q-a"));
  const cardA = page.locator(".pending-request-card");
  await cardA.waitFor();
  const after = await geometry();
  // q-a 有运行中的 run，对话流里会出现阶段提示属正常；composer/dock 必须不变。
  assert.deepEqual(after.composer, before.composer, "问答出现前后 composer 布局不得改变");
  assert.deepEqual(after.dock, before.dock, "问答出现前后 composer-dock 布局不得改变");
  // 浮层在可用内容区（workspace 顶到 dock 顶）两轴居中。
  const centerErr = await centeringError(page);
  assert.ok(Math.abs(centerErr.x) <= 1, `浮层必须水平居中，偏差 ${centerErr.x}`);
  assert.ok(Math.abs(centerErr.y) <= 1, `浮层必须垂直居中于可用区中点，偏差 ${centerErr.y}`);
  // 基础对齐：标题/正文同左边线；radio 16px 且圆点中心对齐首行行盒中心；
  // description 与 label 左边线一致。
  const alignA = await cardA.evaluate((card) => {
    const left = (node) => node.getBoundingClientRect().left;
    const text = left(card.querySelector(".pending-request-text"));
    const header = left(card.querySelector(".pending-request-header h3"));
    const option = card.querySelector(".pending-request-option");
    const radioNode = option.querySelector("input");
    const radio = radioNode.getBoundingClientRect();
    const label = option.querySelector(".pending-request-option-label");
    const desc = option.querySelector(".pending-request-option-description");
    const labelBox = label.getBoundingClientRect();
    return {
      header: header - text,
      desc: desc ? left(desc) - left(label) : null,
      radioSize: Math.max(Math.abs(radio.width - 16), Math.abs(radio.height - 16)),
      probe: `${radio.width}x${radio.height}/${getComputedStyle(radioNode).boxSizing}`,
      radioCenter: radio.top + radio.height / 2 - (labelBox.top + 10),
    };
  });
  assert.ok(Math.abs(alignA.header) <= 1, "标题与正文必须共享左边线");
  assert.ok(alignA.desc === null || Math.abs(alignA.desc) <= 1, "description 必须与 label 左对齐");
  assert.ok(alignA.radioSize <= 0.5, `radio 必须是 16x16，实际 ${alignA.probe} 偏差 ${alignA.radioSize}`);
  assert.ok(Math.abs(alignA.radioCenter) <= 1, "radio 圆点必须对齐首行文本行盒中心");
  assert.ok(
    await page.getByRole("button", { name: "选择技能" }).isDisabled(),
    "busy 时技能菜单必须禁用，避免插入运行中草稿",
  );
  // MarkdownText 渲染块级元素：容器必须是 DIV，且不允许 p 内嵌块级。
  assert.equal(
    await cardA.locator(".pending-request-text").evaluate((node) => node.tagName),
    "DIV",
  );
  assert.equal(await cardA.locator("p div, p ol, p ul, p p").count(), 0);
  // 第一题：两个建议选项 + 其他。
  const optionsA = cardA.locator("label.pending-request-option");
  assert.equal(await optionsA.count(), 3);
  const nextA = cardA.getByRole("button", { name: "下一题" });
  assert.ok(await nextA.isDisabled(), "未选不能进下一题");
  await optionsA.filter({ hasText: "启动" }).click();
  await sleep(400);
  assert.equal(answerCalls().length, 0, "选择不得自动提交");
  assert.ok(await nextA.isEnabled());
  await nextA.click();
  await cardA.getByText("第 2/2 题").waitFor();
  // 纯文字题：无选项，textarea 直接可用。
  const freeA = cardA.getByRole("textbox", { name: "填写回复" });
  await freeA.fill("自定义");
  // busy 会话里浮层不遮主停止按钮。
  const layerBox = () => page.locator(".pending-request-layer").boundingBox();
  const stopA = page.locator(".composer-submit .send-action");
  assert.ok(await stopA.isVisible(), "busy 会话停止按钮可见");
  const stopBoxA = await stopA.boundingBox();
  const layerNow = await layerBox();
  assert.ok(
    stopBoxA.y >= layerNow.y + layerNow.height - 1,
    "浮层不得遮挡主停止按钮",
  );
  // 草稿与题序跨会话切换恢复。
  await page.evaluate(() => window.showConv("q-z"));
  await sleep(200);
  await page.evaluate(() => window.showConv("q-a"));
  await cardA.waitFor();
  await cardA.getByText("第 2/2 题").waitFor();
  assert.equal(await freeA.inputValue(), "自定义");
  // 收起也持久化。
  await cardA.getByRole("button", { name: "收起面板" }).click();
  const chipA = page.locator(".pending-request-chip");
  await chipA.waitFor();
  await page.evaluate(() => window.showConv("q-z"));
  await sleep(200);
  await page.evaluate(() => window.showConv("q-a"));
  await chipA.waitFor();
  await chipA.click();
  await cardA.waitFor();
  // 提交失败：同内容重试同 key，含切走再回来。
  failNextAnswer = true;
  const submitA = cardA.getByRole("button", { name: "发送回复", exact: true });
  await submitA.click();
  await cardA.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(await freeA.inputValue(), "自定义");
  assert.equal(answerCalls().length, 1);
  const failedKey = answerCalls()[0].params.clientKey;
  assert.deepEqual(answerCalls()[0].params.answers, {
    mode: { answers: ["启动"] },
    path: { answers: ["自定义"] },
  });
  await page.evaluate(() => window.showConv("q-z"));
  await sleep(200);
  await page.evaluate(() => window.showConv("q-a"));
  await cardA.waitFor();
  failNextAnswer = true;
  await submitA.click();
  await cardA.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(
    answerCalls()[1].params.clientKey,
    failedKey,
    "切走再回来同内容重试仍必须同 key",
  );
  await submitA.click();
  await page.waitForFunction(() => !document.querySelector(".pending-request-card"));
  assert.equal(answerCalls().length, 3);
  assert.equal(answerCalls()[2].params.clientKey, failedKey, "同内容重试必须同 key");
  // ACK 后迟到快照不复活。
  staleSnapshot = true;
  pushEvent("q-a", "run.changed", "run-a", { ...run("q-a"), revision: 2 });
  await sleep(2500);
  assert.equal(await cardCount(), 0, "过期快照不得复活已答问题");
  assert.equal(answerCalls().length, 3);
  staleSnapshot = false;
  // 刷新重挂：已答请求不可再提交。
  await page.reload();
  await composer.waitFor();
  await sleep(1500);
  assert.equal(await cardCount(), 0, "ACK 后刷新不得恢复草稿或问题");

  // ---- B. 事件进入的新问题：runId 以事件归属为权威 ----
  await page.evaluate(() => window.showConv("q-b"));
  const cardB = page.locator(".pending-request-card");
  await cardB.waitFor();
  await cardB.locator("label.pending-request-option").filter({ hasText: "快速" }).click();
  await cardB.getByRole("button", { name: "发送回复", exact: true }).click();
  await page.waitForFunction(() => !document.querySelector(".pending-request-card"));
  assert.equal(answerCalls().length, 4);
  assert.equal(answerCalls()[3].params.id, "run-b", "专用 answer RPC 的 id 必须等于事件归属 runId");
  assert.equal(answerCalls()[3].params.requestId, "call-b1");
  // 同文本、新 requestId 可以再次回答（快照与事件同时可见也不重复出卡）。
  // run-b 维持 awaitingUser（终态不可重开；handoff→task.submit 由独立
  // composer 脚本实测）：直接持久化新请求并发事件。
  openRequests["q-b"].push(request("call-b2", "run-b", trickyQuestion));
  pushEvent("q-b", "question", "run-b", {
    question: "怎么运行？",
    request: eventRequest("call-b2", trickyQuestion),
  });
  await cardB.waitFor();
  assert.equal(await cardCount(), 1, "同一请求不得因快照+事件重复出卡");
  const optionsB = cardB.locator("label.pending-request-option");
  assert.equal(await optionsB.count(), 3, "label=__free__ 只是普通选项");
  await optionsB.filter({ hasText: "__free__" }).click();
  await cardB.getByRole("button", { name: "下一题" }).click();
  const freeB = cardB.getByRole("textbox", { name: "填写回复" });
  await freeB.fill("参数描述文字");
  // 失败后改答案换 clientKey。
  failNextAnswer = true;
  const submitB = cardB.getByRole("button", { name: "发送回复", exact: true });
  await submitB.click();
  await cardB.getByText("Fixture retry", { exact: true }).waitFor();
  const changedKey = answerCalls()[4].params.clientKey;
  assert.deepEqual(
    answerCalls()[4].params.answers,
    JSON.parse('{"constructor":{"answers":["__free__"]},"__proto__":{"answers":["参数描述文字"]}}'),
  );
  await freeB.fill("参数描述文字二");
  await submitB.click();
  await page.waitForFunction(() => !document.querySelector(".pending-request-card"));
  assert.equal(answerCalls().length, 6);
  assert.notEqual(
    answerCalls()[5].params.clientKey,
    changedKey,
    "改了答案必须换新 clientKey",
  );
  // 迟到的 ACK 回调不写别的聊天：提交中途切走（run-b 仍是 awaitingUser）。
  // 先持久化再发事件：run.changed 触发的权威快照要能见到它。
  openRequests["q-b"].push(request("call-b3", "run-b", singleQuestion));
  pushEvent("q-b", "question", "run-b", {
    question: "怎么运行？",
    request: eventRequest("call-b3", singleQuestion),
  });
  await cardB.waitFor();
  answerDelayMs = 1200;
  await cardB.locator("label.pending-request-option").filter({ hasText: "快速" }).click();
  const submitB3 = cardB.getByRole("button", { name: "发送回复", exact: true });
  await submitB3.click();
  // 发送中选项禁用，payload 不会变。
  await cardB.getByRole("button", { name: "正在发送…", exact: true }).waitFor();
  assert.ok(
    await cardB.locator('input[type="radio"]').first().isDisabled(),
    "提交中必须禁用选项",
  );
  await page.evaluate(() => window.showConv("q-z"));
  await sleep(300);
  assert.equal(await cardCount(), 0, "切换后不得出现问题卡");
  await page.waitForTimeout(1500);
  assert.equal(await cardCount(), 0, "切换后不得出现问题卡");
  const b3 = answerCalls().filter((call) => call.params.requestId === "call-b3");
  assert.equal(b3.length, 1, "迟到的回调不得重复提交");
  await page.evaluate(() => window.showConv("q-b"));
  await sleep(1200);
  assert.equal(await cardCount(), 0, "已 ACK 的请求回到原聊天也不得复活");

  // ---- B2. 同一会话几何：请求事件出现前后布局不变 ----
  await page.evaluate(() => window.showConv("q-e"));
  await composer.waitFor();
  await sleep(800);
  const geoBeforeEvent = await geometry();
  pushEvent("q-e", "question", "run-e", {
    question: "怎么运行？",
    request: eventRequest("call-e1", singleQuestion),
  });
  // 事件到达后请求也持久化：后续权威快照同样能见到它。
  openRequests["q-e"].push(request("call-e1", "run-e", singleQuestion));
  await page.locator(".pending-request-card").waitFor();
  await sleep(800);
  assert.deepEqual(
    await geometry(),
    geoBeforeEvent,
    "同一会话请求事件出现前后 composer/dock/flow 布局不得改变",
  );
  const cardE = page.locator(".pending-request-card");
  await cardE.locator("label.pending-request-option").filter({ hasText: "完整" }).click();
  await cardE.getByRole("button", { name: "发送回复", exact: true }).click();
  await page.waitForFunction(() => !document.querySelector(".pending-request-card"));

  // ---- B3. 取消运行后：过期快照 / 重放 question 事件都不得恢复面板 ----
  await page.evaluate(() => window.showConv("q-f"));
  await page.locator(".pending-request-card").waitFor();
  pushEvent("q-f", "cancel.requested", "run-f", {});
  // cancel.requested 先于权威 run.changed 到达；随后拉到的旧 awaitingUser
  // 快照（请求仍在）也不得复活面板。
  staleConvs.add("q-f");
  pushEvent("q-f", "input.received", "run-f", {});
  await sleep(1200);
  assert.equal(
    await cardCount(),
    0,
    "cancel.requested 后过期 awaitingUser 快照不得复活问答面板",
  );
  staleConvs.delete("q-f");
  pushEvent("q-f", "run.changed", "run-f", {
    ...runById("run-f"),
    state: "cancelling",
    revision: 2,
  });
  // 取消像真实后端一样先持久化 cancelling，随后落终态。
  runState["run-f"] = "cancelling";
  await page.waitForFunction(() => !document.querySelector(".pending-request-card"));
  // 过期快照仍带 open 请求，重放 question 事件：停止后的题不能复活。
  staleConvs.add("q-f");
  pushEvent("q-f", "question", "run-f", {
    question: "怎么运行？",
    request: eventRequest("call-f1", singleQuestion),
  });
  await sleep(2500);
  assert.equal(
    await cardCount(),
    0,
    "取消后过期快照与重放事件不得恢复问答面板",
  );
  staleConvs.delete("q-f");
  runState["run-f"] = "cancelled";
  await page.reload();
  await composer.waitFor();
  await sleep(1500);
  assert.equal(await cardCount(), 0, "取消后重挂也不得恢复问答面板");

  // ---- C. 审批同浮层 + 结果标注只看真实消息流 ----
  await page.evaluate(() => window.showConv("q-c"));
  const approvalCard = page.locator(".pending-approval-card");
  await approvalCard.waitFor();
  assert.equal(await page.locator(".conversation-flow .run-approval").count(), 0,
    "审批不得留在消息流里");
  assert.equal(
    await approvalCard.getByText("删除临时文件", { exact: true }).count(),
    1,
  );
  await approvalCard.getByText("操作参数").click();
  await approvalCard.locator("pre").waitFor();
  // footer 同排按钮必须垂直居中于同一行。
  const rowSpread = await approvalCard.evaluate((card) => {
    const centers = [...card.querySelectorAll(".pending-request-footer button")].map(
      (button) => {
        const box = button.getBoundingClientRect();
        return box.top + box.height / 2;
      },
    );
    return Math.max(...centers) - Math.min(...centers);
  });
  assert.ok(rowSpread <= 1, `footer 按钮必须同一行垂直对齐，偏差 ${rowSpread}`);
  // 审批期间主输入保持 busy 禁用，停止按钮可见。
  assert.ok(await composer.isDisabled(), "审批期间主输入禁用");
  assert.ok(await page.locator(".composer-submit .send-action").isVisible());
  await approvalCard.getByRole("button", { name: "拒绝" }).click();
  await sleep(600);
  const decision = calls.find((call) => call.method === "approval.respond");
  assert.equal(decision.params.approved, false);
  // needsReview 不产生任何末尾标注（取消保留「已停止」，用真实助手消息检测）。
  pushEvent("q-c", "run.changed", "run-c", { ...run("q-c"), state: "needsReview", revision: 3 });
  await sleep(1500);
  assert.equal(await page.locator(".turn-outcome-note").count(), 0,
    "needsReview 不得出现末尾标注");
  assert.equal(await page.locator(".run-error").count(), 0,
    "needsReview 不得出现结果 footer");
  pushEvent("q-c", "run.changed", "run-c", { ...run("q-c"), state: "cancelled", revision: 4 });
  await sleep(1500);
  const notes = page.locator(".turn-outcome-note");
  assert.equal(await notes.count(), 1, "cancelled 保留一条末尾标注");
  assert.equal(await notes.first().innerText(), "已停止");
  assert.equal(await page.locator(".run-error").count(), 0, "cancelled 不得出现 footer");

  // ---- D. 正常布局视觉样例：1 题 2 选项、主输入空 ----
  await page.evaluate(() => window.showConv("q-g"));
  const cardG = page.locator(".pending-request-card");
  await cardG.waitFor();
  await composer.waitFor();
  await sleep(400);
  assert.equal(await composer.inputValue(), "", "正常样例的主输入必须为空");
  assert.ok(await composer.isDisabled(), "run-g 等待回答期间主输入 busy 禁用");
  const gCenter = await centeringError(page);
  assert.ok(Math.abs(gCenter.x) <= 1 && Math.abs(gCenter.y) <= 1,
    `正常样例浮层必须两轴居中，偏差 ${gCenter.x}/${gCenter.y}`);
  fs.mkdirSync(shotDir, { recursive: true });
  await page.screenshot({ path: path.join(shotDir, "questions-desktop.png") });
  await page.setViewportSize({ width: 360, height: 740 });
  await sleep(400);
  const gMobileBox = await page.locator(".pending-request-layer").boundingBox();
  assert.ok(
    gMobileBox.x >= 0 && gMobileBox.x + gMobileBox.width <= 360.5,
    "移动端浮层不得横向溢出",
  );
  assert.ok(gMobileBox.y >= 0, "移动端浮层不得越出顶部");
  const gMobileCenter = await centeringError(page);
  assert.ok(Math.abs(gMobileCenter.x) <= 1 && Math.abs(gMobileCenter.y) <= 1,
    `移动端浮层必须两轴居中，偏差 ${gMobileCenter.x}/${gMobileCenter.y}`);
  await page.screenshot({ path: path.join(shotDir, "questions-mobile.png") });
  await page.setViewportSize({ width: 1100, height: 850 });
  await sleep(400);
  // blocked 是终态但允许重试：回 awaitingUser 后题目必须能再次出现
  // （cancelledRuns 只记取消语义，不得把其它终态永久封死）。
  pushEvent("q-g", "run.changed", "run-g", {
    ...runById("run-g"),
    state: "blocked",
    revision: 2,
  });
  runState["run-g"] = "blocked";
  await sleep(800);
  assert.equal(await cardCount(), 0, "blocked 终态下面板必须消失");
  runState["run-g"] = "awaitingUser";
  pushEvent("q-g", "run.changed", "run-g", {
    ...runById("run-g"),
    state: "awaitingUser",
    revision: 3,
  });
  await cardG.waitFor();
  assert.equal(await cardCount(), 1, "blocked 重试回 awaitingUser 后题目必须恢复");

  // ---- E. 压力几何：短窗大草稿双请求（只做坐标检查，不存截图） ----
  // q-d 是两个 awaitingUser run 各持一个 open 请求：主输入 busy 禁用，
  // 大草稿在该聊天首次挂载前经 localStorage 预置，浮层交互不改主草稿。
  const bigDraft = "一行\n".repeat(30);
  await page.evaluate(
    (value) => localStorage.setItem("sleepy-doll-draft:q-d", value),
    bigDraft,
  );
  await page.evaluate(() => window.showConv("q-d"));
  const cardsD = page.locator(".pending-request-card");
  await cardsD.first().waitFor();
  await composer.waitFor();
  assert.ok(await composer.isDisabled(), "q-d 双 run 等待回答，主输入禁用");
  assert.equal(await composer.inputValue(), bigDraft, "预置草稿必须恢复");
  await sleep(300);
  assert.equal(await cardsD.count(), 2, "两个 open 请求都要出现");
  const dCenter = await centeringError(page);
  assert.ok(Math.abs(dCenter.x) <= 1 && Math.abs(dCenter.y) <= 1,
    `压力双请求浮层仍必须两轴居中，偏差 ${dCenter.x}/${dCenter.y}`);
  const dockTop = () =>
    page.locator(".composer-dock").evaluate((node) => node.getBoundingClientRect().top);
  let box = await layerBox();
  assert.ok(box.y >= 0, "浮层不得越出视口顶部");
  assert.ok(box.y + box.height <= (await dockTop()) + 1);
  await page.setViewportSize({ width: 360, height: 740 });
  await sleep(400);
  box = await layerBox();
  assert.ok(box.x >= 0 && box.x + box.width <= 360.5, "移动端浮层不得横向溢出");
  assert.ok(box.y >= 0, "移动端浮层不得越出顶部");
  await page.setViewportSize({ width: 1100, height: 520 });
  await sleep(400);
  box = await layerBox();
  assert.ok(box.y >= 0, "composer 空间压缩后浮层不得越出顶部");
  assert.ok(box.y + box.height <= (await dockTop()) + 1, "浮层不得遮挡 composer");
  assert.equal(await composer.inputValue(), bigDraft, "大草稿必须保留");
  // 内部滚动必须能到达每个请求的 footer；层内不横溢出。
  const layerNode = page.locator(".pending-request-layer");
  assert.ok(
    await layerNode.evaluate(
      (node) => node.scrollWidth <= node.clientWidth + 1,
    ),
    "浮层不得横向溢出",
  );
  for (let i = 0; i < 2; i += 1) {
    await cardsD.nth(i).getByRole("button", { name: "发送回复", exact: true })
      .scrollIntoViewIfNeeded();
    assert.ok(
      await cardsD.nth(i)
        .getByRole("button", { name: "发送回复", exact: true })
        .isVisible(),
      `第 ${i + 1} 个请求的 footer 必须可达`,
    );
  }
  assert.equal(
    await composer.inputValue(),
    bigDraft,
    "浮层交互不得修改主草稿",
  );

  assert.deepEqual(errors, [], "无页面错误");
  assert.deepEqual(consoleErrors, [], "无控制台错误");
  report = {
    passed: true,
    answerCalls: answerCalls().length,
    checks:
      "snapshot/event recovery (event request without runId, answer RPC id=event.runId), persisted-snapshot sees open request until ACK, no-auto-submit, multi-question nav, draft/collapse persistence, retry same key across switch / changed answer new key, stale snapshot & reload & switch no-resurrect, handoff submit-only, same text new id, tricky ids/labels, late callback isolation, same-conversation geometry, two-axis centering in available area, header/body left edge & 16px radio first-line & label/description alignment, footer row alignment, cancelled run stale snapshot & replayed event & cancel.requested no panel, blocked-then-retried run question recovery, approval overlay busy composer, needsReview no note / cancelled 已停止 via real messages, normal mining screenshot desktop/mobile, pressure geometry desktop/mobile/short-window two awaitingUser runs with preset draft, no console errors",
    screenshots: [
      path.join(shotDir, "questions-desktop.png"),
      path.join(shotDir, "questions-mobile.png"),
    ],
  };
} finally {
  await browser?.close();
  await server?.close();
}
console.log(JSON.stringify(report, null, 2));
