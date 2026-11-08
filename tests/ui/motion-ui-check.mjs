import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const scratch = path.join(root, "target/.tmp/web-motion");
const temporary = path.join(root, "target/tmp/web-motion");
fs.mkdirSync(scratch, { recursive: true });
fs.mkdirSync(temporary, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = temporary;
// 临时探针只挂载正式 Hook，验证长文本与空闲提交；不复制产品实现。
fs.writeFileSync(
  path.join(scratch, "text-presentation.tsx"),
  `
import {useLayoutEffect} from "react";
import {createRoot} from "react-dom/client";
import {usePresentedText} from "../../../web/src/components/chat/usePresentedText";
const node=document.createElement("div");node.id="text-presentation-probe";
node.style.cssText="position:fixed;width:1px;height:1px;overflow:hidden;opacity:0;pointer-events:none";
document.body.append(node);const root=createRoot(node);
(window as any).__textProbeCommits=0;
function Probe({text,owner}:{text:string;owner:string}){
  const value=usePresentedText([],text,owner);
  useLayoutEffect(()=>{(window as any).__textProbeCommits++;});
  return <pre>{value.stream}</pre>;
}
(window as any).__textProbe={update:(text:string,owner="one")=>root.render(<Probe text={text} owner={owner}/>),unmount:()=>{root.unmount();node.remove();}};
`,
);
const { createServer, preview } = await import("vite");
const { chromium } = await import("playwright");
const version = JSON.parse(
  fs.readFileSync(path.join(root, "package.json")),
).version;
const baseline = process.argv.includes("--baseline");
const production = process.argv.includes("--production");
const conversations = Array.from({ length: 18 }, (_, i) => ({
  id: `motion-${i}`,
  title: `动画验证对话 ${i + 1}`,
  createdAt: "2026-11-01T00:00:00Z",
  updatedAt: "2026-11-01T00:00:00Z",
}));
const task = {
  id: "motion-task",
  name: "动画验证任务",
  description: "检查交互与动画",
  state: "readyUnverified",
  stateLabel: "可运行",
  actionLabel: "运行",
  runnable: true,
  pinned: false,
  sourceDeleted: false,
  publishedRevision: 1,
  modelUsage: "none",
  zeroToken: true,
  nodeCount: 1,
  updatedAt: "2026-11-01T00:00:00Z",
  shortcut: {
    applicationName: "演示",
    targetName: "固定目标",
    prepare: [],
    action: { tool: "demo.run", arguments: {} },
  },
};
const bootstrap = {
  configPath: "E:\\Sleepy Doll\\user\\config.json",
  models: [
    {
      id: "demo",
      name: "演示模型",
      model: "demo",
      active: true,
      baseUrl: "https://demo.invalid/v1",
      protocol: "openai-chat",
    },
  ],
  skills: [],
  tools: [],
  plugins: [],
  conversations,
  tasks: [],
  strategies: [],
  workflows: [task],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  conversationGroups: {
    groups: [],
    membership: {},
    order: conversations.map((item) => item.id),
  },
  permission: { mode: "fullAccess", label: "允许执行", levels: [] },
  runtimeToolLabels: { "demo.lookup": "检查内容" },
  bridge: { enabled: false, connected: false, baseUrl: "demo" },
};
const history = Array.from({ length: 32 }, (_, i) => ({
  role: i % 2 ? "assistant" : "user",
  runId: `history-${Math.floor(i / 2)}`,
  content: `${i + 1}. 历史消息。\n\n${"用于验证阅读位置不会被内容变化拉走。".repeat(12)}`,
  createdAt: "2026-11-01T00:00:00Z",
}));
history.push(
  {
    role: "user",
    runId: "process-demo",
    content: "检查已有任务",
    createdAt: "2026-11-01T00:00:00Z",
  },
  {
    role: "assistant",
    runId: "process-demo",
    content: "核对已有内容。",
    toolCalls: [{ id: "process-call", name: "demo.lookup", arguments: {} }],
    createdAt: "2026-11-01T00:00:00Z",
  },
  {
    role: "tool",
    runId: "process-demo",
    toolCallId: "process-call",
    content: '{"ok":true,"value":{"found":true}}',
    createdAt: "2026-11-01T00:00:00Z",
  },
  {
    role: "assistant",
    runId: "process-demo",
    content: "已有内容核对完成。",
    createdAt: "2026-11-01T00:00:00Z",
  },
);
let server, browser;
const report = {};
try {
  server = production
    ? await preview({
        configFile: path.join(root, "vite.config.ts"),
        configLoader: "runner",
        logLevel: "error",
        preview: { host: "127.0.0.1", port: 0 },
      })
    : await createServer({
        configFile: path.join(root, "vite.config.ts"),
        configLoader: "runner",
        cacheDir: path.join(temporary, "vite"),
        logLevel: "error",
        server: { host: "127.0.0.1", port: 0, hmr: false },
      });
  if (!production) await server.listen();
  console.log(
    JSON.stringify({
      resource: "web-motion",
      pid: process.pid,
      port: server.httpServer.address().port,
    }),
  );
  browser = await chromium.launch({ channel: "msedge", headless: true });
  const context = await browser.newContext({
    viewport: { width: 1400, height: 900 },
    locale: "zh-CN",
  });
  await context.route(
    (url) => !["127.0.0.1", "localhost"].includes(url.hostname),
    (route) => route.fulfill({ body: "" }),
  );
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(
    ({ version }) => {
      localStorage.setItem("sleepy-doll-version", version);
      localStorage.setItem("sleepy-doll-theme", "light");
      localStorage.setItem("sleepy-doll-locale", "zh");
      window.__motionCommits = 0;
      window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
        supportsFiber: true,
        renderers: new Map(),
        inject() {
          return 1;
        },
        onCommitFiberRoot() {
          window.__motionCommits += 1;
        },
        onCommitFiberUnmount() {},
      };
    },
    { version },
  );
  await page.route("**/ipc", async (route) => {
    const headers = {
      "access-control-allow-origin": "*",
      "access-control-allow-headers": "content-type",
      "access-control-allow-methods": "POST,OPTIONS",
    };
    if (route.request().method() === "OPTIONS") {
      await route.fulfill({ status: 204, headers });
      return;
    }
    const q = route.request().postDataJSON();
    let result = {};
    if (q.method === "bootstrap") result = bootstrap;
    if (q.method === "events.read") result = { events: [] };
    if (q.method === "task.list") result = [];
    if (q.method === "conversation.get")
      result = { id: q.params.id, messages: history, runs: [] };
    await route.fulfill({
      contentType: "application/json",
      headers,
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });
  await page.route("**/motion-late-image.svg", (route) =>
    route.fulfill({
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="180"><rect width="100" height="180" fill="#ddd"/></svg>',
    }),
  );
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  await page.locator(".app-nav").waitFor();
  await page.waitForTimeout(400);
  report.reactHookActive = await page.evaluate(
    () => window.__motionCommits > 0,
  );
  assert.equal(
    report.reactHookActive,
    true,
    "提交次数指标需要真实的 React 提交回调",
  );
  const row = page.locator(
    '[data-sd-slot="conversation"][data-sd-id="motion-0"]',
  );
  const bounds = await row.boundingBox();
  await page.mouse.move(bounds.x + 65, bounds.y + bounds.height / 2);
  await page.mouse.down();
  await page.mouse.move(bounds.x + 80, bounds.y + bounds.height / 2);
  await page.mouse.move(500, 350);
  await page.waitForTimeout(40);
  const before = await page.evaluate(() => window.__motionCommits);
  for (let i = 0; i < 24; i++) {
    await page.mouse.move(500 + i, 350);
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(resolve)),
    );
  }
  report.pointerMoveCommits =
    (await page.evaluate(() => window.__motionCommits)) - before;
  await page.mouse.up();
  await page.waitForTimeout(650);
  if (!baseline)
    assert.ok(report.pointerMoveCommits <= 3, JSON.stringify(report));

  await page
    .locator(".app-nav")
    .getByRole("button", { name: "快捷任务", exact: true })
    .click();
  await page.getByRole("button", { name: "运行记录", exact: true }).click();
  await page.waitForTimeout(45);
  await page.locator(".sd-tabs-pill").evaluate((node) => {
    const animation = node.getAnimations()[0];
    animation?.pause();
    if (animation) animation.currentTime = 70;
  });
  const from = await page.locator(".sd-tabs-pill").boundingBox();
  await page.getByRole("button", { name: "已保存任务", exact: true }).click();
  await page.locator(".sd-tabs-pill").evaluate((node) => {
    const animation = node.getAnimations()[0];
    animation?.pause();
    if (animation) animation.currentTime = 0;
  });
  const after = await page.locator(".sd-tabs-pill").boundingBox();
  report.tabReverseJump = Math.abs(after.x - from.x);
  report.tabAnimatedProperties = await page
    .locator(".sd-tabs-pill")
    .evaluate((node) =>
      node
        .getAnimations()
        .flatMap((animation) =>
          Object.keys(animation.effect.getKeyframes()[0] ?? {}),
        ),
    );
  if (!baseline)
    assert.ok(
      !report.tabAnimatedProperties.some((key) =>
        ["left", "top", "width", "height"].includes(key),
      ),
      JSON.stringify(report),
    );
  if (!baseline) assert.ok(report.tabReverseJump < 1, JSON.stringify(report));
  await page
    .locator(".sd-tabs-pill")
    .evaluate((node) =>
      node.getAnimations().forEach((animation) => animation.finish()),
    );
  await page.waitForTimeout(350);
  await page.getByRole("button", { name: "动画验证任务 的更多操作" }).click();
  await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
  await page.getByRole("dialog").waitFor();
  await page.waitForTimeout(70);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "取消", exact: true })
    .click();
  if (!baseline) {
    assert.equal(
      await page.locator(".sd-dialog-layer").count(),
      1,
      "退场前不能卸载",
    );
    await page.getByRole("button", { name: "动画验证任务 的更多操作" }).click();
    await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
    await page.waitForTimeout(300);
    assert.equal(
      await page.locator(".sd-dialog-layer").count(),
      1,
      "旧动画不能清理重新打开的弹窗",
    );
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "取消", exact: true })
      .click();
  }
  await page.waitForTimeout(350);
  assert.equal(await page.locator(".sd-dialog-layer").count(), 0);
  await page.screenshot({
    path: path.join(scratch, baseline ? "baseline.png" : "tasks.png"),
  });
  if (!baseline) {
    if (!production)
      report.springRetarget = await page.evaluate(
        async (moduleUrl) => {
          const { springPosition } = await import(moduleUrl);
          const node = document.createElement("div");
          node.style.cssText =
            "position:fixed;left:0;top:0;width:10px;height:10px";
          document.body.append(node);
          let stale = 0,
            latest = 0;
          try {
            const first = springPosition(
              node,
              { x: 0, y: 0 },
              { x: 300, y: 0 },
              { x: 120, y: 0 },
            );
            void first.then(
              () => stale++,
              () => {},
            );
            const original = node.getAnimations()[0];
            original.pause();
            original.currentTime = 80;
            const before = node.getBoundingClientRect().left;
            const second = springPosition(
              node,
              { x: 0, y: 0 },
              { x: 80, y: 0 },
              { x: 0, y: 0 },
            );
            void second.then(
              () => stale++,
              () => {},
            );
            const middle = node.getAnimations()[0];
            middle.pause();
            middle.currentTime = 0;
            const jump = Math.abs(node.getBoundingClientRect().left - before);
            middle.finish();
            const third = springPosition(
              node,
              { x: 0, y: 0 },
              { x: 140, y: 0 },
              { x: 0, y: 0 },
            );
            void third.then(
              () => latest++,
              () => {},
            );
            node.getAnimations()[0].finish();
            await Promise.allSettled([first, second, third]);
            await Promise.resolve();
            return { jump, stale, latest };
          } finally {
            node.remove();
          }
        },
        `/@fs/${root.replaceAll("\\", "/")}packages/ui/src/motion/visual.ts`,
      );
    if (!production)
      assert.ok(
        report.springRetarget.jump < 1,
        JSON.stringify(report.springRetarget),
      );
    if (!production) assert.equal(report.springRetarget.stale, 0);
    if (!production) assert.equal(report.springRetarget.latest, 1);
    const savedOrder = await page.evaluate(() =>
      localStorage.getItem("sleepy-doll-conversation-groups"),
    );
    const source = await page
      .locator('[data-sd-slot="conversation"]')
      .first()
      .boundingBox();
    const destination = await page
      .locator('[data-sd-slot="conversation"]')
      .nth(3)
      .boundingBox();
    await page.mouse.move(source.x + 70, source.y + source.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      destination.x + 70,
      destination.y + destination.height / 2,
      { steps: 8 },
    );
    await page.waitForTimeout(60);
    await page.keyboard.press("Escape");
    await page.mouse.up();
    await page.waitForTimeout(650);
    assert.equal(
      await page.evaluate(() =>
        localStorage.getItem("sleepy-doll-conversation-groups"),
      ),
      savedOrder,
      "取消手势不得保存草稿",
    );
    assert.equal(await page.locator(".app-drag-ghost").count(), 0);

    const resize = await page.getByRole("separator").boundingBox();
    await page.mouse.move(resize.x + 5, 120);
    await page.mouse.down();
    await page.waitForTimeout(40);
    const resizeBefore = await page.evaluate(() => window.__motionCommits);
    for (let i = 1; i <= 16; i++) {
      await page.mouse.move(resize.x + 5 + i * 2, 120);
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(resolve)),
      );
    }
    report.resizeMoveCommits =
      (await page.evaluate(() => window.__motionCommits)) - resizeBefore;
    assert.ok(report.resizeMoveCommits <= 3, JSON.stringify(report));
    await page.mouse.up();
    await page.waitForTimeout(250);

    await page
      .locator('[data-sd-slot="conversation"] button[title]')
      .first()
      .click();
    await page.locator(".message-turn").first().waitFor();
    await page.waitForTimeout(450);
    const process = page.locator(".process-disclosure:not([hidden])").last();
    const summary = process.locator(".process-disclosure-summary");
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(100);
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(35);
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(300);
    assert.equal(await summary.getAttribute("aria-expanded"), "true");
    assert.ok(
      (await process
        .locator(".activity-disclosure-inner")
        .first()
        .evaluate((node) => node.children.length)) > 0,
    );
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(280);
    assert.equal(
      await process
        .locator(".activity-disclosure-inner")
        .first()
        .evaluate((node) => node.children.length),
      0,
    );
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(100);
    await summary.evaluate((node) => node.click());
    await page.waitForTimeout(30);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForTimeout(100);
    assert.equal(
      await process
        .locator(".activity-disclosure-inner")
        .first()
        .evaluate((node) => node.children.length),
      0,
      "中途减少动态效果应清理已关闭内容",
    );
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.locator(".chat-scroll").evaluate((node) => {
      node.scrollTop = (node.scrollHeight - node.clientHeight) * 0.45;
    });
    await page.waitForTimeout(100);
    const offset = await page.locator(".chat-scroll").evaluate((root) => {
      const top = root.getBoundingClientRect().top;
      const anchor = [...root.querySelectorAll(".message-content p")].find(
        (node) => node.getBoundingClientRect().bottom > top + 1,
      );
      anchor.dataset.motionAnchor = "true";
      return anchor.getBoundingClientRect().top - top;
    });
    await page.locator(".conversation-flow").evaluate((node) => {
      const gap = document.createElement("div");
      gap.dataset.lateImage = "true";
      gap.style.height = "180px";
      node.prepend(gap);
    });
    await page.waitForTimeout(100);
    const drift = () =>
      page
        .locator(".chat-scroll")
        .evaluate(
          (root, offset) =>
            root
              .querySelector('[data-motion-anchor="true"]')
              .getBoundingClientRect().top -
            root.getBoundingClientRect().top -
            offset,
          offset,
        );
    report.contentAnchorDrift = Math.abs(await drift());
    await page.locator('[data-late-image="true"]').evaluate((node) => {
      node.style.height = "360px";
    });
    await page.waitForTimeout(100);
    report.imageAnchorDrift = Math.abs(await drift());
    const textarea = page.locator("textarea.sd-composer-field");
    await textarea.fill("输入框伸缩保持阅读位置\n".repeat(6));
    await page.waitForTimeout(150);
    report.composerAnchorDrift = Math.abs(await drift());
    assert.ok(
      Math.max(
        report.contentAnchorDrift,
        report.imageAnchorDrift,
        report.composerAnchorDrift,
      ) < 2,
      JSON.stringify(report),
    );
    const inputHeight = await textarea.evaluate(
      (node) => node.getBoundingClientRect().height,
    );
    assert.ok(
      inputHeight > 24 && inputHeight <= 181,
      `输入框高度 ${inputHeight}`,
    );
    await textarea.fill("");
    await page.waitForTimeout(100);
    assert.ok(
      (await textarea.evaluate(
        (node) => node.getBoundingClientRect().height,
      )) <= 25,
    );
    await page.locator(".conversation-flow").evaluate((flow) => {
      const article = document.createElement("article");
      article.className = "message-turn assistant";
      article.dataset.turnKey = "late-image-probe";
      const content = document.createElement("div");
      content.className = "message-content";
      const p = document.createElement("p"),
        image = document.createElement("img");
      image.dataset.realLateImage = "true";
      image.style.cssText = "display:block;width:100px;height:0px";
      const text = document.createTextNode(
        "阅读字符锚点随段内图片保持。".repeat(300),
      );
      p.append(image, text);
      content.append(p);
      article.append(content);
      flow.append(article);
      const range = document.createRange();
      range.setStart(text, 400);
      range.setEnd(text, 401);
      window.__imageTextRange = range;
      const root = document.querySelector(".chat-scroll");
      root.scrollTop +=
        range.getBoundingClientRect().top -
        root.getBoundingClientRect().top -
        40;
    });
    await page.waitForTimeout(100);
    const imageOffset = await page
      .locator(".chat-scroll")
      .evaluate(
        (root) =>
          window.__imageTextRange.getBoundingClientRect().top -
          root.getBoundingClientRect().top,
      );
    await page.locator("[data-real-late-image]").evaluate(
      (image) =>
        new Promise((resolve, reject) => {
          image.onload = resolve;
          image.onerror = reject;
          image.style.height = "auto";
          image.src = "/motion-late-image.svg";
        }),
    );
    await page.waitForTimeout(120);
    report.inlineImageCharacterDrift = Math.abs(
      (await page
        .locator(".chat-scroll")
        .evaluate(
          (root) =>
            window.__imageTextRange.getBoundingClientRect().top -
            root.getBoundingClientRect().top,
        )) - imageOffset,
    );
    assert.ok(report.inlineImageCharacterDrift < 2, JSON.stringify(report));
    const unreadBox = await page
      .getByRole("button", { name: "有新内容 · 回到最新" })
      .boundingBox();
    await page.mouse.move(
      unreadBox.x + unreadBox.width / 2,
      unreadBox.y + unreadBox.height / 2,
    );
    await page.mouse.down();
    await page.waitForTimeout(100);
    assert.ok(
      await page.evaluate(
        ({ x, y }) =>
          Boolean(document.elementFromPoint(x, y)?.closest(".chat-unread")),
        {
          x: unreadBox.x + unreadBox.width / 2,
          y: unreadBox.y + unreadBox.height / 2,
        },
      ),
      "按压不能覆盖按钮的定位变换",
    );
    await page.mouse.up();
    await page.waitForTimeout(650);
    report.latestGap = await page
      .locator(".chat-scroll")
      .evaluate(
        (node) => node.scrollHeight - node.clientHeight - node.scrollTop,
      );
    assert.ok(report.latestGap < 2, JSON.stringify(report));
    await page.screenshot({ path: path.join(scratch, "chat-wide.png") });
    await page.setViewportSize({ width: 1000, height: 900 });
    await page.waitForTimeout(650);
    await page.screenshot({ path: path.join(scratch, "chat-narrow.png") });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth + 1,
      ),
    );
    report.inputNativeSizing = await page.evaluate(() =>
      CSS.supports("field-sizing", "content"),
    );
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.locator(".app-account").click();
    await page.waitForTimeout(30);
    await page.locator(".app-account").click();
    await page.waitForTimeout(30);
    assert.equal(
      await page.locator(".app-account-menu").count(),
      0,
      "减少动态效果也应完成卸载",
    );
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.addInitScript(() => {
      const supports = CSS.supports.bind(CSS);
      CSS.supports = (...args) =>
        args[0] === "field-sizing" ? false : supports(...args);
    });
    await page.reload();
    await page.locator(".app-nav").waitFor();
    const fallback = page.locator("textarea.sd-composer-field");
    await fallback.fill("兼容旧浏览器的输入框\n".repeat(5));
    report.inputFallbackHeight = await fallback.evaluate(
      (node) => node.getBoundingClientRect().height,
    );
    assert.ok(
      report.inputFallbackHeight > 24 && report.inputFallbackHeight <= 181,
    );
    assert.ok(await fallback.evaluate((node) => Boolean(node.style.height)));
    await fallback.fill("");
    assert.ok(
      (await fallback.evaluate(
        (node) => node.getBoundingClientRect().height,
      )) <= 25,
    );
    await page
      .locator(".app-nav")
      .getByRole("button", { name: "快捷任务", exact: true })
      .click();
    await page.getByRole("button", { name: "动画验证任务 的更多操作" }).click();
    await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
    await page.waitForTimeout(250);
    await page.screenshot({ path: path.join(scratch, "dialog-narrow.png") });
    const modal = await page.getByRole("dialog").boundingBox();
    assert.ok(
      modal.x >= 23 &&
        modal.x + modal.width <= 977 &&
        modal.y + modal.height <= 877,
    );
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForTimeout(300);
    await page.screenshot({ path: path.join(scratch, "dialog-wide.png") });
    if (!production) {
      await page.evaluate(
        async (url) => {
          await import(url);
          window.__textProbe.update("");
        },
        `/@fs/${root.replaceAll("\\", "/")}target/.tmp/web-motion/text-presentation.tsx`,
      );
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(resolve)),
      );
      const longText = "长文本不应再逐字等待。".repeat(1000);
      await page.evaluate((text) => window.__textProbe.update(text), longText);
      await page.waitForFunction(
        (length) =>
          document.querySelector("#text-presentation-probe pre")?.textContent
            ?.length === length,
        longText.length,
        { timeout: 1000 },
      );
      await page.waitForTimeout(100);
      const commits = await page.evaluate(() => window.__textProbeCommits);
      await page.waitForTimeout(250);
      report.textIdleCommits =
        (await page.evaluate(() => window.__textProbeCommits)) - commits;
      assert.equal(
        report.textIdleCommits,
        0,
        "没有新文本时不能靠动画时钟反复提交 React",
      );
      await page.evaluate(() => window.__textProbe.update("下一轮文本", "two"));
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(resolve)),
      );
      assert.equal(
        await page.locator("#text-presentation-probe pre").textContent(),
        "下一轮文本",
      );
      await page.evaluate(() => window.__textProbe.update("", "two"));
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(resolve)),
      );
      assert.equal(
        await page.locator("#text-presentation-probe pre").textContent(),
        "",
      );
      await page.evaluate(() => window.__textProbe.unmount());
    }
  }
  report.errors = errors;
  assert.deepEqual(errors, []);
  fs.writeFileSync(
    path.join(
      scratch,
      baseline
        ? "baseline.json"
        : production
          ? "result-production.json"
          : "result.json",
    ),
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report));
} finally {
  await browser?.close();
  if (production && server)
    await new Promise((resolve, reject) =>
      server.httpServer.close((error) => (error ? reject(error) : resolve())),
    );
  else await server?.close();
}
