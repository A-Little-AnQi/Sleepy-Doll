// 风格试验台采集：Vite 构建 → 截 6 个真实组件状态（deviceScaleFactor=2）→
// 测锚点与文字 tight rect → 注入 StyleLab → seek/WebGL/shader/像素 gate 验证 →
// 25 帧 reports + 5 张 design PNG + 自包含离线预览页源文件（本轮不做浏览器交互验证）。
// TEMP/TMP/TMPDIR/npm cache 全部固定在 target/video-build，不写系统 Temp。
import { chromium } from "playwright";
import { createServer } from "node:http";
import { execSync } from "node:child_process";
import {
  mkdirSync,
  readFileSync,
  writeFileSync,
  existsSync,
} from "node:fs";
import { resolve, dirname, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { UI, CACHE, REPORTS, OUTPUT, RUNTIME } from "../paths.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const STATES = ["compose", "message", "plan", "approval", "run", "submitted"];
const SCENE_MS = 8000;
const FRAME_LOCALS = [900, 1800, 2400, 3000, 4500, 6800];
// 最终 5 PNG 时点：01=1800 全字可读+卷曲、02=1800 静态「目标」可读（白底黑字）、
// 03=1800 压力 payload、04=2400、05=2400 折射 payload；30 检查帧（FRAME_LOCALS）保留。
const FINAL_LOCALS = [1800, 1800, 1800, 2400, 2400];

mkdirSync(RUNTIME, { recursive: true });
process.env.TEMP = RUNTIME;
process.env.TMP = RUNTIME;
process.env.TMPDIR = RUNTIME;

function build() {
  execSync("npx vite build --config video/vite.config.ts", {
    cwd: ROOT,
    stdio: "inherit",
    env: { ...process.env, npm_config_cache: resolve(CACHE, "npmcache") },
  });
}

function serveDir(root) {
  const types = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".css": "text/css",
    ".png": "image/png",
    ".webp": "image/webp",
    ".svg": "image/svg+xml",
    ".json": "application/json",
    ".woff2": "font/woff2",
  };
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    const file = resolve(root, path === "/" ? "index.html" : `.${path}`);
    if (!file.startsWith(root + sep)) {
      res.writeHead(403).end();
      return;
    }
    try {
      res
        .writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" })
        .end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((done) => server.listen(0, "127.0.0.1", () => done(server)));
}

async function rectOf(page, selector) {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return null;
    return {
      x: r.x / innerWidth,
      y: r.y / innerHeight,
      w: r.width / innerWidth,
      h: r.height / innerHeight,
    };
  }, selector);
}

/** 文字 tight rect：TreeWalker 遍历真实 TextNode 的 Range 并集，padding 8px。 */
async function tightRectOf(page, selector) {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel);
    if (!root) return null;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let box = null;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!node.textContent?.trim()) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const r = range.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      box = box
        ? {
            left: Math.min(box.left, r.left),
            top: Math.min(box.top, r.top),
            right: Math.max(box.right, r.right),
            bottom: Math.max(box.bottom, r.bottom),
          }
        : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    }
    if (!box) return null;
    const pad = 8;
    const x = Math.max(0, box.left - pad);
    const y = Math.max(0, box.top - pad);
    return {
      x: x / innerWidth,
      y: y / innerHeight,
      w: Math.min(1 - x / innerWidth, (box.right + pad - x) / innerWidth),
      h: Math.min(1 - y / innerHeight, (box.bottom + pad - y) / innerHeight),
    };
  }, selector);
}

async function settle(page) {
  await page.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
}

