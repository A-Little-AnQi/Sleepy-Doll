// UI integration regression: fake IPC only; no BetterGI process, injection or model requests.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import ts from "typescript";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const source = await fs.readFile(
  path.join(root, "web/src/session/bridge-status.ts"),
  "utf8",
);
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2023,
    module: ts.ModuleKind.ESNext,
  },
}).outputText;
const { watchBridgeStatus, applyBridgeStatus } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
globalThis.window = new EventTarget();
globalThis.document = Object.assign(new EventTarget(), {
  visibilityState: "visible",
});
const offline = {
  enabled: true,
  connected: false,
  baseUrl: "http://127.0.0.1:26101",
  launchSilently: true,
};
const online = { ...offline, connected: true };
const initial = { bridge: offline, tasks: [], conversations: [], sentinel: {} };
assert.equal(applyBridgeStatus(initial, { ...offline }), initial);
const patched = applyBridgeStatus(initial, online);
assert.equal(patched.tasks, initial.tasks);
assert.equal(patched.conversations, initial.conversations);
assert.equal(patched.sentinel, initial.sentinel);
assert.equal(initial.bridge.connected, false);
let generation = 0;
let requests = 0;
let resolve;
const changes = [];
const stop = watchBridgeStatus({
  interval: 10,
  generation: () => generation,
  read: () => {
    requests++;
    return new Promise((done) => {
      resolve = done;
    });
  },
  publish: (value) => changes.push(value),
});
window.dispatchEvent(new Event("focus"));
await delay(25);
assert.equal(requests, 1, "slow probes overlapped");
generation++;
resolve(offline);
await delay(5);
assert.equal(changes.length, 0, "old probe overwrote newer reload");
stop();
let lateResolve;
const stopLate = watchBridgeStatus({
  generation: () => generation,
  read: () =>
    new Promise((done) => {
      lateResolve = done;
    }),
  publish: (value) => changes.push(value),
});
stopLate();
lateResolve(online);
await delay(5);
assert.equal(changes.length, 0, "disposed monitor still published");

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
    JSON.stringify({
      resource: "headless UI fixture",
      port,
      cleanup: "finally closes browser and Vite server",
    }),
  );
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on("pageerror", (error) => console.error(error.message));
  let connected = false;
  let bridgeError;
  let bootstrapCalls = 0;
  let statusCalls = 0;
  const bootstrap = () => ({
    configPath: "fixture/config.json",
    models: [],
    skills: [],
    tools: [],
    runtimeToolLabels: {},
    plugins: [
      {
        manifest: {
          id: "bgi",
          name: "BGI",
          version: "0.1.0",
          description: "fixture",
        },
        configuredEnabled: true,
        host: true,
        status: "active",
      },
    ],
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
    bridge: { ...offline, connected, error: bridgeError },
  });
  await page.addInitScript(() => {
    localStorage.setItem("sleepy-doll-version", "0.1.0");
    localStorage.setItem("sleepy-doll-locale", "zh");
  });
  await page.route("**/ipc", async (route) => {
    const request = route.request().postDataJSON();
    let result = {};
    if (request.method === "bootstrap") {
      bootstrapCalls++;
      result = bootstrap();
    }
    if (request.method === "bridge.status") {
      statusCalls++;
      result = { ...offline, connected, error: bridgeError };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: request.id, ok: true, result }),
    });
  });
  await page.goto(`http://127.0.0.1:${port}`);
  await page.waitForFunction(
    () =>
      document.querySelector(".app-connection small")?.textContent === "未连接",
    undefined,
    { timeout: 7000 },
  );
  await page.evaluate(() => {
    window.fixtureComposer = document.querySelector("textarea");
  });
  const before = bootstrapCalls;
  connected = true;
  await page.waitForFunction(
    () =>
      document.querySelector(".app-connection small")?.textContent === "已连接",
    undefined,
    { timeout: 7000 },
  );
  assert.equal(
    bootstrapCalls,
    before,
    "connection change reloaded the full app",
  );
  assert(
    await page.evaluate(
      () => window.fixtureComposer === document.querySelector("textarea"),
    ),
    "composer was remounted",
  );
  await page.locator(".app-connection").click();
  await page.waitForFunction(
    () =>
      document.querySelector(".bridge-connection-row strong")?.textContent ===
      "已连接",
  );
  connected = false;
  await page.waitForFunction(
    () =>
      document.querySelector(".app-connection small")?.textContent ===
        "未连接" &&
      document.querySelector(".bridge-connection-row strong")?.textContent ===
        "未连接",
    undefined,
    { timeout: 7000 },
  );
  connected = true;
  await page.waitForFunction(
    () =>
      document.querySelector(".app-connection small")?.textContent ===
        "已连接" &&
      document.querySelector(".bridge-connection-row strong")?.textContent ===
        "已连接",
    undefined,
    { timeout: 7000 },
  );
  assert(statusCalls >= 3);
  connected = false;
  bridgeError = "连接失败，请使用官方版本的BetterGI。";
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForFunction(
    () =>
      document.querySelector(".bridge-connection-row span")?.textContent ===
      "连接失败，请使用官方版本的BetterGI。",
    undefined,
    { timeout: 7000 },
  );
  connected = true;
  bridgeError = undefined;
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForFunction(
    () =>
      document.querySelector(".bridge-connection-row strong")?.textContent ===
      "已连接",
    undefined,
    { timeout: 7000 },
  );
  console.log(
    JSON.stringify({
      unit: "merge, no overlap, stale response and cleanup passed",
      ui: "sidebar and connection page disconnected→connected→disconnected→connected passed",
      fullBootstrapPolling: false,
      composerPreserved: true,
      realBgiTouched: false,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
