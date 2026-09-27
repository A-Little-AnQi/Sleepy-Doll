// Exercise the real transcript with in-memory messages, without an Agent or BGI.
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
import {Transcript} from '/src/components/chat/Transcript.tsx';
import '/src/product.css';
function Fixture(){
 const [state,setState]=useState({messages:[],stream:'',phase:'等待模型响应',seconds:1,toolLabels:{},tasks:[],running:true});
 const [generation,setGeneration]=useState(0);
 window.setFixture=(next,remount=false)=>{setState(next);if(remount)setGeneration(x=>x+1)};
 return React.createElement('div',{style:{padding:32,width:800}},React.createElement(Transcript,{key:generation,...state}));
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
  const page = await browser.newPage();
  page.on("pageerror", (error) => console.error(error.message));
  page.on("response", (response) => {
    if (response.status() >= 400)
      console.error(response.status(), response.url());
  });
  await page.route("**/__transcript_fixture.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(reactUrl))
        .replace("'react-dom/client'", JSON.stringify(domUrl)),
    }),
  );
  const fixtureHtml = await server.transformIndexHtml(
    "/__fixture",
    '<html data-theme="dark"><head></head><body><div id="root"></div><script type="module" src="/__transcript_fixture.js"></script></body></html>',
  );
  await page.route("**/__fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: fixtureHtml,
    }),
  );
  await page.goto(`http://127.0.0.1:${port}/__fixture`);
  await page.waitForFunction(() => Boolean(window.setFixture), undefined, {
    timeout: 5000,
  });
  const task = {
    id: "run",
    conversationId: "chat",
    state: "executing",
    createdAt: "2026-09-27T00:00:00Z",
    updatedAt: "2026-09-27T00:00:38Z",
  };
  const message = (content, toolCalls = []) => ({
    role: "assistant",
    content,
    toolCalls,
    runId: "run",
  });
  const call = {
    id: "tool1",
    name: "bgi.api.invoke",
    arguments: { methodId: "bgi.delete_local_resource" },
  };
  const messages = [
    { role: "user", content: "血斛的路线删一下", runId: "run" },
    message("继续。先检查血斛路线的真实文件和删除范围。", [call]),
    {
      role: "tool",
      toolCallId: "tool1",
      content: '{"ok":false,"error":"版本已变化"}',
      runId: "run",
    },
  ];
  let state = {
    messages,
    stream: "",
    phase: "等待模型响应",
    seconds: 38,
    toolLabels: { "bgi.api.invoke": "执行 BetterGI 操作" },
    tasks: [task],
    currentTask: task,
    running: true,
  };
  const update = (remount = false) =>
    page.evaluate(({ state, remount }) => window.setFixture(state, remount), {
      state,
      remount,
    });
  await update();
  const disclosure = page.locator(".assistant .process-disclosure");
  await disclosure
    .locator(".assistant-message")
    .getByText("继续。先检查血斛路线的真实文件和删除范围。", { exact: true })
    .waitFor();
  assert.equal(await disclosure.getAttribute("data-expanded"), "true");
  assert.equal(
    await disclosure.locator(".process-disclosure-summary").isVisible(),
    false,
    "running progress should not sit behind a thinking/status heading",
  );
  await page.evaluate(
    () =>
      (window.fixtureProcess = document.querySelector(
        ".assistant .process-disclosure",
      )),
  );
  for (const seconds of [39, 40, 41]) {
    state = { ...state, seconds };
    await update();
  }
  assert(
    await page.evaluate(
      () =>
        window.fixtureProcess ===
        document.querySelector(".assistant .process-disclosure"),
    ),
    "status updates remounted process",
  );
  state = {
    ...state,
    messages: [
      ...messages,
      message("版本变了，我重新读取目录后继续删除。", [
        { ...call, id: "tool2" },
      ]),
    ],
  };
  await update();
  await disclosure
    .getByText("版本变了，我重新读取目录后继续删除。", { exact: true })
    .waitFor();
  const final = "已删除血斛的五条路线，其他路线保留。";
  state = { ...state, stream: final };
  await update();
  await disclosure.getByText(final, { exact: true }).waitFor();
  state = {
    ...state,
    stream: "",
    messages: [...state.messages, message(final)],
    running: false,
    currentTask: { ...task, state: "answered" },
  };
  await update();
  await page.waitForFunction(
    () =>
      document.querySelector("[data-answer]")?.textContent ===
      "已删除血斛的五条路线，其他路线保留。",
  );
  assert.equal(await disclosure.getAttribute("data-expanded"), "false");
  assert.equal(
    await page.locator("[data-answer]").textContent(),
    final,
    "streamed final restarted or blanked when persisted",
  );
  assert.equal(
    await disclosure.locator(".turn-process-content").count(),
    0,
    "completed process remained visible",
  );
  await disclosure.locator(".process-disclosure-summary").click();
  await disclosure
    .getByText("继续。先检查血斛路线的真实文件和删除范围。", { exact: true })
    .waitFor();
  await update(true);
  await page.waitForFunction(
    () =>
      window.fixtureProcess !==
      document.querySelector(".assistant .process-disclosure"),
  );
  assert.equal(
    await page
      .locator(".assistant .process-disclosure")
      .getAttribute("data-expanded"),
    "false",
    "history flashed open before collapsing",
  );
  assert.equal(await page.locator(".turn-process-content").count(), 0);
  state = {
    ...state,
    messages: [state.messages[0], message(final)],
    running: true,
  };
  await update(true);
  await page.waitForFunction(
    () =>
      document.querySelector("[data-answer]")?.textContent ===
      "已删除血斛的五条路线，其他路线保留。",
  );
  assert.equal(
    await page.locator(".process-disclosure").isVisible(),
    false,
    "a plain answer should not show an empty process header",
  );
  console.log(
    JSON.stringify({
      liveProgressVisible: true,
      statusNodePreserved: true,
      finalCollapsesProcess: true,
      streamedFinalPreserved: true,
      historyStartsCollapsed: true,
      realDataTouched: false,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
