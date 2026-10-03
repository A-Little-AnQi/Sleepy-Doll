// 导演段全片审计：20 个时点截图 + 关键节点可见性检查，写 reports/director-audit.json。
// 只验证结构与渲染（pageerror、可见 rect、禁词、规格），截图本身不是美术通过的依据。
import { mkdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { REPORTS } from "../paths.mjs";
import { openReel } from "./reel-browser.mjs";

const STILLS = [
  800, 2400, 3400, 5400, 6400, 7000, 8500, 10800, 13500, 15500, 18500, 21800,
  24500, 30000, 33300, 35300, 41800, 46800, 52500, 57700,
];

// 关键节点：时点 → 名称与选择器。要求真实可见（非零 rect 且落在 1920×1080 视口内），
// 不只数 DOM 节点。
const CHECKPOINTS = new Map([
  [5400, { name: "nativeComposer", selector: ".composer-dock textarea" }],
  [7000, { name: "message", selector: ".user-message" }],
  [10800, { name: "plan", selector: ".run-plan-motion li" }],
  [13500, { name: "approval", selector: "[data-video-anchor='late-allow']" }],
  [33300, { name: "taskCardA", selector: ".task-card" }],
  [35300, { name: "taskCardB", selector: ".task-card" }],
  [41800, { name: "modelsPage", selector: ".model-settings" }],
  [46800, { name: "extensionsPage", selector: ".extensions-page" }],
  [52500, { name: "bridgePage", selector: ".bridge-page" }],
]);

// 页面上不允许出现这些原始调试键的可读文本。
const FORBIDDEN_TEXT = ["gameStatus", "runRoute", "verifyResult"];

// 点击对准实测：时点 → 按钮选择器与指针来源。A 镜指针是 director-over 内无 class 的
// svg g（识别特征：子 path d 以 "M0 0 L0 40" 开头），尖端取 g 局部原点 (0,0)；
// Late 镜指针是 .lf-pointer svg，尖端在 viewBox (8,2)。尖端均用 getScreenCTM()
// 从局部坐标换算到视口（= 舞台 1920×1080）坐标，再验证落在真实按钮 rect 内。
const CLICK_TARGETS = [
  { ms: 5820, name: "send", selector: "[data-video-anchor='sd-send']", stage: "A" },
  { ms: 14210, name: "allow", selector: "[data-video-anchor='late-allow']", stage: "late" },
  { ms: 34920, name: "run", selector: "[data-video-anchor='late-run']", stage: "late" },
];

// 确定性检查：同一时点 → 0 → 同一时点两次截图必须像素一致（允许 ≤2 级灰差且
// 差异像素 <0.05%）。明显位移或内容变化即失败。纯检查，不输出新文件。
const DETERMINISM_TIMES = [6400, 10800, 14210, 34920, 41800, 52500];
const DIFF_TOLERANCE = 2;
const DIFF_PIXEL_RATIO = 0.0005;

async function visibleRect(page, selector) {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden") return null;
    const r = el.getBoundingClientRect();
    if (r.width < 50 || r.height < 20) return null;
    if (r.right < 0 || r.bottom < 0 || r.left > innerWidth || r.top > innerHeight)
      return null;
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }, selector);
}