async function captureNativeStates(port) {
  const images = {};
  const rects = {};
  const browser = await chromium.launch({ args: ["--hide-scrollbars", "--disable-lcd-text"] });
  try {
    for (const state of STATES) {
      const page = await browser.newPage({
        viewport: { width: 1440, height: 900 },
        deviceScaleFactor: 2,
      });
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      await page.goto(`http://127.0.0.1:${port}/?capture=1&design-native=1&state=${state}`);
      await page.evaluate(() => window.__VIDEO__.ready());
      const stateRects = {};
      if (state !== "run") stateRects.composer = await rectOf(page, '[data-native-anchor="composer-dock"]');
      if (state === "compose") stateRects.send = await rectOf(page, '[data-native-anchor="send"]');
      if (state === "message") stateRects.userMessage = await tightRectOf(page, '[data-native-anchor="user-message"]');
      if (state === "plan" || state === "approval") {
        await page.click(".run-plan-summary");
        await settle(page);
        if (state === "approval") {
          // 计划展开后把审批卡片滚到视口中央再测 rect/截图，
          // 防止按钮下半被 composer 遮挡、crop 误把输入框边界当审批。
          await page.evaluate(() => {
            document
              .querySelector('[data-native-anchor="approval"]')
              ?.scrollIntoView({ block: "center" });
          });
          await settle(page);
          const occluded = await page.evaluate(() => {
            const allow = document.querySelector('[data-native-anchor="approval-allow"]');
            if (!allow) return "approval-allow 锚点不存在";
            const r = allow.getBoundingClientRect();
            const cx = r.x + r.width / 2;
            const cy = r.y + r.height / 2;
            if (cx < 0 || cy < 0 || cx > innerWidth || cy > innerHeight)
              return `allow 按钮中心 (${cx.toFixed(0)},${cy.toFixed(0)}) 不在视口内`;
            const hit = document.elementFromPoint(cx, cy);
            if (!hit) return `elementFromPoint(${cx},${cy}) 无命中`;
            if (hit === allow || allow.contains(hit) || hit.contains(allow)) return null;
            return `allow 按钮中心被 ${hit.tagName}.${hit.className} 遮挡`;
          });
          if (occluded) throw new Error(`native approval 真实遮挡：${occluded}`);
        }
        stateRects.plan = await rectOf(page, '[data-native-anchor="plan"]');
        stateRects.planTight = await tightRectOf(page, '[data-native-anchor="plan"]');
        stateRects.planRows = [];
        stateRects.planRowsTight = [];
        const rows = await page.$$(".run-plan-motion-inner li");
        for (const row of rows.slice(0, 3)) {
          const handle = await row.evaluateHandle((el) => el);
          const tight = await page.evaluate((el) => {
            const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
            let box = null;
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {
              if (!node.textContent?.trim()) continue;
              const range = document.createRange();
              range.selectNodeContents(node);
              const r = range.getBoundingClientRect();
              if (r.width < 1 || r.height < 1) continue;
              box = box
                ? {
                    left: Math.min(box.left, r.left),
                    top: Math.min(box.top, r.top),
                    right: Math.max(box.right, r.right),
                    bottom: Math.max(box.bottom, r.bottom),
                  }
                : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
            }
            if (!box) return null;
            const pad = 8;
            const x = Math.max(0, box.left - pad);
            const y = Math.max(0, box.top - pad);
            return {
              x: x / innerWidth,
              y: y / innerHeight,
              w: Math.min(1 - x / innerWidth, (box.right + pad - x) / innerWidth),
              h: Math.min(1 - y / innerHeight, (box.bottom + pad - y) / innerHeight),
            };
          }, handle);
          const r = await row.boundingBox();
          if (r && tight)
            stateRects.planRows.push({
              x: r.x / 1440, y: r.y / 900, w: r.width / 1440, h: r.height / 900,
            });
          if (tight) stateRects.planRowsTight.push(tight);
        }
      }
      if (state === "approval") {
        stateRects.approval = await rectOf(page, '[data-native-anchor="approval"]');
        stateRects.approvalAllow = await rectOf(page, '[data-native-anchor="approval-allow"]');
      }
      if (state === "run") stateRects.taskButton = await rectOf(page, ".task-card .primary-action");
      if (state === "submitted") stateRects.receipt = await tightRectOf(page, '[data-native-anchor="receipt"]');
      for (const [key, value] of Object.entries(stateRects)) {
        if (!value || (Array.isArray(value) && value.length === 0))
          throw new Error(`状态 ${state} 锚点 ${key} 未测到`);
      }
      rects[state] = stateRects;
      const png = await page.screenshot();
      writeFileSync(resolve(UI, `style-native-${state}.png`), png);
      images[state] = `data:image/png;base64,${png.toString("base64")}`;
      if (errors.length) throw new Error(`native ${state} 页面错误：${errors.join("; ")}`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
  return { images, rects };
}

/** WebGL canvas 像素统计（preserveDrawingBuffer=true 可读回）。 */
async function canvasStats(page, region) {
  return page.evaluate((reg) => {
    const canvas = document.querySelector(".style-lab-canvas-host canvas");
    if (!canvas) throw new Error("WebGL canvas 不存在");
    const probe = document.createElement("canvas");
    probe.width = canvas.width;
    probe.height = canvas.height;
    const ctx = probe.getContext("2d");
    ctx.drawImage(canvas, 0, 0);
    const x0 = Math.round(reg.x * canvas.width);
    const y0 = Math.round(reg.y * canvas.height);
    const w = Math.round(reg.w * canvas.width);
    const h = Math.round(reg.h * canvas.height);
    const data = ctx.getImageData(x0, y0, w, h).data;
    let dark = 0;
    let bright = 0;
    let ink = 0;
    let nonBlack = 0;
    let minL = 255;
    let maxL = 0;
    let sum = 0;
    let minX = w, minY = h, maxX = 0, maxY = 0;
    for (let y = 0; y < h; y += 1) {
      for (let x = 0; x < w; x += 1) {
        const i = (y * w + x) * 4;
        const l = data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
        if (l < 100) dark += 1;
        if (l > 200) bright += 1;
        if (l > 110 && l < 250) ink += 1;
        if (l > 30) {
          nonBlack += 1;
          if (x < minX) minX = x;
          if (y < minY) minY = y;
          if (x > maxX) maxX = x;
          if (y > maxY) maxY = y;
        }
        minL = Math.min(minL, l);
        maxL = Math.max(maxL, l);
        sum += l;
      }
    }
    const total = w * h;
    const bbox =
      nonBlack > 20 ? { w: maxX - minX, h: maxY - minY } : { w: 0, h: 0 };
    return {
      total,
      dark,
      bright,
      ink,
      nonBlack,
      darkFrac: dark / total,
      brightFrac: bright / total,
      nonBlackFrac: nonBlack / total,
      mean: sum / total,
      bboxAspect: bbox.h > 0 ? bbox.w / bbox.h : 0,
    };
  }, region);
}

async function runLab(port, inputs) {
  const browser = await chromium.launch({ args: ["--hide-scrollbars", "--disable-lcd-text"] });
  try {
    const page = await browser.newPage({
      viewport: { width: 1920, height: 1080 },
      deviceScaleFactor: 1,
    });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(`console: ${msg.text()}`);
    });
    await page.addInitScript((value) => {
      window.__STYLE_INPUTS__ = value;
    }, inputs);
    await page.goto(`http://127.0.0.1:${port}/?capture=1&design-lab=1`);
    await page.evaluate(() => window.__VIDEO__.ready());
    const spec = await page.evaluate(() => window.__VIDEO__.spec());
    if (spec.durationMs !== 40000) throw new Error(`lab durationMs=${spec.durationMs}`);
    const hasError = () => page.evaluate(() => !!document.querySelector(".style-lab-error"));
    if (await hasError()) throw new Error("StyleLab 渲染了错误面板（WebGL 或纹理失败）");

    const seek = async (ms) => {
      await page.evaluate((v) => window.__VIDEO__.seek(v), ms);
      await settle(page);
    };

    // 5 个镜头各到位：捕获 shader 编译 / 运行错误，data-style 随 seek 切换。
    for (let preset = 0; preset < 5; preset += 1) {
      await seek(preset * SCENE_MS + 4000);
      if (await hasError()) throw new Error(`镜头 0${preset + 1} 渲染失败`);
      const style = await page.evaluate(() => document.querySelector(".style-lab")?.dataset.style);
      if (style !== String(preset + 1).padStart(2, "0"))
        throw new Error(`镜头 ${preset} data-style=${style}`);
    }

    // 确定性：同 seek 两次，位图逐字节一致。
    await seek(8123);
    const first = await page.screenshot();
    await seek(24000);
    await seek(8123);
    const second = await page.screenshot();
    if (!first.equals(second)) throw new Error("seek 8123ms 两次渲染不一致（非确定性）");

    // 像素 gate：只在有画面主张的时间点检查真实渲染结果。
    // 02 白底黑字：字形方形区（uv u 0.5±0.163、v 0.52±0.29）左右中三段都有墨
    // （两字完整）且四角无黑块 = 完整居中字形。
    const GLYPH = { x: 0.337, y: 0.23, w: 0.326, h: 0.58 };
    const check02Glyph = async (label) => {
      const whole = await canvasStats(page, GLYPH);
      if (whole.dark < 3000)
        throw new Error(`02 ${label} 字形区黑字不足（dark=${whole.dark}）`);
      if (whole.bright < whole.total * 0.3)
        throw new Error(`02 ${label} 字形区白底不足（bright=${whole.bright}/${whole.total}）`);
      for (let i = 0; i < 3; i += 1) {
        const third = await canvasStats(page, {
          x: GLYPH.x + (GLYPH.w * i) / 3,
          y: GLYPH.y,
          w: GLYPH.w / 3,
          h: GLYPH.h,
        });
        if (third.dark < 400)
          throw new Error(`02 ${label} 字形第 ${["左", "中", "右"][i]}段黑字不足（dark=${third.dark}），字形可能被裁切`);
      }
      for (const c of [
        { x: 0.02, y: 0.02 }, { x: 0.88, y: 0.02 },
        { x: 0.02, y: 0.88 }, { x: 0.88, y: 0.88 },
      ]) {
        const corner = await canvasStats(page, { ...c, w: 0.1, h: 0.1 });
        if (corner.dark > 50)
          throw new Error(`02 ${label} 角部（${c.x},${c.y}）出现黑块（dark=${corner.dark}）`);
      }
    };
    const gates = [
      {
        at: 1800, scene: "02",
        check: async () => check02Glyph("@1.8s「目标」"),
      },
      {
        at: 4500, scene: "02",
        check: async () => {
          const s = await canvasStats(page, GLYPH);
          if (s.dark < 3000) throw new Error(`02 @4.5s 轮廓变形帧为空白（dark=${s.dark}）`);
        },
      },
      {
        at: 6800, scene: "02",
        check: async () => check02Glyph("@6.8s「执行」"),
      },
      {
        at: 1800, scene: "04",
        check: async () => {
          const band = await canvasStats(page, { x: 0.2, y: 0.24, w: 0.62, h: 0.5 });
          if (band.ink < 2000)
            throw new Error(`04 @1.8s 看不到三行步骤字墨迹（ink=${band.ink}）`);
          const full = await canvasStats(page, { x: 0, y: 0, w: 1, h: 1 });
          if (full.brightFrac > 0.3)
            throw new Error(`04 @1.8s 出现大面积白条（brightFrac=${full.brightFrac.toFixed(2)}）`);
        },
      },
      {
        at: 6800, scene: "04",
        check: async () => {
          const band = await canvasStats(page, { x: 0.25, y: 0.2, w: 0.5, h: 0.66 });
          if (band.nonBlackFrac < 0.04)
            throw new Error(`04 @6.8s 计划证据为空白（nonBlackFrac=${band.nonBlackFrac.toFixed(3)}）`);
        },
      },
      {
        at: 6800, scene: "05",
        check: async () => {
          const s = await canvasStats(page, { x: 0.16, y: 0.2, w: 0.68, h: 0.7 });
          if (s.nonBlackFrac < 0.05)
            throw new Error(`05 @6.8s 回执区域空白（nonBlackFrac=${s.nonBlackFrac.toFixed(3)}）`);
          if (s.mean < 24 || s.mean > 236)
            throw new Error(`05 @6.8s 回执亮度异常（mean=${s.mean.toFixed(1)}）`);
          const expect = (inputs.rects.submitted.receipt.w * 1440) / (inputs.rects.submitted.receipt.h * 900);
          if (s.bboxAspect > 0 && Math.abs(s.bboxAspect - expect) / expect > 0.2)
            throw new Error(
              `05 @6.8s 内容 bbox 宽高比 ${s.bboxAspect.toFixed(2)} 与回执 aspect ${expect.toFixed(2)} 偏差 >20%`,
            );
        },
      },
      {
        at: 4500, scene: "05",
        check: async () => {
          const s = await canvasStats(page, { x: 0.16, y: 0.2, w: 0.68, h: 0.7 });
          if (s.nonBlackFrac < 0.05)
            throw new Error(`05 @4.5s 审批近景空白（nonBlackFrac=${s.nonBlackFrac.toFixed(3)}）`);
        },
      },
      {
        at: 1800, scene: "03",
        check: async () => {
          const s = await canvasStats(page, { x: 0.1, y: 0.3, w: 0.8, h: 0.6 });
          if (s.nonBlackFrac < 0.5)
            throw new Error(`03 @1.8s composer 近景缺失（nonBlackFrac=${s.nonBlackFrac.toFixed(2)}）`);
        },
      },
    ];
    for (const gate of gates) {
      await seek(gate.at + SCENE_MS * (Number(gate.scene) - 1));
      if (await hasError()) throw new Error(`${gate.scene} 渲染失败`);
      await gate.check();
    }

    // 05 折射实测：baseline 1.8s（env=0）vs active 2.4s（env 峰值），审批近景 ROI 像素差。
    {
      const approval = inputs.rects.approval.approval;
      const allow = inputs.rects.approval.approvalAllow;
      const aspect = (approval.w * 1440) / (approval.h * 900);
      const width = 1400;
      const height = width / aspect;
      const region = {
        x: (1920 - width) / 2 / 1920,
        y: (190 + (820 - 190 - height) / 2 + 30) / 1080,
        w: width / 1920,
        h: height / 1080,
      };
      // 触点（allow 中心）换算到屏幕归一化坐标。
      const press = {
        x: region.x + ((allow.x + allow.w / 2 - approval.x) / approval.w) * region.w,
        // Canvas2D getImageData 取上左坐标，approval 相对 y 不再翻转（shader 内 bottom-UV 保持不动）。
        y: region.y + ((allow.y + allow.h / 2 - approval.y) / approval.h) * region.h,
      };
      const SCENE05 = 4 * SCENE_MS;
      const grabBase = async () => {
        await seek(SCENE05 + 1800);
        await page.evaluate(() => {
          const canvas = document.querySelector(".style-lab-canvas-host canvas");
          const probe = document.createElement("canvas");
          probe.width = canvas.width;
          probe.height = canvas.height;
          const ctx = probe.getContext("2d");
          ctx.drawImage(canvas, 0, 0);
          window.__REFRACT_BASE__ = ctx.getImageData(0, 0, probe.width, probe.height).data;
        });
      };
      await grabBase();
      await seek(SCENE05 + 2400);
      const diff = await page.evaluate((reg) => {
        const canvas = document.querySelector(".style-lab-canvas-host canvas");
        const probe = document.createElement("canvas");
        probe.width = canvas.width;
        probe.height = canvas.height;
        const ctx = probe.getContext("2d");
        ctx.drawImage(canvas, 0, 0);
        const now = ctx.getImageData(0, 0, probe.width, probe.height).data;
        const base = window.__REFRACT_BASE__;
        const x0 = Math.round(reg.x * canvas.width);
        const y0 = Math.round(reg.y * canvas.height);
        const w = Math.round(reg.w * canvas.width);
        const h = Math.round(reg.h * canvas.height);
        let changed = 0;
        let sx = 0;
        let sy = 0;
        let minX = 1, minY = 1, maxX = 0, maxY = 0;
        for (let y = 0; y < h; y += 1) {
          for (let x = 0; x < w; x += 1) {
            const i = ((y0 + y) * canvas.width + (x0 + x)) * 4;
            const d =
              Math.abs(now[i] - base[i]) +
              Math.abs(now[i + 1] - base[i + 1]) +
              Math.abs(now[i + 2] - base[i + 2]);
            if (d > 12) {
              changed += 1;
              sx += x0 + x;
              sy += y0 + y;
              const nx = (x0 + x) / canvas.width;
              const ny = (y0 + y) / canvas.height;
              minX = Math.min(minX, nx); minY = Math.min(minY, ny);
              maxX = Math.max(maxX, nx); maxY = Math.max(maxY, ny);
            }
          }
        }
        return {
          total: w * h,
          changed,
          cx: changed ? (sx / changed) / canvas.width : 0,
          cy: changed ? (sy / changed) / canvas.height : 0,
          bbox: changed ? { minX, minY, maxX, maxY } : null,
        };
      }, region);
      const ratio = diff.changed / diff.total;
      console.log(
        `05 折射差分：changed=${diff.changed}/${diff.total}（ratio=${ratio.toFixed(5)}），质心=(${diff.cx.toFixed(3)},${diff.cy.toFixed(3)})，bbox=${diff.bbox ? `${diff.bbox.minX.toFixed(3)},${diff.bbox.minY.toFixed(3)}~${diff.bbox.maxX.toFixed(3)},${diff.bbox.maxY.toFixed(3)}` : "无"}，触点=(${press.x.toFixed(3)},${press.y.toFixed(3)})，region=${region.x.toFixed(3)},${region.y.toFixed(3)},${region.w.toFixed(3)}x${region.h.toFixed(3)}`,
      );
      if (diff.changed < 100)
        throw new Error(`05 折射差异不足（changed=${diff.changed}）：触点/折射坐标需修`);
      const far =
        Math.abs(diff.cx - press.x) > region.w * 0.35 ||
        Math.abs(diff.cy - press.y) > region.h * 0.35;
      if (far)
        throw new Error(
          `05 折射质心 (${diff.cx.toFixed(3)},${diff.cy.toFixed(3)}) 偏离触点 (${press.x.toFixed(3)},${press.y.toFixed(3)})：坐标需修`,
        );
      await page.evaluate(() => {
        window.__REFRACT_BASE__ = undefined;
      });
    }

    // 25 帧 reports + 最终 5 PNG。
    mkdirSync(REPORTS, { recursive: true });
    for (let preset = 0; preset < 5; preset += 1) {
      for (const local of FRAME_LOCALS) {
        await seek(preset * SCENE_MS + local);
        writeFileSync(
          resolve(REPORTS, `design-${String(preset + 1).padStart(2, "0")}-${local}.png`),
          await page.screenshot(),
        );
      }
      await seek(preset * SCENE_MS + FINAL_LOCALS[preset]);
      if (await hasError()) throw new Error(`镜头 ${preset + 1} 最终帧渲染失败`);
      writeFileSync(
        resolve(OUTPUT, `design-${String(preset + 1).padStart(2, "0")}.png`),
        await page.screenshot(),
      );
    }
    if (errors.length)
      throw new Error(`lab 页面错误（shader/运行时）：${errors.slice(0, 4).join(" | ")}`);
    await page.close();
  } finally {
    await browser.close();
  }
}

function buildPreview(inputs) {
  let html = readFileSync(resolve(UI, "index.html"), "utf8");
  const dir = (file) => readFileSync(resolve(UI, file.replace(/^\.\//, "")), "utf8");
  html = html.replace(
    /<script type="module"[^>]*src="([^"]+)"[^>]*><\/script>/g,
    (m, src) => `<script type="module">\n${dir(src)}\n</script>`,
  );
  html = html.replace(
    /<link rel="stylesheet"[^>]*href="([^"]+)"[^>]*>/g,
    (m, href) => `<style>\n${dir(href)}\n</style>`,
  );
  const injection = `<script>window.__DESIGN_LAB__=true;try{history.replaceState(null,"","?design-lab=1")}catch(e){}</script>\n<script>window.__STYLE_INPUTS__=${JSON.stringify(inputs).replaceAll("<", "\\u003c")};</script>`;
  html = html.replace("</head>", `${injection}\n</head>`);
  // 预览模式专用样式（只注入到离线页，不改业务 CSS）：两组控件 fixed 置顶、可点，
  // 不被画布或缩放层遮挡，且互不重叠。
  const controlCss = `<style>
#design-controls, .preview-controls { position: fixed !important; z-index: 2147483000 !important; pointer-events: auto !important; }
#design-controls, #design-controls button { pointer-events: auto !important; }
#design-controls { left: 16px !important; bottom: 18px !important; }
</style>`;
  html = html.replace("</head>", `${controlCss}\n</head>`);
  const controls = `<div id="design-controls" style="position:fixed;left:16px;bottom:18px;z-index:2147483000;display:flex;gap:12px;pointer-events:auto">
${[0, 1, 2, 3, 4].map((i) => `<button data-scene="${i}">镜 0${i + 1}</button>`).join("")}
</div>
<script>
  for (const b of document.querySelectorAll("#design-controls button"))
    b.addEventListener("click", () => window.__VIDEO__.seek(Number(b.dataset.scene) * 8000 + 400));
</script>`;
  html = html.replace("</body>", `${controls}\n</body>`);
  const leftover = html.match(/(?:src|href)="\.?\/?assets\/[^"]+"/);
  if (leftover) throw new Error(`预览页仍有未内联资产引用：${leftover[0]}`);
  writeFileSync(resolve(OUTPUT, "design-preview.html"), html);
}

