// 模型设置页回归：真实 App/SettingsPage/ModelsPage + fake IPC，不启动模型、
// 不访问真实供应商站点（外链点击由 context 路由拦截为空页，只核对目标 URL）。
// 资源登记：本地 vite 服务、Chromium、截图与运行时 TEMP/TMP 均固定在
// target/.tmp/model-layout，属本任务；finally 关闭 browser/server，截图留查。
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
const tempRoot = path.join(root, "target", ".tmp", "model-layout");
fs.mkdirSync(tempRoot, { recursive: true });
process.env.TEMP = tempRoot;
process.env.TMP = tempRoot;
// 版本号唯一来源是 package.json；夹具不一致会弹更新窗挡住交互。
const appVersion = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
).version;

// presets.ts 的期望链接；platform 是控制台首页，key 是密钥管理页。
const EXPECTED_LINKS = {
  deepseek: {
    platform: "https://platform.deepseek.com",
    key: "https://platform.deepseek.com/api_keys",
  },
  zhipu: {
    platform: "https://bigmodel.cn",
    key: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys",
  },
  kimi: {
    platform: "https://platform.kimi.com",
    key: "https://platform.kimi.com/console/api-keys",
  },
  bailian: {
    platform: "https://bailian.console.aliyun.com/",
    key: "https://bailian.console.aliyun.com/cn-beijing/model/settings/api-key",
  },
  minimax: {
    platform: "https://platform.minimax.cn",
    key: "https://platform.minimax.cn/console/access?tab=api-keys",
  },
  siliconflow: {
    platform: "https://cloud.siliconflow.cn",
    key: "https://cloud.siliconflow.cn/account/ak",
  },
  openrouter: {
    platform: "https://openrouter.ai",
    key: "https://openrouter.ai/keys",
  },
  openai: {
    platform: "https://platform.openai.com",
    key: "https://platform.openai.com/api-keys",
  },
  anthropic: {
    platform: "https://platform.claude.com",
    key: "https://platform.claude.com/settings/keys",
  },
  gemini: {
    platform: "https://aistudio.google.com",
    key: "https://aistudio.google.com/apikey",
  },
  ollama: { platform: "https://ollama.com" },
};
const PRESET_NAMES = {
  deepseek: "DeepSeek",
  zhipu: "智谱 GLM",
  kimi: "Kimi",
  bailian: "通义百炼",
  minimax: "MiniMax",
  siliconflow: "硅基流动",
  openrouter: "OpenRouter",
  openai: "OpenAI",
  anthropic: "Claude 官方",
  gemini: "Gemini",
  ollama: "Ollama 本机",
  custom: "自定义",
};

