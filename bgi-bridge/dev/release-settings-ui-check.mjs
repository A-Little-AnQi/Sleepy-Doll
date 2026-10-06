// 软件更新/关于页回归：真实 App/SettingsPage/AboutPage/ReleaseSettings + fake IPC，
// 不访问线上下载/安装，不使用真实凭证。资源登记：本地 vite 服务、Chromium、
// 截图与运行时 TEMP/TMP 固定 target/.tmp/telemetry-removal，属本任务；
// finally 关闭 browser/server，截图留 Root 查设计。
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
const tempRoot = path.join(root, "target", ".tmp", "telemetry-removal");
fs.mkdirSync(tempRoot, { recursive: true });
process.env.TEMP = tempRoot;
process.env.TMP = tempRoot;
const appVersion = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
).version;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let server, browser;
try {
  server = await createServer({
    root: path.join(root, "web"),
    configFile: path.join(root, "vite.config.ts"),
    logLevel: "silent",
    // 纯内存夹具没有 HMR WebSocket：Vite 8 的 console 转发会连不上而报错，显式关闭。
    server: { host: "127.0.0.1", port: 0, hmr: false, forwardConsole: false },
  });
  await server.listen();
  const port = server.httpServer.address().port;
  browser = await chromium.launch({ headless: true });

  const runScenario = async (theme, locale = "zh") => {
    const context = await browser.newContext({
      viewport: { width: 1400, height: 900 },
    });
    const page = await context.newPage();
    page.setDefaultTimeout(8000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));

    // fake 后端状态：先无更新，检查后出现 9.9.9，可下载、可安装。
    let release = null;
    let downloaded = false;
    let failNextCheck = false;
    let noUpdateOnce = false;
    let slowNextCheck = false;
    let installed = false;
    const configureCalls = [];
    const downloadCalls = [];
    const installCalls = [];
    const state = () => ({
      currentVersion: appVersion,
      channel: "stable",
      analyticsEnabled: true,
      downloaded,
      release,
    });
    const nextRelease = () => ({
      version: "9.9.9",
      channel: "stable",
      url: "https://fixture.invalid/app-setup.exe",
      size: 1,
      sha256: "0".repeat(64),
      notes: "修复说明第一行\n修复说明第二行",
      publishedAt: "2026-10-04T00:00:00Z",
    });
    await page.route("**/ipc", async (route) => {
      const q = route.request().postDataJSON();
      let result = {};
      if (q.method === "bootstrap")
        result = {
          configPath: "fixture",
          models: [],
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
        };
      if (q.method === "release.state") result = state();
      if (q.method === "release.check") {
        if (slowNextCheck) {
          slowNextCheck = false;
          await sleep(500);
        }
        if (failNextCheck) {
          failNextCheck = false;
          await route.fulfill({
            contentType: "application/json",
            body: JSON.stringify({
              id: q.id,
              ok: false,
              error: { code: "NETWORK", message: "更新服务不可用（夹具）" },
            }),
          });
          return;
        }
        if (noUpdateOnce) {
          noUpdateOnce = false;
          result = { ...state(), release: null };
        } else {
          if (!release) release = nextRelease();
          result = state();
        }
      }
      if (q.method === "release.configure") {
        configureCalls.push(q.params);
        result = {
          ...state(),
          analyticsEnabled: q.params.analyticsEnabled,
          channel: q.params.channel,
        };
      }
      if (q.method === "release.download") {
        downloadCalls.push(q.params ?? {});
        downloaded = true;
        result = state();
      }
      if (q.method === "release.install") {
        installCalls.push(q.params ?? {});
        installed = true;
        result = { installing: true };
      }
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({ id: q.id, ok: true, result }),
      });
    });
    await page.addInitScript(
      ({ version, themeName, localeName }) => {
        localStorage.setItem("sleepy-doll-version", version);
        localStorage.setItem("sleepy-doll-locale", localeName);
        localStorage.setItem("sleepy-doll-theme", themeName);
        localStorage.setItem("sleepy-doll-sidebar-collapsed", "true");
      },
      { version: appVersion, themeName: theme, localeName: locale },
    );
    await page.goto(`http://127.0.0.1:${port}`);
    await page.locator(".app-nav").waitFor();
    // 侧栏折叠：先点 peek 条唤出侧栏，再从账户菜单进设置页。
    await page.locator(".app-sidebar-peek").click();
    await page.locator("#app-account-trigger").click();
    const settingsLabel = locale === "en" ? "Settings" : "设置";
    await page
      .getByRole("menuitem", { name: settingsLabel, exact: true })
      .click();
    await page.locator(".settings-general").waitFor();

    // 侧栏保持折叠：正常点击 .app-brand 按钮折叠（aria-hidden 已为 true 则跳过），
    // 并把鼠标移到右侧远离 peek 条，避免 hover 唤出侧栏遮挡截图。
    const hideSidebar = async () => {
      const sidebar = page.locator(".app-sidebar");
      if ((await sidebar.getAttribute("aria-hidden")) !== "true") {
        await page.locator(".app-sidebar .app-brand button").click();
      }
      await page.mouse.move(
        page.viewportSize().width - 10,
        page.viewportSize().height / 2,
      );
      await page.waitForTimeout(250);
      assert.equal(
        await sidebar.getAttribute("aria-hidden"),
        "true",
        "截图前侧栏必须处于折叠隐藏态",
      );
    };
    await hideSidebar();

    // 通用页不再出现更新/统计/查看更新内容。
    const generalText = await page
      .locator(".settings-general")
      .textContent();
    const bannedList =
      locale === "en"
        ? ["Check for updates", "Update channel", "analytics", "release notes", "Privacy"]
        : ["检查更新", "更新通道", "统计", "查看更新内容", "隐私"];
    for (const banned of bannedList) {
      assert.ok(
        !generalText.includes(banned),
        `通用页不应出现「${banned}」`,
      );
    }
    assert.equal(
      await page.locator(".settings-general .release-settings").count(),
      0,
      "通用页不应挂更新区",
    );

    // 关于 tab：真实 SlidingTabs 导航可打开，中英文 tab 名可识别。
    const aboutTab = locale === "en" ? "About" : "关于";
    await page
      .locator(".settings-nav .sd-tabs button", { hasText: aboutTab })
      .click();
    await page.locator(".settings-about-page").waitFor();
    await page.waitForTimeout(300); // 等 tab 滑块动画。

    const zone = page.locator(".settings-about-page");
    // 客户端统计入口已整体删除：关于页只有一个软件更新组，无隐私组。
    assert.equal(
      await zone.locator(".release-settings").count(),
      1,
      "关于页只应保留一个软件更新组",
    );
    const updateGroup = zone.locator(".release-settings").first();
    const checkButton = updateGroup.locator(
      ".setting-row-control .secondary-action",
    );
    // 软件更新组只有 当前版本 + 通道 两行。
    assert.equal(
      await updateGroup.locator(".setting-row").count(),
      2,
      "更新组应只有当前版本/通道两行",
    );
    const hint = await updateGroup.locator(".setting-row-hint").textContent();
    assert.equal(hint, appVersion, "版本号只在更新组 hint 显示一次");
    const aboutText = await zone.textContent();
    const bannedAbout =
      locale === "en"
        ? ["Privacy", "anonymous usage", "On", "Off", "analytics"]
        : ["隐私", "匿名", "开启", "关闭", "统计"];
    for (const banned of bannedAbout) {
      assert.ok(
        !aboutText.includes(banned),
        `关于页不应出现统计相关文案「${banned}」`,
      );
    }
    assert.equal(
      await zone.locator("[data-ui='select-trigger']").count(),
      1,
      "关于页只应有通道一个 Select",
    );
    assert.equal(
      aboutText.split(appVersion).length - 1,
      1,
      "关于页版本号只出现一次",
    );
    assert.equal(await checkButton.textContent(), locale === "en" ? "Check for updates" : "检查更新");
    // 「查看更新内容」入口已删：全页不再有该按钮/事件入口。
    assert.equal(await page.locator(".settings-about").count(), 0);

    // label 不被侧栏等浮层遮挡：scrollIntoView 后中心点命中自身或
    // setting-row-copy 内部节点。
    const assertLabelsVisible = async (node) => {
      const occluded = await node.evaluate((rootNode) => {
        rootNode.scrollIntoView({ block: "center" });
        const rows = rootNode.querySelectorAll(".setting-row");
        for (const row of rows) {
          const copy = row.querySelector(".setting-row-copy");
          if (!copy) continue;
          const rect = copy.getBoundingClientRect();
          const x = rect.left + rect.width / 2;
          const y = rect.top + rect.height / 2;
          if (y < 0 || y > window.innerHeight || x < 0 || x > window.innerWidth)
            return row.textContent.slice(0, 20);
          const hit = document.elementFromPoint(x, y);
          if (!hit || !copy.contains(hit)) return row.textContent.slice(0, 20);
        }
        return null;
      });
      assert.equal(occluded, null, `label 被遮挡：${occluded ?? ""}`);
    };
    await assertLabelsVisible(zone);

    // 初始载入只读 state，不发 configure，更不能自动开启统计。
    await sleep(200);
    assert.deepEqual(configureCalls, [], "初始载入不应触发 release.configure");

    // 初始外观验收：控件同宽与无横向溢出（desktop + narrow）。
    const widths = await zone.evaluate((node) => {
      const button = node.querySelector(
        ".release-settings .setting-row-control .secondary-action",
      );
      const select = node.querySelector("[data-ui='select-trigger']");
      const overflow =
        document.documentElement.scrollWidth - window.innerWidth;
      return {
        button: button.getBoundingClientRect().width,
        select: select.getBoundingClientRect().width,
        overflow,
      };
    });
    assert.ok(
      Math.abs(widths.button - widths.select) <= 1,
      `检查更新按钮应与通道 Select 同宽：${widths.button} vs ${widths.select}`,
    );
    assert.ok(widths.overflow <= 1, "desktop 不应横向溢出");
    await zone.screenshot({
      path: path.join(tempRoot, `about-desktop-${theme}-${locale}.png`),
    });
    await page.setViewportSize({ width: 660, height: 760 });
    await page.waitForTimeout(250);
    await hideSidebar();
    const narrow = await page.evaluate(() => ({
      overflow: document.documentElement.scrollWidth - window.innerWidth,
      buttonWidth: document
        .querySelector(
          ".release-settings .setting-row-control .secondary-action",
        )
        .getBoundingClientRect().width,
      content: document.querySelector(".settings-content").clientWidth,
    }));
    assert.ok(narrow.overflow <= 1, "narrow 不应横向溢出");
    assert.ok(
      narrow.buttonWidth <= narrow.content + 1,
      "narrow 下按钮不应超出内容列",
    );
    await assertLabelsVisible(zone);
    await zone.screenshot({
      path: path.join(tempRoot, `about-narrow-${theme}-${locale}.png`),
    });
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForTimeout(250);
    await hideSidebar();

    // busy 态：检查期间按钮禁用并显示进行中文案。
    slowNextCheck = true;
    await checkButton.click();
    assert.ok(await checkButton.isDisabled(), "检查期间按钮应禁用");
    assert.equal(await checkButton.textContent(), locale === "en" ? "Checking…" : "正在检查…");
    await zone.locator(".release-actions").waitFor();
    assert.equal(
      await checkButton.textContent(),
      locale === "en" ? "Check for updates" : "检查更新",
      "检查结束后按钮恢复",
    );

    // 有更新：信息区 + 下载按钮，下载后切安装动作。Fake 流，不真实下载安装。
    assert.equal(
      await zone.locator(".release-info strong").textContent(),
      locale === "en" ? "New version available · 9.9.9" : "发现新版本 · 9.9.9",
    );
    const actionButton = zone.locator(".primary-action");
    assert.equal(await actionButton.textContent(), locale === "en" ? "Download update" : "下载更新");
    await actionButton.click();
    await zone
      .locator(".primary-action", { hasText: locale === "en" ? "Install and restart" : "安装并重启" })
      .waitFor();
    assert.equal(downloadCalls.length, 1, "应发出一次 release.download");
    await actionButton.click();
    await sleep(200);
    assert.equal(installCalls.length, 1, "下载后应转为 release.install");
    assert.ok(installed, "install 只发生在 fake IPC 层");

    // 当前最新分支：release.check 返回 release:null，结果走 Toast 浮层，
    // 列表内没有 release-status 段落。
    noUpdateOnce = true;
    await checkButton.click();
    const toast = page.locator(".toast");
    await toast.waitFor();
    assert.ok(
      (await toast.textContent()).includes(
        locale === "en" ? "Up to date" : "当前已是最新版本",
      ),
      "已是最新应以 Toast 提示",
    );
    assert.equal(
      await zone.locator(".release-status").count(),
      0,
      "列表内不应有 release-status",
    );
    // 等 Toast 进场动画结束再截图，拍实际可见的浮层。
    await sleep(350);
    assert.ok(await toast.isVisible(), "截图时 Toast 必须可见");
    await page.screenshot({
      path: path.join(tempRoot, `about-message-${theme}-${locale}.png`),
    });

    // 重复相同 latest 结果：重新 check 也会重置提示生命周期，浮层再次可见。
    noUpdateOnce = true;
    await checkButton.click();
    await toast.waitFor();
    await sleep(350);
    assert.ok(
      await toast.isVisible(),
      "重复相同结果应重新出现 Toast 提示",
    );
    // Toast 到时自动消失（默认 4000ms）。
    await toast.waitFor({ state: "detached", timeout: 6000 });
    assert.equal(await zone.locator(".release-actions").count(), 0);

    // 通道变更：必须回传服务端既有统计偏好（夹具初始 analyticsEnabled=true），
    // 换通道不得改写统计开关。
    await zone.locator("[data-ui='select-trigger']").click();
    await page
      .getByRole("option", { name: locale === "en" ? "Test" : "测试版", exact: true })
      .click();
    await sleep(200);
    assert.deepEqual(
      configureCalls,
      [{ analyticsEnabled: true, channel: "test" }],
      "换通道 payload 必须保留既有 analyticsEnabled",
    );

    // 错误提示：同样走 Toast，不落到列表内散排段落。
    failNextCheck = true;
    await checkButton.click();
    await page.locator(".toast", { hasText: "更新服务不可用" }).waitFor();
    assert.equal(await zone.locator(".release-status").count(), 0);
    // 点击 Toast 立即关闭。
    await page.locator(".toast").click();
    await page.locator(".toast").waitFor({ state: "detached" });

    assert.deepEqual(errors, []);
    await context.close();
    return { theme, locale, configureCalls: configureCalls.length };
  };

  const results = [];
  for (const theme of ["light", "dark"]) results.push(await runScenario(theme));
  // 英文 tab 识别（轻量：light 一轮，验证 About 导航与文案存在即可）。
  results.push(await runScenario("light", "en"));
  console.log(JSON.stringify({ scenarios: results, screenshots: tempRoot }));
} finally {
  await browser?.close();
  await server?.close();
}
