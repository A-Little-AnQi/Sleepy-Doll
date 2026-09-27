import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const fixture = `import React from 'react';import ReactDOM from 'react-dom/client';import {BridgeRecovery} from '/src/components/bridge/BridgeRecovery.tsx';import '/src/product.css';import '/src/motion.css';ReactDOM.createRoot(document.getElementById('root')).render(React.createElement('div',{style:{padding:32,maxWidth:1000}},React.createElement(BridgeRecovery,{onBack:()=>{}})));`;
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
  const port = server.httpServer.address().port;
  console.log(
    JSON.stringify({
      port,
      cleanup: "finally closes Vite and Chromium",
      diskFixtures: false,
    }),
  );
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1200, height: 900 },
  });
  page.setDefaultTimeout(6000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem("sleepy-doll-locale", "zh");
    localStorage.setItem("sleepy-doll-theme", "dark");
  });
  const exe = "D:/BetterGI/BetterGI.exe";
  const args = "genshinStartConfig.genshinStartArgs";
  const other = "maskWindowConfig.enabled";
  let running = true;
  let failRestore = false;
  let restoreCalls = [];
  let previews = [];
  const base = {
    recordVersion: "record",
    currentVersion: "current",
    createdAt: "2026-09-24T03:41:13Z",
    configPath: "D:/BetterGI/User/config.json",
    hostExecutable: exe,
    canPreview: true,
    canRestore: false,
  };
  let records = [
    {
      ...base,
      changeId: "changed",
      kind: "change",
      paths: [args, other],
      fields: [
        { path: args, label: "原神启动参数" },
        { path: other, label: "日志遮罩开关" },
      ],
    },
    ...["snapshot-a", "snapshot-b"].map((changeId) => ({
      ...base,
      changeId,
      kind: "snapshot",
      paths: [],
      fields: [],
      snapshotDigest: "same-snapshot",
      state: "commandCheckpoint",
    })),
  ];
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    let result = {};
    if (q.method === "bridge.recoveryList")
      result = {
        hostRunning: running,
        runningTargets: running ? [exe] : [],
        records,
      };
    if (q.method === "bridge.recoveryStatus")
      result = { runningTargets: running ? [exe] : [] };
    if (q.method === "bridge.recoveryPreview") {
      previews.push(q.params);
      const selected =
        q.params.mode === "full" ? [args, other] : q.params.paths;
      result = {
        ...base,
        ...q.params,
        online: q.params.mode === "fields" && running,
        planId: "plan-" + selected.length,
        canApply: q.params.mode === "fields" || !running,
        hostRunning: running,
        reason:
          q.params.mode === "full" && running
            ? "完整恢复需要退出 BetterGI，避免自动保存覆盖。"
            : null,
        differences: selected.map((field) => ({
          path: field,
          label: field === args ? "原神启动参数" : "日志遮罩开关",
          current: field === args ? "--new" : false,
          restore: field === args ? "--old" : true,
          changed: true,
          laterChanged: field === args,
        })),
      };
    }
    if (q.method === "bridge.restore") {
      restoreCalls.push(q.params);
      if (failRestore) {
        failRestore = false;
        await route.fulfill({
          contentType: "application/json",
          body: JSON.stringify({
            id: q.id,
            ok: false,
            error: { message: "预览后配置已变化，没有覆盖。请重新预览。" },
          }),
        });
        return;
      }
      records = [
        {
          ...base,
          changeId: "undo",
          kind: "change",
          paths: [args],
          fields: [{ path: args, label: "原神启动参数" }],
          operation: "setting-restore",
        },
        ...records,
      ];
      result = {
        restored: true,
        online: q.params.online,
        recoveryChangeId: "undo",
      };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });
  await page.route("**/__recovery.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fixture
        .replace("'react'", JSON.stringify(react))
        .replace("'react-dom/client'", JSON.stringify(dom)),
    }),
  );
  const html = await server.transformIndexHtml(
    "/__recovery",
    '<html data-theme="dark"><body><div id="root"></div><script type="module" src="/__recovery.js"></script></body></html>',
  );
  await page.route("**/__recovery", (route) =>
    route.fulfill({ contentType: "text/html", body: html }),
  );
  await page.goto(`http://127.0.0.1:${port}/__recovery`);
  const rows = page.locator(".recovery-row");
  await rows.first().waitFor();
  assert.equal(
    await rows.count(),
    1,
    "Snapshots were mixed into setting changes",
  );
  assert.equal(
    await rows.first().getByRole("button", { name: "查看与恢复" }).isEnabled(),
    true,
    "Running BetterGI blocks even preview",
  );
  await rows.first().getByRole("button", { name: "查看与恢复" }).click();
  let dialog = page.getByRole("dialog", { name: "恢复所选设置" });
  await dialog.getByRole("button", { name: "恢复所选 2 项" }).waitFor();
  await page.waitForFunction(
    () => document.querySelectorAll(".recovery-difference").length === 2,
  );
  assert.equal(restoreCalls.length, 0, "Opening preview restored config");
  await dialog.getByRole("checkbox", { name: "日志遮罩开关" }).uncheck();
  await page.waitForFunction(
    () => document.querySelectorAll(".recovery-difference").length === 1,
  );
  await dialog.getByText("后来还改过，此次会替换", { exact: true }).waitFor();
  if (process.env.RECOVERY_VISUAL)
    console.log(
      "RECOVERY_IMAGE:" + (await page.screenshot()).toString("base64"),
    );
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(restoreCalls.length, 0);
  await rows.first().getByRole("button", { name: "查看与恢复" }).click();
  dialog = page.getByRole("dialog", { name: "恢复所选设置" });
  await dialog.getByRole("checkbox", { name: "日志遮罩开关" }).uncheck();
  await page.waitForFunction(
    () => document.querySelectorAll(".recovery-difference").length === 1,
  );
  await dialog.getByRole("button", { name: "恢复所选 1 项" }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.deepEqual(restoreCalls[0].paths, [args]);
  assert.equal(restoreCalls[0].online, true);
  await page.getByRole("button", { name: "查看并撤销本次恢复" }).click();
  dialog = page.getByRole("dialog", { name: "恢复所选设置" });
  await page.waitForFunction(
    () => document.querySelectorAll(".recovery-difference").length === 1,
  );
  assert(previews.some((value) => value.changeId === "undo"));
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  await page
    .locator(".sd-tabs")
    .getByRole("button", { name: /完整备份/ })
    .click();
  assert.equal(
    await rows.count(),
    1,
    "Identical snapshots were not deduplicated",
  );
  await rows.first().getByRole("button", { name: "查看与恢复" }).click();
  dialog = page.getByRole("dialog", { name: "恢复完整备份" });
  await dialog
    .getByText("完整恢复需要退出 BetterGI，避免自动保存覆盖。", { exact: true })
    .waitFor();
  const confirm = dialog.getByRole("button", { name: "确认恢复完整配置" });
  assert.equal(await confirm.isEnabled(), false);
  running = false;
  await page.waitForFunction(
    () =>
      document.querySelector(".recovery-preview-dialog .primary-action")
        ?.disabled === false,
    {},
    { timeout: 6500 },
  );
  assert.equal(
    await confirm.isEnabled(),
    true,
    "Closing BetterGI did not refresh recovery availability",
  );
  failRestore = true;
  await confirm.click();
  await dialog.getByRole("alert").waitFor();
  assert.equal(
    await confirm.isEnabled(),
    false,
    "A failed restore left the stale preview executable",
  );
  await dialog.getByRole("button", { name: "重新核对当前配置" }).click();
  await page.waitForFunction(
    () =>
      document.querySelector(".recovery-preview-dialog .primary-action")
        ?.disabled === false,
  );
  await page.setViewportSize({ width: 920, height: 650 });
  const bounds = await dialog.boundingBox();
  assert(
    bounds.y >= 0 && bounds.y + bounds.height <= 650,
    "Recovery preview overflows short desktop window",
  );
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify({
      settingChangesSeparatedFromSnapshots: true,
      snapshotDeduplication: true,
      previewEnabledWhileRunning: true,
      explicitConfirmation: true,
      partialSelection: true,
      laterChangesExplained: true,
      undoFromNewBackup: true,
      hostExitAutoRefresh: true,
      failedRestoreRequiresNewPreview: true,
      shortWindowFits: true,
      realUserDataTouched: false,
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
