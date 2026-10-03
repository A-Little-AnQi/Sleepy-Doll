import { chromium } from "playwright";
import { UI, RUNTIME } from "../paths.mjs";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { resolve, extname, sep } from "node:path";

export async function openReel(query = "") {
  const root = UI;
  const types = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".css": "text/css",
    ".webp": "image/webp",
    ".svg": "image/svg+xml",
    ".wav": "audio/wav",
  };
  const server = createServer((req, res) => {
    const path = decodeURIComponent(
      new URL(req.url, "http://localhost").pathname,
    );
    const file = resolve(root, path === "/" ? "index.html" : `.${path}`);
    if (!file.startsWith(root + sep)) {
      res.writeHead(403).end();
      return;
    }
    try {
      res
        .writeHead(200, {
          "content-type": types[extname(file)] ?? "application/octet-stream",
        })
        .end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  // Chromium 的临时 profile 与 artifacts 跟随 TEMP/TMP/TMPDIR；启动前统一指向 runtime，不写系统 Temp。
  mkdirSync(RUNTIME, { recursive: true });
  process.env.TEMP = RUNTIME;
  process.env.TMP = RUNTIME;
  process.env.TMPDIR = RUNTIME;
  let browser;
  try {
    browser = await chromium.launch({
      args: ["--hide-scrollbars", "--disable-lcd-text"],
    });
  } catch (error) {
    server.close();
    throw error;
  }
  const errors = [];
  return {
    errors,
    async page() {
      const page = await browser.newPage({
        viewport: { width: 1920, height: 1080 },
        deviceScaleFactor: 1,
      });
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(
        `http://127.0.0.1:${server.address().port}/?capture=1&${query}`,
      );
      await page.evaluate(() => window.__VIDEO__.ready());
      const spec = await page.evaluate(() => window.__VIDEO__.spec());
      await page.setViewportSize({ width: spec.width, height: spec.height });
      await page.evaluate(() => window.__VIDEO__.ready());
      return page;
    },
    async close() {
      await browser.close();
      await new Promise((done) => server.close(done));
    },
  };
}