function writeDirections() {
  const directions = {
    version: 5,
    status: "第四轮缺陷修复：02 SDF 符号反转（out=sqrt(din)-sqrt(dout)，白底黑字）+逐 mask 符号断言、approval 捕获 scrollIntoView 居中+elementFromPoint 遮挡检查、最终帧改为 1800/1800/1800/2400/2400；30 检查帧保留；离线预览交互未本轮验证，等待导演看图",
    date: "2026-10-02",
    scope: "40 秒风格比较试验，不是成片；正式界面仍由当前组件每次重建",
    inputs:
      "6 张 native 截图（compose/message/plan/approval/run/submitted）来自当前源码真实组件，deviceScaleFactor=2（2880×1800），锚点与文字 tight rect 一次采集入 metadata",
    scenes: [
      {
        id: 1,
        name: "黑白文字曲面",
        technique:
          "128px 大字 alpha 纹理贴 96×24 细分曲面，vertex 圆柱卷曲（R 520→∞），局部折回峰值 θ≤0.8rad 保持可读，解析法线光照；UI 为 composer/approval/receipt 真实截图 aspect fit（宽 ≤1180）",
        frameLocalMs: FINAL_LOCALS[0],
      },
      {
        id: 2,
        name: "白场字形轮廓",
        technique:
          "Canvas 亮度采样 → Felzenszwalb 两遍 1D 平方 EDT 的真欧氏 SDF（内负外正，覆盖 2%–60% 断言），GLSL 距离场插值 + 仅边界涡旋 uv 变形；1.8s「目标」/4.5s 变形/6.8s「执行」均过像素 gate",
        frameLocalMs: FINAL_LOCALS[1],
      },
      {
        id: 3,
        name: "冷蓝界面弹性",
        technique:
          "composer-dock tight 近景（宽 1440 aspect fit）贴 64×48 网格，send 实测 rect 相对裁剪区换算为压力原点，z 波幅 80px、0.8–2.0s 连续形变后指数消散；平面后才交接真实消息/计划裁剪",
        frameLocalMs: FINAL_LOCALS[2],
      },
      {
        id: 4,
        name: "原生计划的空间折页",
        technique:
          "li TreeWalker 文字 tight rect 裁真实步骤墨迹，白底转透明黑字转白 alpha；每行 72×12 网格左 hinge 连续圆柱弯曲（R≥W/1.2 不自遮），0.15s 错峰，3.6s 归整为 native plan tight aspect fit",
        frameLocalMs: FINAL_LOCALS[3],
      },
      {
        id: 5,
        name: "局部光学",
        technique:
          "审批近景与已提交回执各自 aspect 居中区（宽 ≤1400），允许字形 SDF 高度场，x/y 分别数值梯度折射（各轴 16px 上限）+ 0.985/1/1.015 RGB 微色散 + Fresnel；2.0s 按下 0.8s 收束后回原像素，5.0–5.3 切真实回执",
        frameLocalMs: FINAL_LOCALS[4],
      },
    ],
    verified: [
      "6 状态 native 截图 + send/composer/userMessage/approval/approvalAllow/plan/planRows(+tight)/taskButton/receipt 归一化 rect 全部实测",
      "pageerror 与 console error 为空；data-style 01..05 随 seek 切换；seek 8123ms 两次渲染逐字节一致",
      "像素 gate：02 字形三段墨迹+四角无黑块、04 行墨迹与白条上限、05 回执非空 + bbox aspect 偏差 ≤20%、03 近景覆盖",
      "05 折射差分：1.8s baseline vs 2.4s active，审批 ROI 实际 changed 像素数与质心-触点距离（见 stdout）；触点 y 坐标已修正为上左系不翻转",
      "30 帧（每镜 900/1800/2400/3000/4500/6800ms）在 target/video-build/reports",
    ],
    notVerified: [
      "离线预览未在受限浏览器连续观看，交互未本轮验证（此前“5 按钮普通 click 已验证”不再沿用）",
      "非 60 秒成片，未导出 MP4（用户先选风格）",
      "构图与节奏审美待导演看图返修",
    ],
  };
  writeFileSync(resolve(OUTPUT, "design-directions.json"), JSON.stringify(directions, null, 2));
}

async function main() {
  if (!existsSync(resolve(UI, "index.html")) || process.argv.includes("--build"))
    build();
  const server = await serveDir(UI);
  try {
    const inputs = await captureNativeStates(server.address().port);
    writeFileSync(resolve(CACHE, "style-inputs.json"), JSON.stringify(inputs));
    await runLab(server.address().port, inputs);
    buildPreview(inputs);
    writeDirections();
    console.log("design-lab 完成：6 状态截图、25 帧、5 PNG、HTML 源文件已生成；离线预览交互未验证");
  } finally {
    await new Promise((done) => server.close(done));
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
