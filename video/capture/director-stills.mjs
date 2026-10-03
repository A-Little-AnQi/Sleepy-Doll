// 导演段开发截图：六个时点 PNG 写入 target/video-build/reports。不录制、不合成声音。
// 需要先构建 UI（make-video.mjs --director 或 vite build --config video/vite.config.ts）。
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { REPORTS } from "../paths.mjs";
import { openReel } from "./reel-browser.mjs";

const STILLS = [800, 2400, 3400, 5400, 6400, 7000, 8500, 10800];

const app = await openReel();
try {
  const page = await app.page();
  mkdirSync(REPORTS, { recursive: true });
  for (const ms of STILLS) {
    await page.evaluate((t) => window.__VIDEO__.seek(t), ms);
    await page.evaluate(() => window.__VIDEO__.ready());
    const file = resolve(REPORTS, `director-${ms / 1000}.png`);
    await page.screenshot({ path: file });
    console.log(`已截取：${file}`);
  }
} finally {
  await app.close();
}