let server, browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    // 纯内存夹具没有 HMR WebSocket：Vite 8 会把页面 console 经 WS 转发给
    // dev server，连不上时每次转发都报错。显式关闭让 console 断言只看产品输出。
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  const port = server.httpServer.address().port;
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1400, height: 900 },
  });
  // 外链（含 target=_blank 弹出页）一律拦截为空页：只验证目标 URL，不触网。
  await context.route(
    (url) =>
      (url.protocol === "https:" || url.protocol === "http:") &&
      url.hostname !== "127.0.0.1",
    (route) =>
      route.fulfill({ contentType: "text/html", body: "<title>blocked</title>" }),
  );
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));

  const savedModels = [];
  const bootstrap = () => ({
    configPath: "fixture",
    models: [
      {
        id: "fixture-model",
        name: "Fixture Model",
        model: "fixture",
        baseUrl: "https://fixture.invalid/v1",
        protocol: "openai-chat",
        active: true,
      },
    ],
    skills: [],
    tools: [],
    runtimeToolLabels: {},
    plugins: [],
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
    bridge: { enabled: false, connected: false, baseUrl: "fixture" },
  });
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    let result = {};
    if (q.method === "bootstrap") result = bootstrap();
    if (q.method === "model.save") {
      savedModels.push(q.params.model);
      result = { saved: true };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });
  await page.addInitScript((version) => {
    localStorage.setItem("sleepy-doll-version", version);
    localStorage.setItem("sleepy-doll-locale", "zh");
    localStorage.setItem("sleepy-doll-theme", "dark");
    localStorage.setItem("sleepy-doll-sidebar-collapsed", "true");
  }, appVersion);
  await page.goto(`http://127.0.0.1:${port}`);
  await page.locator(".app-nav").waitFor();
  await page.evaluate(() =>
    window.dispatchEvent(new CustomEvent("sleepy-doll:open-models")),
  );
  await page.locator(".model-settings").waitFor();

  const startNewForm = async () => {
    await page.getByRole("button", { name: "添加模型", exact: true }).click();
    await page.locator(".model-form").waitFor();
  };
  const pickPreset = async (id) => {
    await page.locator(".model-form [data-ui='select-trigger']").first().click();
    await page
      .getByRole("option", { name: PRESET_NAMES[id], exact: true })
      .click();
    await page.waitForTimeout(150);
  };

  // ---- 1. 所有预设的外链目标与真实点击 ----
  await startNewForm();
  const openedUrls = [];
  for (const id of Object.keys(EXPECTED_LINKS)) {
    await pickPreset(id);
    const links = await page.locator(".model-provider-link").evaluateAll(
      (els) =>
        els.map((a) => ({
          text: a.textContent.trim(),
          href: a.getAttribute("href"),
          target: a.getAttribute("target"),
        })),
    );
    const expected = EXPECTED_LINKS[id];
    assert.deepEqual(
      links.map((link) => link.href),
      [expected.platform, expected.key].filter(Boolean),
      `${id} 预设的外链目标不符合`,
    );
    for (const link of links) {
      assert.equal(link.target, "_blank", `${id} 外链必须开新窗口`);
      const popup = await Promise.all([
        page.waitForEvent("popup"),
        page
          .locator(".model-provider-link", { hasText: link.text })
          .first()
          .click(),
      ]).then(([result]) => result);
      await popup.waitForLoadState("domcontentloaded").catch(() => {});
      const landed = new URL(popup.url()).href;
      const wanted = new URL(link.href).href;
      assert.equal(landed, wanted, `${id} 的「${link.text}」点击后目标不符`);
      openedUrls.push(landed);
      await popup.close();
    }
  }
  await pickPreset("custom");
  assert.equal(
    await page.locator(".model-provider-link").count(),
    0,
    "自定义预设不应有外链",
  );
  // 链接区域单独留一张截图（表单顶部，不依赖滚动后底部）。
  await pickPreset("zhipu");
  await page.locator(".model-provider-links").screenshot({
    path: path.join(tempRoot, "provider-links.png"),
  });

  // ---- 2. 普通/高级表单 × 三种视口：滚动区 + 固定保存行结构 ----
  // 布局契约：.model-form-scroll 独立滚动；footer 是它兄弟、static、
  // 右对齐，滚动区 bottom 不越过 footer.top，末字段可滚到完全可见。
  const readLayout = () =>
    page.evaluate(() => {
      const settings = document.querySelector(".settings-content");
      const form = document.querySelector(".model-form");
      const scroll = form.querySelector(".model-form-scroll");
      const footer = form.querySelector(".detail-actions");
      const collapsed = form.querySelector(".model-advanced-body.is-collapsed");
      const last = [...scroll.querySelectorAll("input, button, [role='switch']")]
        .filter(
          (el) =>
            !(collapsed && collapsed.contains(el)) &&
            el.getBoundingClientRect().height > 0,
        )
        .pop();
      const buttons = [...footer.querySelectorAll("button")];
      const save = buttons.find((b) =>
        b.className.includes("primary-action"),
      );
      const rightmost = buttons.reduce((a, b) =>
        b.getBoundingClientRect().right > a.getBoundingClientRect().right
          ? b
          : a,
      );
      const rect = (el) => {
        const r = el.getBoundingClientRect();
        return { top: r.top, bottom: r.bottom, left: r.left, right: r.right };
      };
      return {
        overflowX: {
          document: document.documentElement.scrollWidth - window.innerWidth,
          settings: settings.scrollWidth - settings.clientWidth,
          form: form.scrollWidth - form.clientWidth,
        },
        settingsOverflowY: getComputedStyle(settings).overflowY,
        scroll: rect(scroll),
        footer: rect(footer),
        footerPosition: getComputedStyle(footer).position,
        footerInsideScroll: scroll.contains(footer),
        scrollable: scroll.scrollHeight > scroll.clientHeight + 1,
        last: last ? rect(last) : null,
        saveIsRightmost: save === rightmost,
        saveIsLastDom: save === buttons[buttons.length - 1],
        saveRect: save ? rect(save) : null,
        footerRight: rect(footer).right,
        formRight: rect(form).right,
      };
    });
  const checkLayout = async (label) => {
    const layout = await readLayout();
    assert.equal(
      layout.settingsOverflowY,
      "hidden",
      `${label} 模型页 settings-content 应改为 overflow:hidden`,
    );
    assert.ok(
      Object.values(layout.overflowX).every((value) => value <= 1),
      `${label} 出现横向溢出：${JSON.stringify(layout.overflowX)}`,
    );
    assert.equal(
      layout.footerPosition,
      "static",
      `${label} 保存行必须是正常文档流（static）`,
    );
    assert.ok(
      !layout.footerInsideScroll,
      `${label} 保存行不能位于滚动区内部`,
    );
    assert.ok(
      Math.abs(layout.footerRight - layout.formRight) <= 1,
      `${label} 保存行未与表单列右对齐`,
    );
    assert.ok(
      layout.saveIsRightmost,
      `${label} 保存主按钮必须在保存行最右`,
    );
    assert.ok(
      layout.saveIsLastDom,
      `${label} 保存主按钮必须是保存行最后一个 DOM 按钮`,
    );
    assert.ok(
      layout.saveRect &&
        Math.abs(layout.saveRect.right - layout.footer.right) <= 1,
      `${label} 保存主按钮未与保存行右缘对齐：save.right=${layout.saveRect?.right} footer.right=${layout.footer.right}`,
    );
    assert.ok(
      layout.scroll.bottom <= layout.footer.top + 1,
      `${label} 滚动区越过保存行：scroll.bottom=${layout.scroll.bottom} footer.top=${layout.footer.top}`,
    );
    if (!layout.scrollable) return layout;
    // 顶/中/底三种滚动位置：footer 的位置不允许变化。
    const footerTops = [];
    for (const ratio of [0, 0.5, 1]) {
      await page.evaluate((r) => {
        const scroll = document.querySelector(".model-form-scroll");
        scroll.scrollTop = (scroll.scrollHeight - scroll.clientHeight) * r;
      }, ratio);
      footerTops.push((await readLayout()).footer.top);
    }
    assert.ok(
      footerTops.every((top) => Math.abs(top - footerTops[0]) <= 1),
      `${label} 滚动时保存行位置漂移：${footerTops.join(",")}`,
    );
    // 滚到底后，最后的字段必须完全可见且可点击。
    const atBottom = await readLayout();
    assert.ok(
      atBottom.last &&
        atBottom.last.bottom <= atBottom.scroll.bottom + 1 &&
        atBottom.last.top >= atBottom.scroll.top - 1,
      `${label} 末尾字段滚不到完全可见`,
    );
    const clickable = await page.evaluate(() => {
      const scroll = document.querySelector(".model-form-scroll");
      const collapsed = scroll.querySelector(
        ".model-advanced-body.is-collapsed",
      );
      const last = [...scroll.querySelectorAll("input, button, [role='switch']")]
        .filter(
          (el) =>
            !(collapsed && collapsed.contains(el)) &&
            el.getBoundingClientRect().height > 0,
        )
        .pop();
      const r = last.getBoundingClientRect();
      const hit = document.elementFromPoint(
        (r.left + r.right) / 2,
        Math.min(r.bottom - 4, (r.top + r.bottom) / 2),
      );
      return last === hit || last.contains(hit) || hit?.contains(last);
    });
    assert.ok(clickable, `${label} 末尾字段在滚动区底部不可点击`);
    return atBottom;
  };
  const views = [
    { name: "desktop", width: 1400, height: 900 },
    { name: "narrow", width: 660, height: 760 },
    { name: "short", width: 1200, height: 520 },
  ];
  for (const view of views) {
    await page.setViewportSize({
      width: view.width,
      height: view.height,
    });
    await page.waitForTimeout(250);
    await startNewForm();
    await pickPreset("deepseek");
    await checkLayout(`${view.name}/普通表单`);
    await page
      .locator(".model-advanced-toggle", { hasText: "高级选项" })
      .click();
    await page.waitForTimeout(400);
    await checkLayout(`${view.name}/高级表单`);
    await page.screenshot({
      path: path.join(tempRoot, `advanced-${view.name}.png`),
      fullPage: false,
    });
  }
  // 顶/底两张静态截图给 Root 看（高级展开态，滚到底）。
  await page.evaluate(() => {
    const scroll = document.querySelector(".model-form-scroll");
    scroll.scrollTop = 0;
  });
  await page.screenshot({ path: path.join(tempRoot, "top.png") });
  await page.evaluate(() => {
    const scroll = document.querySelector(".model-form-scroll");
    scroll.scrollTop = scroll.scrollHeight;
  });
  await page.screenshot({ path: path.join(tempRoot, "bottom.png") });
  await page.setViewportSize({ width: 1400, height: 900 });

  // ---- 2b. 大量模型：左栏独立滚动，不挤压保存行 ----
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    let result = {};
    if (q.method === "bootstrap") {
      const base = bootstrap();
      base.models = Array.from({ length: 40 }, (_, i) => ({
        id: `fixture-${i}`,
        name: `Fixture ${i}`,
        model: `fixture-${i}`,
        baseUrl: "https://fixture.invalid/v1",
        protocol: "openai-chat",
        active: i === 0,
      }));
      result = base;
    }
    if (q.method === "model.save") {
      savedModels.push(q.params.model);
      result = { saved: true };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });
  await page.reload();
  await page.locator(".app-nav").waitFor();
  await page.evaluate(() =>
    window.dispatchEvent(new CustomEvent("sleepy-doll:open-models")),
  );
  await page.locator(".model-settings").waitFor();
  await page.waitForTimeout(400);
  {
    const list = await page.evaluate(() => {
      const list = document.querySelector(".model-list");
      return {
        scrollable: list.scrollHeight > list.clientHeight + 1,
        bottom: list.getBoundingClientRect().bottom,
        height: list.getBoundingClientRect().height,
      };
    });
    assert.ok(list.scrollable, "40 个模型时左列表应独立滚动");
    const layout = await readLayout();
    assert.ok(
      layout.scroll.bottom <= layout.footer.top + 1,
      "大量模型时滚动区越过保存行",
    );
    assert.ok(
      Math.abs(layout.footerRight - layout.formRight) <= 1,
      "大量模型时保存行未与表单列右对齐",
    );
    await page.screenshot({ path: path.join(tempRoot, "many-models.png") });
  }

  // ---- 2c. 靠近底部的下拉：body portal 弹层不越过保存行 ----
  await startNewForm();
  await pickPreset("deepseek");
  await page.evaluate(() => {
    const scroll = document.querySelector(".model-form-scroll");
    scroll.scrollTop = scroll.scrollHeight;
  });
  // 靠近底部的协议下拉（高级区内还有鉴权下拉，这里测正文区的即可）。
  await page
    .locator(".model-form-scroll [data-ui='select-trigger']")
    .nth(1)
    .click();
  await page.waitForTimeout(200);
  {
    const menu = await page.evaluate(() => {
      const menu = document.querySelector("[data-ui='select-menu']");
      const footer = document
        .querySelector(".model-form")
        .querySelector(".detail-actions");
      const m = menu.getBoundingClientRect();
      const f = footer.getBoundingClientRect();
      return { top: m.top, bottom: m.bottom, footerTop: f.top };
    });
    assert.ok(
      menu.bottom <= menu.footerTop + 1,
      `底部下拉越过保存行：menu.bottom=${menu.bottom} footer.top=${menu.footerTop}`,
    );
    await page.screenshot({ path: path.join(tempRoot, "select-near-bottom.png") });
    // 菜单开着时卷动正文：boundary Select 应直接收起，而不是跟随重算。
    // 滚动方向必须真实改变 scrollTop（在底部就往上，在顶就往下）。
    const scrolled = await page.evaluate(() => {
      const scroll = document.querySelector(".model-form-scroll");
      const before = scroll.scrollTop;
      scroll.scrollTop = before > 0 ? before - 24 : before + 24;
      return { before, after: scroll.scrollTop };
    });
    assert.notEqual(
      scrolled.after,
      scrolled.before,
      `正文滚动量应真实变化：${scrolled.before} -> ${scrolled.after}`,
    );
    await page.waitForTimeout(200);
    assert.equal(
      await page.locator("[data-ui='select-menu']").count(),
      0,
      "正文滚动时 boundary 下拉应收起",
    );
  }
  // 恢复单一 fixture 模型的 IPC 路由，供保存断言继续使用。
  await page.route("**/ipc", async (route) => {
    const q = route.request().postDataJSON();
    let result = {};
    if (q.method === "bootstrap") result = bootstrap();
    if (q.method === "model.save") {
      savedModels.push(q.params.model);
      result = { saved: true };
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: q.id, ok: true, result }),
    });
  });

  // ---- 3. 保存调用：普通与高级各一次 ----
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const waitForSave = async (count) => {
    for (let i = 0; i < 50 && savedModels.length < count; i++) await sleep(100);
  };
  await startNewForm();
  await pickPreset("deepseek");
  // 假密钥只为过表单必填校验，不会发往任何真实端点。
  await page.locator(".key-field input").fill("fixture-key");
  await page.locator(".model-pick input").fill("deepseek-fixture");
  await page.locator(".detail-actions .primary-action").click();
  await waitForSave(1);
  assert.equal(savedModels.length, 1, "普通表单应发出一次 model.save");
  assert.deepEqual(
    {
      name: savedModels[0].name,
      protocol: savedModels[0].protocol,
      baseUrl: savedModels[0].baseUrl,
      model: savedModels[0].model,
      apiKey: savedModels[0].apiKey,
    },
    {
      name: "DeepSeek",
      protocol: "anthropic-messages",
      baseUrl: "https://api.deepseek.com/anthropic/v1",
      model: "deepseek-fixture",
      apiKey: "fixture-key",
    },
    "普通表单保存的载荷不符合预设",
  );
  await startNewForm();
  await pickPreset("kimi");
  await page.locator(".key-field input").fill("fixture-key");
  await page
    .locator(".model-advanced-toggle", { hasText: "高级选项" })
    .click();
  await page.waitForTimeout(400);
  await page
    .locator(".model-advanced-inner label", { hasText: "响应超时" })
    .locator("input")
    .fill("90");
  await page.locator(".model-pick input").fill("kimi-fixture");
  await page.locator(".detail-actions .primary-action").click();
  await waitForSave(2);
  assert.equal(savedModels.length, 2, "高级表单应发出第二次 model.save");
  assert.equal(savedModels[1].timeoutMs, 90000, "高级表单的超时应换算为毫秒");
  assert.equal(savedModels[1].promptCache, true, "预设的提示缓存默认开启");

  assert.equal(
    openedUrls.length,
    21,
    "11 个预设 × 2 入口 − Ollama 无密钥入口 = 21 次外链点击",
  );
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify({
      presetLinks: Object.keys(EXPECTED_LINKS).length,
      linkClicks: openedUrls.length,
      layoutViews: views.map((view) => view.name),
      saves: savedModels.map((model) => model.model),
      pageErrors: errors,
      screenshots: path.join(
        tempRoot,
        "advanced-{desktop,narrow,short}.png, top.png, bottom.png, many-models.png, select-near-bottom.png",
      ),
    }),
  );
} finally {
  await browser?.close();
  await server?.close();
}