/** 取指针尖端与按钮 rect。返回 null 表示该帧指针不存在。缺元素直接在页面侧抛错。 */
async function pointerAlignment(page, target) {
  return page.evaluate((stage) => {
    const button = document.querySelector(stage.selector);
    if (!button)
      throw new Error(`点击实测缺按钮：t=${stage.ms}ms ${stage.selector} 不存在`);
    const rect = button.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0)
      throw new Error(`点击实测按钮不可见：t=${stage.ms}ms ${stage.selector}`);
    let tipElement = null;
    let localTip = null;
    if (stage.stage === "A") {
      const gs = Array.from(
        document.querySelectorAll(".director-over g"),
      ).filter((g) =>
        g
          .querySelector("path")
          ?.getAttribute("d")
          ?.startsWith("M0 0 L0 40"),
      );
      tipElement = gs[gs.length - 1] ?? null;
      localTip = { x: 0, y: 0 };
    } else {
      tipElement = document.querySelector(".lf-pointer svg");
      localTip = { x: 8, y: 2 };
    }
    if (!tipElement)
      throw new Error(
        `点击实测缺指针：t=${stage.ms}ms ${
          stage.stage === "A" ? ".director-over 内指针 g" : ".lf-pointer svg"
        } 不存在`,
      );
    const ctm = tipElement.getScreenCTM();
    if (!ctm)
      throw new Error(`点击实测指针未渲染（t=${stage.ms}ms，getScreenCTM 为空）`);
    const x = ctm.a * localTip.x + ctm.c * localTip.y + ctm.e;
    const y = ctm.b * localTip.x + ctm.d * localTip.y + ctm.f;
    const inside =
      x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
    return {
      method: "dom-measured",
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      tip: { x, y },
      distance: Math.hypot(
        x - (rect.left + rect.width / 2),
        y - (rect.top + rect.height / 2),
      ),
      inside,
    };
  }, target);
}

