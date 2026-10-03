// 校验配置中的 UI 契约，以及改动布局后的自动取景和点击定位。
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { openReel } from "./reel-browser.mjs";
import { playbackForStory } from "../audio/resolve-cues.mjs";
import { REPORTS } from "../paths.mjs";
import { resolve } from "node:path";
const cueSheet = JSON.parse(
  readFileSync("target/video-build/cues.json", "utf8"),
);
const recipe = JSON.parse(readFileSync("video/recording.json", "utf8"));
const app = await openReel();
const issues = [];
const rasterRounding = [];
mkdirSync(resolve(REPORTS, "stills"), { recursive: true });
try {
  const page = await app.page();
  const spec = await page.evaluate(() => window.__VIDEO__.spec());
  if (spec.durationMs !== 60000) issues.push("完整产品介绍必须为 60 秒");
  const seek = async (storyTime) => {
    await page.evaluate(
      (t) => window.__VIDEO__.seek(t),
      playbackForStory(storyTime, cueSheet.marks),
    );
    await page.evaluate(() => window.__VIDEO__.ready());
  };
  for (const check of recipe.checks) {
    await seek(check.at);
    for (const key of check.anchors) {
      const selector = recipe.anchors[key];
      const count = await page.locator(`.film-live ${selector}`).count();
      if (key === "steps" ? count < 1 : count !== 1)
        issues.push(
          `${check.name} (${check.at}ms): 锚点 ${key} 定位数量不符合契约，实际 ${count}。请更新 recording.json: ${selector}`,
        );
    }
    await page.screenshot({
      path: resolve(REPORTS, "stills", `${check.at}.png`),
    });
  }
  for (let t = 3900; t <= 5250; t += 50) {
    if (issues.length) throw new Error(issues.join("\n"));
    await seek(t);
    const box = await page
      .locator(`.film-live ${recipe.anchors.message}`)
      .boundingBox();
    if (
      !box ||
      box.x < 0 ||
      box.y < 0 ||
      box.x + box.width > recipe.width ||
      box.y + box.height > recipe.height
    )
      issues.push({ t, messageOutside: box });
  }
  async function checkClicks(label) {
    for (const [at, key] of recipe.clicks) {
      await seek(at);
      const hit = await page.evaluate((selector) => {
        const b = document
          .querySelector(`.film-live ${selector}`)
          ?.getBoundingClientRect();
        const c = document
          .querySelector(".video-cursor")
          ?.getBoundingClientRect();
        return !!(
          b &&
          c &&
          c.x + 5 >= b.left &&
          c.x + 5 <= b.right &&
          c.y + 5 >= b.top &&
          c.y + 5 <= b.bottom
        );
      }, recipe.anchors[key]);
      if (!hit) issues.push({ label, at, missed: key });
    }
  }
  await checkClicks("当前UI");
  const hash = (b) => createHash("sha256").update(b).digest("hex");
  for (const t of [4200, 11000, 18000, 26400, 29100]) {
    await seek(t);
    const first = await page.screenshot();
    const a = hash(first);
    await seek(0);
    await seek(t);
    const second = await page.screenshot();
    if (a !== hash(second)) {
      const decode = (png) =>
        execFileSync(
          "ffmpeg",
          [
            "-v",
            "error",
            "-i",
            "pipe:0",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
          ],
          { input: png, maxBuffer: 32 * 1024 * 1024, windowsHide: true },
        );
      const left = decode(first),
        right = decode(second);
      let changed = 0,
        maxDifference = 0;
      const region = { left: recipe.width, top: recipe.height, right: 0, bottom: 0 };
      for (let i = 0; i < left.length; i++) {
        const delta = Math.abs(left[i] - right[i]);
        if (delta) changed++;
        maxDifference = Math.max(maxDifference, delta);
        if (delta > 2) {
          const pixel = Math.floor(i / 3), x = pixel % recipe.width, y = Math.floor(pixel / recipe.width);
          region.left = Math.min(region.left, x); region.right = Math.max(region.right, x);
          region.top = Math.min(region.top, y); region.bottom = Math.max(region.bottom, y);
        }
      }
      // 仅允许极少量边缘像素的不超过两级的舍入差异，不接受位移或内容变化。
      if (
        left.length !== right.length ||
        maxDifference > 2 ||
        changed / left.length > 0.0001
      )
      {
        writeFileSync(resolve(REPORTS, `seek-${t}-a.png`), first);
        writeFileSync(resolve(REPORTS, `seek-${t}-b.png`), second);
        issues.push({ t, nondeterministic: true, changed, maxDifference, region });
      }
      else rasterRounding.push({ t, changed, maxDifference });
    }
  }
  const override = await page.addStyleTag({
    content:
      ".film-live .run-approval{margin-left:24px!important;padding:24px!important}.film-live .task-card{width:520px!important;max-width:100%!important}.film-live .composer-dock{transform:translateX(-32px)!important}",
  });
  await checkClicks("布局变化：审批偏移、任务卡增宽、输入框移动");
  await override.evaluate((el) => el.remove());
  for (const feature of recipe.features) {
    const at = feature.start + 1800;
    await page.evaluate((ms) => window.__VIDEO__.seek(ms), at);
    await page.evaluate(() => window.__VIDEO__.ready());
    if ((await page.locator(`[data-feature='${feature.page}']`).count()) !== 1)
      issues.push({ at, featureMissing: feature.page });
    await page.screenshot({
      path: resolve(REPORTS, "stills", `feature-${feature.page}.png`),
    });
  }
  for (const segment of cueSheet.alignment) {
    await page.evaluate((ms) => window.__VIDEO__.seek(ms), segment.words[0].start + 100);
    await page.evaluate(() => window.__VIDEO__.ready());
    const r = await page.locator(".narration-subtitle").boundingBox();
    if (
      !r ||
      r.x < 0 ||
      r.x + r.width > recipe.width ||
      r.y + r.height > recipe.height
    )
      issues.push({ subtitleOutside: segment.text });
  }
  await page.evaluate((ms) => window.__VIDEO__.seek(ms), 58500);
  await page.evaluate(() => window.__VIDEO__.ready());
  const brandIcon = await page.locator(".brand-lockup .app-brand-icon").boundingBox();
  const brandTitle = await page.locator(".brand-lockup h2").boundingBox();
  const brandDetail = await page.locator(".brand-lockup p").boundingBox();
  if (!brandIcon || brandIcon.width < 96 || !brandTitle || !brandDetail ||
    brandDetail.x < brandTitle.x - 2 || brandDetail.y < brandTitle.y + brandTitle.height - 4)
    issues.push({ brandLayout: { brandIcon, brandTitle, brandDetail } });
  issues.push(...app.errors);
  writeFileSync(
    resolve(REPORTS, "audit.json"),
    JSON.stringify(
      {
        checks: recipe.checks.length,
        layoutMutation: true,
        rasterRounding,
        issues,
      },
      null,
      2,
    ),
  );
  if (issues.length) throw new Error(JSON.stringify(issues, null, 2));
  console.log(
    "检查通过：UI 锚点契约、消息路径、点击、确定性，以及布局改动后的定位。",
  );
} finally {
  await app.close();
}
