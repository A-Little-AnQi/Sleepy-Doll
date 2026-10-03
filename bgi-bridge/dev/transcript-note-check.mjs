// 对话结果标注检查：`needsReview` 状态不再渲染任何 turn-outcome-note，
// 「结果待核对」不再出现在对话流；`cancelled` 保留原有「已停止」标注。
// 在真实浏览器里渲染对话流（vite + playwright），只验证这一个呈现删除；
// 全程内存操作，结束时删除任务临时 shim。
import { fileURLToPath } from "node:url";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import http from "node:http";
import { createServer } from "vite";
import { chromium } from "playwright";

const webRoot = fileURLToPath(new URL("../../web/", import.meta.url));
const tempDir = fileURLToPath(
  new URL("../../target/.tmp/shortcut-runtime/", import.meta.url),
);
mkdirSync(tempDir, { recursive: true });
const shimPath = `${tempDir}transcript-shim.mjs`;
writeFileSync(
  shimPath,
  `// Transcript 渲染 shim：在真实浏览器里同步渲染两种任务状态，innerHTML 挂在
// window.__RESULTS 上供检查脚本读取。仅内存操作，检查结束即删除本文件。
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { Transcript } from "/src/components/chat/Transcript.tsx";

const task = (state) => ({
  id: "run-1",
  conversationId: "c1",
  prompt: "p",
  state,
  createdAt: "2026-10-02T00:00:00Z",
  updatedAt: "2026-10-02T00:00:00Z",
});
const render = (state) => {
  const host = document.createElement("div");
  const root = createRoot(host);
  flushSync(() =>
    root.render(
      createElement(Transcript, {
        messages: [{ role: "assistant", content: "已提交后台计划", runId: "run-1" }],
        stream: "",
        seconds: 0,
        toolLabels: {},
        tasks: [task(state)],
        running: false,
      }),
    ),
  );
  const html = host.innerHTML;
  return html;
};
window.__RESULTS = {
  needsReview: render("needsReview"),
  cancelled: render("cancelled"),
};
`,
);
let server;
let httpServer;
let browser;
const closeQuietly = (resource, method) => {
  try {
    const result = resource?.[method]?.();
    if (result && typeof result.then === "function") return result;
  } catch {
    // 清理失败不掩盖检查结果
  }
};
try {
  server = await createServer({
    root: webRoot,
    logLevel: "error",
    server: { middlewareMode: true },
  });
  httpServer = http.createServer(server.middlewares);
  await new Promise((resolve) => httpServer.listen(0, "127.0.0.1", resolve));
  const port = httpServer.address().port;
  browser = await chromium.launch();
  {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/transcript-note-check-blank`, {
    waitUntil: "domcontentloaded",
  });
  // 经 vite 的 /@fs/ 管道加载 shim，裸导入才会被依赖预打包解析。
  const shimUrl = `/@fs/${shimPath.split("\\").join("/")}`;
  await page.evaluate((url) => import(url), shimUrl);
  await page.waitForFunction(() => Boolean(window.__RESULTS), null, {
    timeout: 20_000,
  });
  const results = await page.evaluate(() => window.__RESULTS);
  const failures = [];
  const expect = (ok, what) => {
    if (!ok) failures.push(what);
  };
  const needsReview = results.needsReview ?? "";
  const cancelled = results.cancelled ?? "";
  expect(needsReview.includes("已提交后台计划"), "needsReview: 助手正文仍在");
  expect(
    !needsReview.includes("turn-outcome-note"),
    "needsReview: 不渲染 turn-outcome-note",
  );
  expect(
    !needsReview.includes("结果待核对"),
    "needsReview: 不出现「结果待核对」",
  );
  expect(
    cancelled.includes("turn-outcome-note"),
    "cancelled: 保留 turn-outcome-note",
  );
  expect(cancelled.includes("已停止"), "cancelled: 显示「已停止」");
  if (failures.length > 0) {
    for (const item of failures) console.error(`FAIL ${item}`);
    process.exitCode = 1;
  } else {
    console.log("transcript-note-check: ok");
  }
  }
} finally {
  await closeQuietly(browser, "close");
  await closeQuietly(server, "close");
  closeQuietly(httpServer, "close");
  // shim 删除统一走专用删除脚本，不直接 rmSync。
  let exitCode = 0;
  let stdout = "";
  try {
    stdout = execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "E:\\tools\\Remove-Directory.ps1",
        "-Path",
        shimPath,
        "-Json",
      ],
      { encoding: "utf8" },
    );
  } catch (error) {
    exitCode = error.status ?? 1;
    stdout = String(error.stdout ?? error.message ?? "");
  }
  if (exitCode === 0 && !existsSync(shimPath)) {
    console.log(`shim removed via Remove-Directory.ps1: ${shimPath}`);
  } else {
    console.error(
      `shim removal failed (exit ${exitCode}): ${stdout.trim() || "(no output)"}`,
    );
    if (process.exitCode === undefined || process.exitCode === 0) {
      process.exitCode = exitCode || 1;
    }
  }
}
