import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { frameToolFile } from "../../scripts/frame-tooling.mjs";

export function inspectElement(element) {
  const bounds = element.getBoundingClientRect();
  const sample = (node) => {
    const style = getComputedStyle(node);
    return Object.fromEntries(
      [
        "fontFamily",
        "fontSize",
        "fontWeight",
        "lineHeight",
        "color",
        "backgroundColor",
        "backgroundImage",
        "borderRadius",
        "borderTopWidth",
        "borderTopColor",
        "padding",
        "gap",
      ].map((key) => [key, style[key]]),
    );
  };
  const rect = (r) => ({
    x: Math.round((r.x - bounds.x) * 100) / 100,
    y: Math.round((r.y - bounds.y) * 100) / 100,
    width: Math.round(r.width * 100) / 100,
    height: Math.round(r.height * 100) / 100,
  });
  const texts = [],
    walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.textContent.trim()) continue;
    const range = document.createRange();
    range.selectNodeContents(node);
    texts.push({
      value: node.textContent.trim(),
      rects: [...range.getClientRects()].map(rect),
    });
  }
  return {
    texts,
    styles: sample(element),
    descendants: [...element.querySelectorAll("*")].map((node) => {
      const r = node.getBoundingClientRect(),
        round = (value) => Math.round(value * 100) / 100;
      return {
        tag: node.tagName,
        styles: sample(node),
        box: {
          x: round(r.x - bounds.x),
          y: round(r.y - bounds.y),
          width: round(r.width),
          height: round(r.height),
        },
      };
    }),
  };
}

