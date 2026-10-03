// 主输入收尾语义检查：真实 ChatPage/Session/API，fake IPC 不启动模型或 BetterGI。
// 覆盖：busy 主 composer 禁用且草稿保留、只有停止可点、技能菜单同样禁用；
// run.succeeded 交接后恢复输入，发送只走 task.submit（不再 run.input 补充）；
// 失败同内容重试同 clientKey；legacy 问答内部 run.input 兼容保留。
// 资源登记：本地 vite 服务与 Chromium profile 仅属本任务，临时文件限
// target/.tmp/shortcut-runtime，脚本结束时精确关闭 server/browser。
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
import {ChatPage} from '/src/pages/chat/ChatPage.tsx';
import {PREVIEW_PERMISSION} from '/src/ipc/types.ts';
import '/src/product.css';
import '/src/motion.css';
const bootstrap={models:[{id:'fixture-model',name:'智谱 GLM 很长的模型名称',model:'fixture',active:true,contextWindow:256000}],
 conversations:[{id:'composer-fixture',title:'Fixture',modelId:'fixture-model'},{id:'busy-fixture',title:'Busy',modelId:'fixture-model'}],
 tools:[],skills:[{name:'check',label:'检查',enabled:true,available:true}],
 permission:PREVIEW_PERMISSION,runtimeToolLabels:{}};
