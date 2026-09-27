// Real ChatPage/composer; fake IPC never starts a model or BetterGI.
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
 conversations:[{id:'composer-fixture',title:'Fixture',modelId:'fixture-model'}],tools:[],
 permission:PREVIEW_PERMISSION,runtimeToolLabels:{}};
function Fixture(){
 const [conversationId,setConversation]=React.useState('composer-fixture');
 window.showNew=()=>setConversation(undefined);
 window.showQuestion=()=>setConversation('question-fixture');
 return React.createElement('div',{style:{height:'100vh'}},
  React.createElement(ChatPage,{bootstrap,conversationId,onConversation:setConversation,reload:async()=>{}}));
}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
`;
let server;
let browser;
let report;
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
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 850 },
  });
  page.setDefaultTimeout(5000);
  const errors = [];
  const calls = [];
  let rejectNextInput = false;
  let resumeQuestion = false;
  const question =
    "需要确认 **挖矿配置**。请告诉我：①要用哪个脚本（莉奈或矿产资源批发）？②要调整哪些项目（运行时长、区域、队伍）？③如果沿用现有参数，请说明。";
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() =>
    localStorage.setItem("sleepy-doll-locale", "zh"),
  );
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
    let result = {};
    if (request.method === "conversation.get")
      result = {
        id: request.params.id,
        messages: [],
        runs: [
          {
            ...task,
            conversationId: request.params.id,
            state:
              request.params.id === "question-fixture" && !resumeQuestion
                ? "awaitingUser"
                : "deciding",
          },
        ],
      };
    if (request.method === "events.read") {
      const events =
        request.params.conversationId === "question-fixture"
          ? [
              {
                sequence: 1,
                kind: "question",
                runId: task.id,
                conversationId: "question-fixture",
                data: { question },
              },
              ...(resumeQuestion
                ? [
                    {
                      sequence: 2,
                      kind: "run.changed",
                      runId: task.id,
                      conversationId: "question-fixture",
                      data: {
                        ...task,
                        revision: 2,
                        conversationId: "question-fixture",
                      },
                    },
                  ]
                : []),
            ]
          : [];
      result = {
        events: events.filter((event) => event.sequence > request.params.after),
      };
    }
    if (request.method === "task.submit")
      result = {
        ...task,
        id: "new-run",
        conversationId: "new-fixture",
        prompt: request.params.prompt,
      };
    const reject = request.method === "run.input" && rejectNextInput;
    if (reject) rejectNextInput = false;
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({
        id: request.id,
        ok: !reject,
        result,
        ...(reject ? { error: { message: "Fixture retry" } } : {}),
      }),
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
  await page.waitForFunction(() =>
    document.querySelector("textarea")?.placeholder.includes("补充说明"),
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
  await input.fill("补充说明");
  await input.press("Enter");
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").value === "" &&
      !document.querySelector("textarea").disabled,
  );
  const first = calls.filter((call) => call.method === "run.input");
  assert.equal(first.length, 1);
  assert.equal(first[0].params.id, "active-run");
  assert.equal(calls.filter((call) => call.method === "task.submit").length, 0);
  rejectNextInput = true;
  await input.fill("重试补充");
  await input.press("Enter");
  await page.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(await input.inputValue(), "重试补充");
  await input.press("Enter");
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").value === "" &&
      !document.querySelector("textarea").disabled,
  );
  const retries = calls.filter(
    (call) => call.method === "run.input" && call.params.content === "重试补充",
  );
  assert.equal(retries.length, 2);
  assert.equal(retries[0].params.clientKey, retries[1].params.clientKey);
  await page.evaluate(() => window.showNew());
  await page.waitForFunction(
    () =>
      document.querySelector("textarea").placeholder === "告诉我你想完成什么",
  );
  await input.fill("新的消息");
  await input.press("Shift+Enter");
  assert.match(await input.inputValue(), /\n/);
  assert.equal(calls.filter((call) => call.method === "task.submit").length, 0);
  await input.press("Enter");
  await page.waitForFunction(() =>
    document.querySelector("textarea").placeholder.includes("补充说明"),
  );
  assert.equal(calls.filter((call) => call.method === "task.submit").length, 1);
  await page.evaluate(() => window.showQuestion());
  const card = page.getByRole("region", { name: "请回答后继续" });
  await card.waitFor();
  assert.equal(await card.locator("ol > li").count(), 3);
  assert.equal(await card.locator("strong").innerText(), "挖矿配置");
  assert.equal(
    await card.getByText("等待你的回复", { exact: true }).count(),
    1,
  );
  const replyButton = page.getByRole("button", {
    name: "发送回复",
    exact: true,
  });
  assert.ok(await replyButton.isVisible());
  assert.ok(await replyButton.isDisabled());
  await card.getByRole("button", { name: "填写回复" }).click();
  assert.ok(await input.evaluate((node) => document.activeElement === node));
  await input.fill("莉奈，队伍采矿，运行30分钟");
  assert.ok(await replyButton.isEnabled());
  await page.getByRole("button", { name: "查看问题" }).click();
  assert.ok(await card.isVisible());
  // Capture only in memory; never write screenshots or touch user data.
  if (process.argv.includes("--visual"))
    images.push((await page.screenshot()).toString("base64"));
  for (const width of [360, 800, 1100]) {
    await page.setViewportSize({ width, height: 850 });
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(resolve)),
    );
    assert.ok(
      await page
        .locator(".composer-dock")
        .evaluate((node) => node.scrollWidth <= node.clientWidth + 1),
    );
  }
  rejectNextInput = true;
  await replyButton.click();
  await page.getByText("Fixture retry", { exact: true }).waitFor();
  assert.equal(await input.inputValue(), "莉奈，队伍采矿，运行30分钟");
  assert.ok(await card.isVisible());
  await replyButton.click();
  await page.getByRole("region", { name: "回复已发送" }).waitFor();
  assert.ok(await replyButton.isDisabled());
  const answers = calls.filter(
    (call) =>
      call.method === "run.input" && call.params.content.includes("运行30分钟"),
  );
  assert.equal(answers.length, 2);
  assert.equal(answers[0].params.clientKey, answers[1].params.clientKey);
  assert.equal(answers[1].params.id, "active-run");
  assert.equal(calls.filter((call) => call.method === "task.submit").length, 1);
  resumeQuestion = true;
  await page.waitForFunction(() => !document.querySelector(".question-card"));
  assert.equal(await replyButton.count(), 0);
  assert.deepEqual(errors, []);
  report = {
    passed: true,
    empty,
    checks:
      "font/baseline, multiline/resize/limit, no queue, follow-up/retry, new message/Shift+Enter, formatted question/reply focus/submit/retry/resume/mobile",
    images,
  };
} finally {
  await browser?.close();
  await server?.close();
}
console.log(JSON.stringify(report));