/** 用 ffmpeg 解码 PNG 为 rgb24 后逐像素比较；只读比较，不落任何文件。 */
async function compareFrames(bufferA, bufferB) {
  const decode = (buffer) => {
    const out = spawnSync(
      "ffmpeg",
      ["-loglevel", "error", "-i", "pipe:0", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
      { input: buffer, maxBuffer: 1 << 30, windowsHide: true },
    );
    if (out.status !== 0 || out.stderr.length)
      throw new Error(`ffmpeg 解码失败：${out.stderr.toString().trim()}`);
    return out.stdout;
  };
  const a = decode(bufferA);
  const b = decode(bufferB);
  if (a.length !== b.length)
    throw new Error(`帧字节长度不一致：${a.length} vs ${b.length}`);
  const pixels = a.length / 3;
  let flagged = 0;
  let maxDiff = 0;
  for (let i = 0; i < a.length; i += 3) {
    const d = Math.max(
      Math.abs(a[i] - b[i]),
      Math.abs(a[i + 1] - b[i + 1]),
      Math.abs(a[i + 2] - b[i + 2]),
    );
    if (d > maxDiff) maxDiff = d;
    if (d > DIFF_TOLERANCE) flagged++;
  }
  return {
    pixels,
    flaggedPixels: flagged,
    flaggedRatio: flagged / pixels,
    maxDiff,
    ok: flagged / pixels < DIFF_PIXEL_RATIO && maxDiff <= DIFF_TOLERANCE,
  };
}

const app = await openReel();
const report = {
  generatedAt: new Date().toISOString(),
  spec: null,
  coverage: null,
  forbiddenTextHits: [],
  timepoints: [],
  pageErrors: [],
  ok: false,
};
try {
  const page = await app.page();
  const errorsBefore = () => app.errors.length;
  const spec = await page.evaluate(() => window.__VIDEO__.spec());
  const coverage = await page.evaluate(() => window.__VIDEO__.coverage());
  report.spec = spec;
  report.coverage = coverage;
  if (
    spec.width !== 1920 ||
    spec.height !== 1080 ||
    spec.fps !== 60 ||
    spec.durationMs !== 60000
  )
    throw new Error(`舞台规格不符预期 1920×1080@60/60000：${JSON.stringify(spec)}`);
  if (coverage.directorMs !== 12000 || coverage.lateMs < 60000)
    throw new Error(`覆盖范围异常：${JSON.stringify(coverage)}`);

  mkdirSync(REPORTS, { recursive: true });
  for (const ms of STILLS) {
    const before = errorsBefore();
    await page.evaluate((t) => window.__VIDEO__.seek(t), ms);
    await page.evaluate(() => window.__VIDEO__.ready());
    const png = resolve(REPORTS, `director-${ms / 1000}.png`);
    await page.screenshot({ path: png });
    const checkpoint = CHECKPOINTS.get(ms);
    const checks = [];
    if (checkpoint) {
      const rect = await visibleRect(page, checkpoint.selector);
      checks.push({
        name: checkpoint.name,
        selector: checkpoint.selector,
        ok: rect !== null,
        rect,
      });
    }
    const forbidden = await page.evaluate((words) => {
      const text = document.body.innerText;
      return words.filter((word) => text.includes(word));
    }, FORBIDDEN_TEXT);
    if (forbidden.length)
      report.forbiddenTextHits.push({ ms, words: forbidden });
    const entry = { ms, png, checks, pageErrors: app.errors.slice(before) };
    report.timepoints.push(entry);
    console.log(
      `t=${ms} -> ${png}` +
        (checks.length
          ? ` | ${checks.map((c) => `${c.name}:${c.ok ? "可见" : "不可见"}`).join(", ")}`
          : "") +
        (entry.pageErrors.length ? ` | pageerror!` : ""),
    );
  }
  // 点击对准实测：指针尖端必须落在真实按钮 rect 内（DOM 实测，不做视口 UI 变更的
  // hover 强造测试）。缺元素或未命中直接抛错，不静默跳过。
  report.clickChecks = [];
  for (const target of CLICK_TARGETS) {
    await page.evaluate((t) => window.__VIDEO__.seek(t), target.ms);
    await page.evaluate(() => window.__VIDEO__.ready());
    const alignment = await pointerAlignment(page, target);
    report.clickChecks.push({ ...target, ...alignment });
    console.log(
      `对准 t=${target.ms} ${target.name}：tip=(${alignment.tip.x.toFixed(1)},${alignment.tip.y.toFixed(1)}) ${alignment.inside ? "在按钮内" : "不在按钮内！"}`,
    );
    if (!alignment.inside)
      throw new Error(
        `点击实测失败：t=${target.ms}ms ${target.name} 指针尖端不在 ${target.selector} rect 内`,
      );
  }

  // 确定性检查：同时点 → 0 → 同时点，帧缓冲比对，不写新文件。
  report.determinism = [];
  for (const ms of DETERMINISM_TIMES) {
    const shot = async () => {
      await page.evaluate((t) => window.__VIDEO__.seek(t), ms);
      await page.evaluate(() => window.__VIDEO__.ready());
      return page.screenshot();
    };
    const a = await shot();
    await page.evaluate(() => window.__VIDEO__.seek(0));
    await page.evaluate(() => window.__VIDEO__.ready());
    const b = await shot();
    const result = await compareFrames(a, b);
    report.determinism.push({ ms, ...result });
    console.log(
      `确定性 t=${ms}：maxDiff=${result.maxDiff} flagged=${result.flaggedPixels}/${result.pixels} ${result.ok ? "OK" : "失败！"}`,
    );
  }

  report.pageErrors = app.errors;
  const failedChecks = report.timepoints.flatMap((tp) =>
    tp.checks.filter((c) => !c.ok).map((c) => `t=${tp.ms} ${c.name}(${c.selector}) 不可见`),
  );
  report.failures = [
    ...failedChecks,
    ...report.forbiddenTextHits.map((h) => `t=${h.ms} 出现禁词 ${h.words.join("/")}`),
    ...report.determinism.filter((d) => !d.ok).map((d) => `t=${d.ms} 确定性失败（maxDiff=${d.maxDiff}, flagged=${d.flaggedPixels}/${d.pixels}）`),
  ];
  if (report.pageErrors.length)
    report.failures.push(`pageerror ${report.pageErrors.length} 条`);
  report.ok = report.failures.length === 0;
  const out = resolve(REPORTS, "director-audit.json");
  writeFileSync(out, JSON.stringify(report, null, 2));
  console.log(report.ok ? `审计通过：${out}` : `审计失败：\n- ${report.failures.join("\n- ")}`);
  if (!report.ok) process.exitCode = 1;
} finally {
  await app.close();
}