function Fixture(){
 const [conversationId,setConversation]=React.useState('composer-fixture');
 window.showNew=()=>setConversation(undefined);
 window.showBusy=()=>setConversation('busy-fixture');
 window.showQuestion=()=>setConversation('question-fixture');
 return React.createElement('div',{style:{height:'100vh'}},
  React.createElement(ChatPage,{bootstrap,conversationId,onConversation:setConversation,reload:async()=>{}}));
}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
`;
let server;
let browser;
let report;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false },
  });
  await server.listen();
  console.error(
    `[composer-ui-check] vite 127.0.0.1:${server.httpServer.address().port}（本任务专用，finally 关闭）`,
  );
  const entry = await server.transformRequest("/src/main.tsx");
  const reactUrl = entry.code.match(/from "([^"]*\/react\.js\?[^"]*)"/)[1];
  const domUrl = entry.code.match(
    /from "([^"]*\/react-dom_client\.js\?[^"]*)"/,
  )[1];
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 850 },
  });
  page.setDefaultTimeout(5000);
  const errors = [];
  const consoleErrors = [];
  const calls = [];
  let rejectNextSubmit = false;
  let rejectNextInput = false;
  let busyState = "executing";
  let resumeQuestion = false;
  let seq = 10;
  const events = { "busy-fixture": [] };
  const pushEvent = (conv, kind, runId, data) =>
    events[conv]?.push({
      sequence: ++seq,
      kind,
      runId,
      conversationId: conv,
      data,
    });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    const text = message.text();
    // vite 的 HMR WebSocket 在纯内存夹具里连不上，属于环境噪音不算页面错误。
    if (message.type() === "error" && !/\[vite\]|hmr|websocket/i.test(text))
      consoleErrors.push(text);
  });
  await page.addInitScript(() =>
    localStorage.setItem("sleepy-doll-locale", "zh"),
  );
  const question =
    "需要确认 **挖矿配置**。请告诉我：①要用哪个脚本（莉奈或矿产资源批发）？②要调整哪些项目（运行时长、区域、队伍）？③如果沿用现有参数，请说明。";
  const task = {
    id: "active-run",
    conversationId: "composer-fixture",
    prompt: "运行现有任务",
    state: "deciding",
    createdAt: "2026-09-28T00:00:00Z",
    updatedAt: "2026-09-28T00:00:01Z",
  };
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    calls.push(request);
    const conv = request.params?.conversationId ?? request.params?.id ?? "";
    let result = {};
    if (request.method === "conversation.get")
      result = {
        id: conv,
        messages: [],
        runs:
          conv === "busy-fixture"
            ? [{ ...task, conversationId: conv, state: busyState }]
            : conv === "question-fixture"
              ? [
                  {
                    ...task,
                    conversationId: conv,
                    state: resumeQuestion ? "succeeded" : "awaitingUser",
                  },
                ]
              : [],
        questionRequests:
          conv === "question-fixture" && !resumeQuestion
            ? [
                {
                  requestId: "legacy",
                  runId: "active-run",
                  legacy: true,
                  questions: [
                    { id: "answer", header: "", question, options: [] },
                  ],
                },
              ]
            : [],
      };
    if (request.method === "events.read") {
      const list =
        conv === "question-fixture"
          ? [
              {
                sequence: 1,
                kind: "question",
                runId: "active-run",
                conversationId: "question-fixture",
                data: { question },
              },
              ...(resumeQuestion
                ? [
                    {
                      sequence: 2,
                      kind: "run.changed",
                      runId: "active-run",
                      conversationId: "question-fixture",
                      data: {
                        ...task,
                        revision: 2,
                        conversationId: "question-fixture",
                        state: "succeeded",
                      },
                    },
                  ]
                : []),
            ]
          : (events[conv] ?? []);
      result = {
        events: list.filter((event) => event.sequence > request.params.after),
      };
    }
    if (request.method === "task.submit") {
      if (rejectNextSubmit) {
        rejectNextSubmit = false;
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
      result = {
        ...task,
        id: "new-run",
        conversationId: conv || "new-fixture",
        prompt: request.params.prompt,
      };
    }
    if (request.method === "run.input") {
      const reject = rejectNextInput;
      rejectNextInput = false;
      if (reject) {
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
      result = { accepted: true };
    }
    if (request.method === "task.cancel")
      result = { ...task, conversationId: conv, state: "cancelled" };
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.route("**/__composer_fixture.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(reactUrl))
        .replace("'react-dom/client'", JSON.stringify(domUrl)),
    }),
  );
  const html = await server.transformIndexHtml(
    "/__fixture",
    '<html data-theme="dark"><body><div id="root"></div><script type="module" src="/__composer_fixture.js"></script></body></html>',
  );
  await page.route("**/__fixture", (route) =>
    route.fulfill({ contentType: "text/html", body: html }),
  );
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/__fixture`,
  );
  const input = page.getByRole("textbox", { name: "消息" });
  await input.waitFor();
  await page.waitForFunction(
    () =>
      document.querySelector("textarea")?.placeholder ===
      "告诉我你想完成什么",
  );
  const geometry = () =>
    input.evaluate((node) => {
      const style = getComputedStyle(node);
      const placeholder = getComputedStyle(node, "::placeholder");
      return {
        height: node.getBoundingClientRect().height,
        font: style.fontFamily,
        placeholderFont: placeholder.fontFamily,
        size: style.fontSize,
        placeholderSize: placeholder.fontSize,
        line: style.lineHeight,
        placeholderLine: placeholder.lineHeight,
        top: parseFloat(style.paddingTop),
        bottom: parseFloat(style.paddingBottom),
        scroll: node.scrollHeight,
      };
    });
  const empty = await geometry();
  assert.equal(empty.font, empty.placeholderFont);
  assert.equal(empty.size, empty.placeholderSize);
  assert.equal(empty.line, empty.placeholderLine);
  assert.equal(empty.top, empty.bottom);
  assert.equal(empty.height, parseFloat(empty.line) + empty.top + empty.bottom);
  assert.match(empty.font, /Microsoft YaHei UI/);
  const images = [];
  if (process.argv.includes("--visual")) {
    await input.focus();
    images.push(
      (await input.screenshot({ caret: "initial" })).toString("base64"),
    );
  }
  await input.fill("中文输入 Abc 123");
  assert.equal((await geometry()).height, empty.height);
  if (process.argv.includes("--visual"))
    images.push(
      (await input.screenshot({ caret: "initial" })).toString("base64"),
    );
  await input.fill("第一行\n第二行\n第三行");
  assert.equal(
    (await geometry()).height,
    3 * parseFloat(empty.line) + empty.top + empty.bottom,
  );
  await input.fill("中文换行检查".repeat(18));
  const wide = await geometry();
  await page.setViewportSize({ width: 500, height: 850 });
  await page.waitForFunction(
    (height) => document.querySelector("textarea").clientHeight > height,
    wide.height,
  );
  await input.fill("一行\n".repeat(30));
  assert.equal((await geometry()).height, 180);
  assert.ok((await geometry()).scroll > 180);
  await input.fill("");
  assert.equal((await geometry()).height, empty.height);
  assert.equal(
    await page.getByRole("button", { name: /排队发送|取消排队/ }).count(),
    0,
  );
  await page.setViewportSize({ width: 1100, height: 850 });
  // 空闲发送：只走 task.submit。
  await input.fill("新的消息");
  await input.press("Shift+Enter");
  assert.match(await input.inputValue(), /\n/);
  await input.press("Enter");
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").value === "" &&
      !document.querySelector("textarea").disabled,
  );
  const submits = calls.filter((call) => call.method === "task.submit");
  assert.equal(submits.length, 1);
  assert.equal(submits[0].params.conversationId, "composer-fixture");
  assert.equal(submits[0].params.prompt, "新的消息");
  assert.equal(calls.filter((call) => call.method === "run.input").length, 0);
  // 发送失败：同内容重试同 clientKey，草稿恢复。
  rejectNextSubmit = true;
  await input.fill("重试消息");
  await input.press("Enter");
  await page.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(await input.inputValue(), "重试消息");
  await input.press("Enter");
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").value === "" &&
      !document.querySelector("textarea").disabled,
  );
  const retries = calls.filter(
    (call) =>
      call.method === "task.submit" && call.params.prompt === "重试消息",
  );
  assert.equal(retries.length, 2);
  assert.ok(retries[0].params.clientKey);
  assert.equal(retries[0].params.clientKey, retries[1].params.clientKey);

  // ---- busy：主输入禁用、草稿保留、只有停止；技能菜单同样禁用 ----
  await page.evaluate(() =>
    localStorage.setItem("sleepy-doll-draft:busy-fixture", "忙碌时的草稿"),
  );
  await page.evaluate(() => window.showBusy());
  await page.waitForFunction(() =>
    document.querySelector("textarea").disabled,
  );
  assert.equal(await input.inputValue(), "忙碌时的草稿", "busy 必须保留草稿");
  assert.equal(
    await page.locator(".composer-submit .send-action").count(),
    1,
    "busy 只有一个操作按钮",
  );
  const stop = page.getByRole("button", { name: "停止" });
  assert.ok(await stop.isVisible());
  assert.ok(await stop.isEnabled(), "busy 时停止必须可点");
  assert.ok(
    await page.getByRole("button", { name: "选择技能" }).isDisabled(),
    "busy 时技能菜单必须禁用",
  );
  assert.equal(
    await page.getByText(/排队发送|取消排队|补充说明/).count(),
    0,
    "不得出现发送/补充/排队提示",
  );
  await stop.click();
  await sleep(300);
  assert.ok(
    calls.some(
      (call) =>
        call.method === "task.cancel" && call.params.id === "active-run",
    ),
    "busy 时点击停止必须发起 task.cancel",
  );
  assert.equal(await input.inputValue(), "忙碌时的草稿");

  // ---- 模拟 run.succeeded 后台交接：输入恢复，发送只 task.submit ----
  busyState = "succeeded";
  pushEvent("busy-fixture", "run.changed", "active-run", {
    ...task,
    id: "active-run",
    conversationId: "busy-fixture",
    state: "succeeded",
    revision: 2,
  });
  await page.waitForFunction(
    () => !document.querySelector("textarea").disabled,
  );
  assert.equal(await input.inputValue(), "忙碌时的草稿", "交接后草稿仍在");
  await input.fill("交接后的新消息");
  await input.press("Enter");
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").value === "" &&
      !document.querySelector("textarea").disabled,
  );
  const submitsAfter = calls.filter((call) => call.method === "task.submit");
  assert.equal(submitsAfter.length, 4);
  assert.equal(submitsAfter[3].params.conversationId, "busy-fixture");
  assert.equal(submitsAfter[3].params.prompt, "交接后的新消息");
  assert.equal(
    calls.filter((call) => call.method === "run.input").length,
    0,
    "主聊天发送不得走 run.input",
  );

  // ---- legacy 问答（run.input 兼容）：浮层可填写，主输入 busy 禁用 ----
  await page.evaluate(() => window.showQuestion());
  const card = page.locator(".pending-request-card");
  await card.waitFor();
  assert.ok(
    await input.isDisabled(),
    "等待回答期间主输入保持禁用",
  );
  assert.equal(await card.locator("strong").innerText(), "挖矿配置");
  const questionText = await card.locator(".pending-request-text").innerText();
  assert.match(questionText, /①.*②.*③/s, "三个问题项都要渲染");
  assert.equal(
    await card.getByText("等待你的回复", { exact: true }).count(),
    1,
  );
  const replyBox = card.getByRole("textbox", { name: "填写回复" });
  await replyBox.waitFor();
  const replyButton = page.getByRole("button", {
    name: "发送回复",
    exact: true,
  });
  assert.ok(await replyButton.isVisible());
  assert.ok(await replyButton.isDisabled());
  await replyBox.fill("莉奈，队伍采矿，运行30分钟");
  assert.ok(await replyButton.isEnabled());
  // 收起后浮层变成窄条入口，可重新打开。
  await card.getByRole("button", { name: "收起面板" }).click();
  const chip = page.getByRole("button", { name: /请回答后继续/ });
  await chip.waitFor();
  await chip.click();
  assert.ok(await card.isVisible());
  // Capture only in memory; never write screenshots or touch user data.
  if (process.argv.includes("--visual"))
    images.push((await page.screenshot()).toString("base64"));
  for (const width of [360, 800, 1100]) {
    await page.setViewportSize({ width, height: 850 });
    await sleep(400);
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(resolve)),
    );
    const overflow = await page
      .locator(".composer-dock")
      .evaluate((node) => [node.scrollWidth, node.clientWidth]);
    assert.ok(
      overflow[0] <= overflow[1] + 1,
      `composer-dock 横向溢出 ${JSON.stringify(overflow)} @${width}`,
    );
    assert.ok(
      await page.locator(".composer-model").evaluate((node) => {
        const parent = node.getBoundingClientRect();
        const trigger = node
          .querySelector('[data-ui="select-trigger"]')
          .getBoundingClientRect();
        return trigger.right <= parent.right + 1;
      }),
    );
  }
  rejectNextInput = true;
  await replyButton.click();
  await page.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(await replyBox.inputValue(), "莉奈，队伍采矿，运行30分钟");
  assert.ok(await card.isVisible());
  await replyButton.click();
  // ACK 后请求整体移除，不再渲染「回复已发送」chip。
  await page.waitForFunction(
    () => !document.querySelector(".pending-request-card"),
  );
  assert.equal(await replyButton.count(), 0);
  const answers = calls.filter(
    (call) =>
      call.method === "run.input" && call.params.content.includes("运行30分钟"),
  );
  assert.equal(answers.length, 2);
  assert.equal(answers[0].params.clientKey, answers[1].params.clientKey);
  resumeQuestion = true;
  await page.waitForFunction(
    () => !document.querySelector(".pending-request-card"),
  );
  assert.equal(await replyButton.count(), 0);
  await page.waitForFunction(
    () => !document.querySelector("textarea").disabled,
  );
  assert.ok(
    await input.isEnabled(),
    "问答结束后主输入恢复可用",
  );
  assert.deepEqual(errors, [], "无页面错误");
  assert.deepEqual(consoleErrors, [], "无控制台错误");
  report = {
    passed: true,
    empty,
    checks:
      "font/baseline, multiline/resize/limit, no queue, idle submit + retry same clientKey, busy composer+skillmenu disabled draft kept stop clickable, handoff restores input submit-only, legacy question run.input compat, mobile overflow",
    images,
  };
} finally {
  await browser?.close();
  await server?.close();
}
console.log(JSON.stringify(report));
