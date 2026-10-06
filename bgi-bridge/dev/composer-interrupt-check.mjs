// Real ChatPage/Session/API with controllable IPC. Owned server/browser/cache:
// target/.tmp/composer-interrupt; finally closes processes, root cleans cache.
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
const scratch = path.join(root, "target/.tmp/composer-interrupt");
fs.mkdirSync(scratch, { recursive: true });
process.env.TEMP = process.env.TMP = scratch;
const states = [
  "deciding",
  "executing",
  "awaitingApproval",
  "awaitingUser",
  "cancelling",
];
const fixture = `
import React from 'react';
import ReactDOM from 'react-dom/client';
import { ChatPage } from '/src/pages/chat/ChatPage.tsx';
import { PREVIEW_PERMISSION } from '/src/ipc/types.ts';
import '/src/product.css';
import '/src/motion.css';
const bootstrap = {models:[{id:'m',name:'Model',model:'fixture',active:true}],
conversations:${JSON.stringify([...states, "failure", "switch", "accepted"].map((id) => ({ id, title: id })))},
tools:[], skills:[{name:'create-shortcut',description:'创建快捷任务',enabled:true,available:true}], tasks:[],permission:PREVIEW_PERMISSION,runtimeToolLabels:{}};
function Fixture() {
 const [id,setId] = React.useState('deciding');
 window.showConv = setId;
 window.currentConv = id;
 return React.createElement('div',{style:{height:'100vh'}},React.createElement(ChatPage,{bootstrap,conversationId:id,onConversation:setId,reload:()=>window.holdReload ? new Promise(()=>{}) : Promise.resolve()}));
}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));`;
const run = (id, state = "deciding") => ({
  id: `run-${id}`,
  conversationId: id,
  prompt: "old",
  state,
  createdAt: "2026-10-07T00:00:00Z",
  updatedAt: "2026-10-07T00:00:01Z",
});
const calls = [];
const interrupted = new Set();
let held;
let server, browser;
try {
  server = await createServer({
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  const entry = await server.transformRequest("/src/main.tsx");
  const react = entry.code.match(/from "([^"]*\/react\.js\?[^"]*)"/)[1];
  const dom = entry.code.match(
    /from "([^"]*\/react-dom_client\.js\?[^"]*)"/,
  )[1];
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1100, height: 850 },
  });
  page.setDefaultTimeout(5000);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() =>
    localStorage.setItem("sleepy-doll-locale", "zh"),
  );
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    calls.push(request);
    const id = request.params?.conversationId ?? request.params?.id;
    let result = {};
    if (request.method === "conversation.get")
      result = {
        id,
        messages: [],
        runs: [run(id, states.includes(id) ? id : "deciding")],
        questionRequests:
          id === "awaitingUser"
            ? [
                {
                  runId: `run-${id}`,
                  requestId: "q",
                  questions: [
                    { id: "team", header: "队伍", question: "用哪个队伍？" },
                  ],
                },
              ]
            : [],
      };
    if (request.method === "events.read") {
      const approval = {
        id: "approval",
        runId: `run-${id}`,
        expiresAt: Math.floor(Date.now() / 1000) + 600,
        request: { methodId: "bgi.start_game", arguments: {} },
      };
      const events =
        id === "awaitingApproval"
          ? [
              {
                sequence: 1,
                runId: `run-${id}`,
                kind: "approval.requested",
                data: approval,
              },
            ]
          : [];
      if (interrupted.has(id)) {
        events.push({
          sequence: 2,
          runId: `run-${id}`,
          kind: "interrupt.requested",
          data: { reason: "newMessage" },
        });
        if (id === "awaitingApproval")
          events.push({
            sequence: 3,
            runId: `run-${id}`,
            kind: "approval.requested",
            data: approval,
          });
      }
      result = {
        events: events.filter((event) => event.sequence > request.params.after),
      };
    }
    if (request.method === "task.submit") {
      assert.equal(request.params.interruptActive, true);
      if (held) {
        held.request = request;
        await new Promise((resolve) => (held.release = resolve));
        const pending = held;
        held = undefined;
        if (pending.fail) {
          await route.fulfill({
            contentType: "application/json",
            body: JSON.stringify({
              id: request.id,
              ok: false,
              error: { message: "Rejected submission" },
            }),
          });
          return;
        }
      }
      result = run(id ?? "accepted", "queued");
      interrupted.add(id);
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.route("**/__composer_fixture.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(react))
        .replace("'react-dom/client'", JSON.stringify(dom)),
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
  const field = page.locator(".sd-composer-deck textarea");
  const plus = page.getByRole("button", { name: "快捷命令", exact: true });
  const send = page.getByRole("button", { name: "发送", exact: true });
  const pick = page.getByRole("menuitem", { name: "创建快捷任务" });
  for (const state of states) {
    await page.evaluate((id) => window.showConv(id), state);
    await page.waitForFunction((id) => window.currentConv === id, state);
    await page.waitForFunction(
      (state) =>
        document.querySelector(".chat-workspace") &&
        document.querySelector('.send-action[aria-label="发送"]') === null,
      state,
    );
    assert.ok(await field.isEditable(), `${state}: typing enabled`);
    assert.ok(await plus.isEnabled(), `${state}: + enabled`);
    await plus.click();
    await pick.click();
    assert.equal(await field.inputValue(), "$create-shortcut ");
    await field.fill("/");
    await pick.waitFor({ state: "visible" });
    await field.press("Enter");
    assert.equal(await field.inputValue(), "$create-shortcut ");
    await field.fill(`interrupt ${state}`);
    assert.ok(await send.isEnabled());
    await send.click();
    await page.waitForFunction(
      () => document.querySelector(".sd-composer-deck textarea").value === "",
    );
    if (state === "awaitingUser" || state === "awaitingApproval")
      await page.locator(".pending-request-card").waitFor({ state: "hidden" });
    await field.fill("");
  }
  assert.equal(calls.filter((x) => x.method === "task.submit").length, 5);
  // A delayed submit must not block editing or either command entry.
  await page.evaluate(() => window.showConv("failure"));
  await page.waitForFunction(() => window.currentConv === "failure");
  await field.fill("first");
  held = { fail: true };
  await send.click();
  await page.waitForFunction(
    () => document.querySelector(".sd-composer-deck textarea").value === "",
  );
  await field.fill("/");
  await pick.waitFor({ state: "visible" });
  await field.press("Enter");
  assert.ok(await plus.isEnabled());
  await field.fill("next draft");
  while (!held.release) await new Promise((r) => setTimeout(r, 10));
  held.release();
  await page.getByText("Rejected submission", { exact: true }).waitFor();
  assert.equal(await field.inputValue(), "next draft");
  // New conversation ACK migrates only the unsent draft; reload cannot hold send.
  await page.evaluate(() => {
    window.showConv(undefined);
    window.holdReload = true;
  });
  await page.waitForFunction(() => window.currentConv === undefined);
  await field.fill("new conversation");
  held = {};
  await send.click();
  await page.waitForFunction(
    () => document.querySelector(".sd-composer-deck textarea").value === "",
  );
  await field.fill("draft written before ACK");
  while (!held.release) await new Promise((r) => setTimeout(r, 10));
  held.release();
  await page.waitForFunction(() => window.currentConv === "accepted");
  assert.equal(await field.inputValue(), "draft written before ACK");
  assert.ok(await send.isEnabled(), "shell reload must not block next send");
  // Switching away during ACK must not navigate back or overwrite other drafts.
  held = {};
  await send.click();
  while (!held.release) await new Promise((r) => setTimeout(r, 10));
  await page.evaluate(() => window.showConv("switch"));
  await page.waitForFunction(() => window.currentConv === "switch");
  await field.fill("other conversation draft");
  held.release();
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.currentConv), "switch");
  assert.equal(await field.inputValue(), "other conversation draft");
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify({
      passed: true,
      states,
      checks: [
        "typing",
        "plus",
        "slash",
        "interrupt RPC",
        "failed submit draft",
        "new conversation draft",
        "reload independence",
        "switch isolation",
      ],
    }),
  );
} finally {
  if (held?.release) held.release();
  await browser?.close();
  await server?.close();
}