/** 在单独端口验证正式 Frame 服务，不占用用户正在看的 4321。 */
export async function checkFrame({ browser, root, scratch, components, PNG }) {
  const { createServer } = await import("vite");
  const { loadConfig, standaloneConfig } = await import(
    pathToFileURL(frameToolFile("loom-frame", "dist/vite.mjs")).href
  );
  const config = await loadConfig(root);
  const { catalog } = await import(
    pathToFileURL(path.join(root, "frame/catalog.ts")).href
  );
  const defaults = config.libraries[0].defaults;
  const ids = new Set(catalog.map((e) => e.id));
  for (const entry of catalog) {
    assert.ok(fs.existsSync(path.join(root, entry.source)), entry.source);
    for (const id of entry.basicComponentIds ?? []) assert.ok(ids.has(id), id);
  }
  const vite = await standaloneConfig(config, root);
  const server = await createServer({
    ...vite,
    logLevel: "error",
    server: { ...vite.server, port: 0, hmr: false, forwardConsole: false },
  });
  const errors = [],
    results = [];
  let context;
  try {
    await server.listen();
    const url = `http://127.0.0.1:${server.httpServer.address().port}`;
    context = await browser.newContext({
      viewport: { width: 1400, height: 900 },
      locale: "zh-CN",
    });
    const page = await context.newPage();
    page.setDefaultTimeout(8000);
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript(() => {
      window.__framePatches = [];
      addEventListener("message", (e) => {
        if (e.data?.type === "frame:patch") window.__framePatches.push(e.data);
      });
    });
    await page.goto(
      url + "/preview.html?library=sleepy-doll&id=sleepy-doll:switch",
    );
    const show = async (
      id,
      mode = "light",
      overrides = {},
      tokens = {},
      themeOverrides = {},
    ) => {
      const entry = catalog.find((e) => e.id === "sleepy-doll:" + id);
      assert.ok(entry, id);
      await page.evaluate(
        (data) => {
          window.__framePatches = [];
          window.postMessage(data, location.origin);
        },
        {
          type: "frame:render",
          id: entry.id,
          libraryKey: "sleepy-doll",
          props: { ...entry.props, ...overrides },
          theme: { ...defaults[mode], ...themeOverrides },
          tokens,
        },
      );
      await page
        .locator(".preview-root")
        .filter({ has: page.locator("*") })
        .waitFor();
      await page.waitForFunction(
        (label) =>
          document
            .querySelector(".preview-root")
            ?.getAttribute("aria-label") === label,
        entry.label + "预览",
      );
      await page.mouse.move(2, 2);
      await page.waitForTimeout(350);
      assert.equal(await page.locator(".preview-error").count(), 0, id);
      return entry;
    };
    fs.mkdirSync(path.join(scratch, "frame"), { recursive: true });
    for (const mode of process.argv.includes("--skip-matrix")
      ? []
      : ["light", "dark"])
      for (const entry of catalog) {
        await show(entry.id.split(":")[1], mode);
        await page.screenshot({
          path: path.join(
            scratch,
            "frame",
            `${mode}-${entry.id.split(":")[1]}.png`,
          ),
          animations: "disabled",
          caret: "hide",
        });
        results.push({ id: entry.id, mode });
      }
    const patch = async (key, value) =>
      page.waitForFunction(
        ({ key, value }) =>
          window.__framePatches.some((p) => p.key === key && p.value === value),
        { key, value },
      );
    await show("switch");
    await page.getByRole("switch").click();
    await patch("checked", false);
    await show("checkbox");
    await page.getByRole("checkbox").press("Space");
    await patch("checked", false);
    await show("text-field");
    await page.getByRole("textbox").fill("修改后的名称");
    await patch("value", "修改后的名称");
    await show("textarea");
    await page.getByRole("textbox").fill("多行\n内容");
    await patch("value", "多行\n内容");
    await show("search-field");
    await page.getByRole("textbox").fill("路线");
    await patch("value", "路线");
    await show("select");
    await page.getByRole("combobox").click();
    await page.getByRole("option", { name: "快捷任务", exact: true }).click();
    await patch("value", "task");
    await show("sliding-tabs");
    await page.getByRole("button", { name: "模型", exact: true }).click();
    await patch("value", "models");
    await show("theme-switch");
    await page.getByRole("switch").click();
    await patch("theme", "dark");
    await show("inline-rename");
    await page.getByRole("textbox").fill("重命名");
    await page.keyboard.press("Enter");
    await patch("outcome", "已提交");
    await show("inline-rename");
    await page.getByRole("textbox").press("Escape");
    await patch("outcome", "已取消");
    await show("dialog");
    await page.keyboard.press("Escape");
    await patch("open", false);
    await show("confirm-dialog");
    await page.getByRole("button", { name: "删除", exact: true }).click();
    await patch("confirmed", true);
    await patch("open", false);
    await show("toast");
    await page.getByRole("button", { name: "查看", exact: true }).click();
    await patch("clicked", true);
    await patch("visible", false);
    await show("extension-row");
    await page.getByRole("switch").click();
    await patch("enabled", false);
    await page.locator(".extension-summary").click();
    await patch("opened", true);
    await show("task-card");
    await page.getByRole("button", { name: "运行", exact: true }).click();
    await patch("running", true);
    await page.getByRole("button", { name: "停止", exact: true }).click();
    await patch("running", false);
    await page.locator(".task-card-menu > button").click();
    await page.getByRole("menuitem", { name: "取消置顶", exact: true }).click();
    await patch("pinned", false);
    await show("composer-deck");
    await page.getByRole("textbox").fill("创建采集任务");
    await page.getByRole("button", { name: "发送", exact: true }).click();
    await patch("submitted", true);

    // 同尺寸比较产品页面与 Frame 的真实组件，包括文字与状态。
    const cross = [];
    for (const sample of components) {
      const overrides =
        sample.id === "theme-switch" ? { theme: sample.theme } : {};
      await page.setViewportSize({
        width: Math.ceil(sample.width) + 96,
        height: 900,
      });
      await show(sample.id, sample.theme, overrides);
      await page.locator(".preview-root > div").evaluate((el, width) => {
        el.style.width = width + "px";
        el.style.maxWidth = "none";
      }, sample.width);
      const selectors = {
        "theme-switch": ".app-theme-switch",
        "composer-field": ".sd-composer-field",
        "task-card": ".task-card",
        "extension-row": ".extension-row",
      };
      const element = page.locator(selectors[sample.id]);
      const measured = await element.boundingBox();
      if (Math.abs(measured.width - sample.width) > 0.01) {
        await page.locator(".preview-root > div").evaluate(
          (el, width) => {
            el.style.width = width + "px";
          },
          sample.width + sample.width - measured.width,
        );
      }
      const aligned = await element.boundingBox();
      assert.ok(
        Math.abs(aligned.height - sample.height) < 0.01,
        `${sample.id} 实际高度不一致`,
      );
      // 让相同组件落在相同的子像素位置，避免裁图边界多出一行像素。
      await page.locator(".preview-root > div").evaluate(
        (el, delta) => {
          el.style.position = "relative";
          el.style.left = delta.x + "px";
          el.style.top = delta.y + "px";
        },
        {
          x: ((sample.x ?? 0) % 1) - (aligned.x % 1),
          y: ((sample.y ?? 0) % 1) - (aligned.y % 1),
        },
      );
      const {
        styles: actual,
        descendants,
        texts,
      } = await element.evaluate(inspectElement);
      if (!sample.styles.backgroundImage) delete actual.backgroundImage;
      assert.deepEqual(
        actual,
        sample.styles,
        `${sample.id}/${sample.theme} 样式与产品不一致`,
      );
      if (sample.descendants)
        assert.deepEqual(
          descendants,
          sample.descendants,
          `${sample.id}/${sample.theme} 子节点样式不一致`,
        );
      if (sample.texts)
        assert.deepEqual(
          texts,
          sample.texts,
          `${sample.id}/${sample.theme} 文本与字形布局不一致`,
        );
      const textMasks = texts.flatMap((t) => t.rects);
      const buffer = await element.screenshot({
        animations: "disabled",
        caret: "hide",
      });
      const a = PNG.sync.read(sample.image),
        b = PNG.sync.read(buffer);
      fs.writeFileSync(
        path.join(scratch, "frame", `product-${sample.theme}-${sample.id}.png`),
        sample.image,
      );
      fs.writeFileSync(
        path.join(scratch, "frame", `frame-${sample.theme}-${sample.id}.png`),
        buffer,
      );
      assert.equal(a.width, b.width, `${sample.id} 宽度`);
      assert.equal(a.height, b.height, `${sample.id} 高度`);
      let changed = 0,
        maxChannelDelta = 0;
      for (let i = 0; i < a.data.length; i += 4) {
        // 条目边缘由父列表裁切，最后一行还可能落到父边框；这里只比较条目内容。
        const x = (i / 4) % a.width,
          y = Math.floor(i / 4 / a.width);
        if (
          sample.id === "extension-row" &&
          (x < 10 || x >= a.width - 10 || y < 10 || y >= a.height - 10)
        )
          continue;
        // 文字已逐项核对内容、字体、颜色与 Range 几何；入口间只比较非文字像素，
        // 不把 Chromium 合成层产生的 LCD/灰度字缘差异计作组件设计变化。
        if (
          textMasks.some(
            (r) =>
              x >= Math.floor(r.x) - 1 &&
              x < Math.ceil(r.x + r.width) + 1 &&
              y >= Math.floor(r.y) - 1 &&
              y < Math.ceil(r.y + r.height) + 1,
          )
        )
          continue;
        // Chromium 在不同合成上下文中会切换 LCD 与灰度抗锯齿；先核对完整样式树，
        // 再以平均色阶比较相同字形，避免把彩色字缘误报为组件样式变化。
        const delta = Math.abs(
          Math.round((a.data[i] + a.data[i + 1] + a.data[i + 2]) / 3) -
            Math.round((b.data[i] + b.data[i + 1] + b.data[i + 2]) / 3),
        );
        if (delta > 3) changed++;
        maxChannelDelta = Math.max(maxChannelDelta, delta);
      }
      cross.push({
        id: sample.id,
        theme: sample.theme,
        changed,
        maxChannelDelta,
      });
      assert.ok(
        changed <= Math.max(64, Math.ceil(a.width * a.height * 0.005)),
        `${sample.id}/${sample.theme} 图片不一致：${changed}/${maxChannelDelta}`,
      );
    }
    await page.setViewportSize({ width: 1400, height: 900 });
    await show(
      "text-field",
      "dark",
      {},
      { "--control-height": "40px" },
      {
        radius: 12,
        fontSize: 16,
        borderWidth: 2,
        surface: "#242424",
        text: "#eeeeee",
      },
    );
    const themed = await page.locator("input").evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        radius: s.borderRadius,
        font: s.fontSize,
        border: s.borderTopWidth,
        height: s.height,
        bg: s.backgroundColor,
      };
    });
    assert.deepEqual(themed, {
      radius: "12px",
      font: "16px",
      border: "2px",
      height: "40px",
      bg: "rgb(36, 36, 36)",
    });
    await show("text-field");
    assert.equal(
      await page.locator("input").evaluate((el) => getComputedStyle(el).height),
      "32px",
      "移除变量后应恢复原值",
    );
    await page.goto(url + "/#sleepy-doll:switch");
    await page.locator(".inspector").waitFor();
    await page.locator(".theme-picker:not([inert])").waitFor();
    const preview = page.frameLocator(".canvas-frames iframe").first();
    await preview
      .getByRole("switch", { name: "启用组件", exact: true })
      .click();
    await page
      .locator(".inspector")
      .getByRole("switch", { name: "选中", exact: true })
      .and(page.locator('[aria-checked="false"]'))
      .waitFor();
    assert.equal(
      await page
        .locator(".inspector")
        .getByRole("switch", { name: "选中", exact: true })
        .getAttribute("aria-checked"),
      "false",
      "画布交互回传右侧参数",
    );
    await page
      .locator(".inspector")
      .getByRole("textbox", { name: "名称", exact: true })
      .fill("新开关名称");
    await preview
      .getByRole("switch", { name: "新开关名称", exact: true })
      .waitFor();
    await page.getByRole("button", { name: "深色预览", exact: true }).click();
    await preview.locator('html[data-theme="dark"]').waitFor();
    assert.equal(
      await preview.locator("html").getAttribute("data-theme"),
      "dark",
    );
    await page.screenshot({
      path: path.join(scratch, "frame/workbench.png"),
      animations: "disabled",
    });
    assert.deepEqual(errors, [], "Frame 运行错误");
    fs.writeFileSync(
      path.join(scratch, "frame-results.json"),
      JSON.stringify(
        { entries: catalog.length, renders: results.length, cross, errors },
        null,
        2,
      ),
    );
    console.log(
      JSON.stringify({
        phase: "frame",
        entries: catalog.length,
        renders: results.length,
        cross: cross.length,
        interactions: "passed",
        theme: "passed",
        workbench: "passed",
        errors: 0,
      }),
    );
  } finally {
    await context?.close();
    await server.close();
  }
}
