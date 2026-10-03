// 逐帧录制；中间视频和唯一成片都只能写入 video/output。
import { spawn, spawnSync, execFileSync } from "node:child_process";
import {
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
  unlinkSync,
} from "node:fs";
import { resolve } from "node:path";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { availableParallelism, freemem } from "node:os";
import { openReel } from "./reel-browser.mjs";
import { OUTPUT, CACHE, UI, AUDIO, REPORTS, FINAL_VIDEO } from "../paths.mjs";
import { assertSingleVideoDirectory } from "../cleanup.mjs";

if (process.env.VIDEO_PIPELINE !== "1")
  throw new Error("请使用 make-video.bat；禁止跳过旧视频清理直接录制。");
assertSingleVideoDirectory();
const app = await openReel();
const parts = [];
try {
  const infoPage = await app.page();
  const spec = await infoPage.evaluate(() => window.__VIDEO__.spec());
  await infoPage.close();
  if (spec.durationMs !== 60000) throw new Error("完整产品介绍必须为 60 秒");
  const fps = process.argv.includes("--draft") ? 30 : spec.fps;
  const total = Math.round((spec.durationMs * fps) / 1000);
  mkdirSync(OUTPUT, { recursive: true });
  mkdirSync(REPORTS, { recursive: true });
  let completed = 0;
  const started = Date.now();
  const workers = Math.min(freemem() > 6 * 1024 ** 3 ? 6 : 4, Math.max(2, Math.floor(availableParallelism() / 2)));
  const jobs = Array.from({ length: workers }, async (_, worker) => {
    const from = Math.floor((total * worker) / workers),
      to = Math.floor((total * (worker + 1)) / workers);
    const page = await app.page();
    const file = resolve(OUTPUT, `.part-${worker}.mp4`);
    parts.push(file);
    const encoder = spawn(
      "ffmpeg",
      [
        "-y",
        "-v",
        "error",
        "-f",
        "image2pipe",
        "-vcodec",
        "mjpeg",
        "-framerate",
        String(fps),
        "-i",
        "pipe:0",
        "-an",
        "-c:v",
        "libx264",
        "-preset",
        "fast",
        "-crf",
        "16",
        "-threads",
        "2",
        "-pix_fmt",
        "yuv420p",
        "-video_track_timescale",
        "60000",
        file,
      ],
      { windowsHide: true, stdio: ["pipe", "ignore", "pipe"] },
    );
    let error = "",
      pipeError;
    encoder.stderr.on("data", (chunk) => (error += chunk));
    encoder.stdin.on("error", (reason) => (pipeError = reason));
    const done = new Promise((ok, fail) => {
      encoder.on("error", fail);
      encoder.on("close", (code) =>
        code === 0 ? ok() : fail(new Error(error || `编码失败 ${code}`)),
      );
    });
    done.catch(() => undefined);
    try {
      for (let frame = from; frame < to; frame++) {
        if (pipeError) throw pipeError;
        if (app.errors.length) throw new Error(app.errors.join("\n"));
        await page.evaluate(
          (t) => window.__VIDEO__.seek(t),
          (frame * 1000) / fps,
        );
        await page.evaluate(() => window.__VIDEO__.ready());
        const buffer = await page.screenshot({ type: "jpeg", quality: 98 });
        if (!encoder.stdin.write(buffer)) await once(encoder.stdin, "drain");
        completed++;
        if (completed % 240 === 0)
          console.log(
            `${completed}/${total} 帧 · ${(completed / ((Date.now() - started) / 1000)).toFixed(1)} fps`,
          );
      }
      encoder.stdin.end();
      await done;
    } finally {
      if (encoder.exitCode === null) encoder.kill();
      await page.close();
    }
    return file;
  });
  const results = await Promise.allSettled(jobs);
  const failed = results.find((item) => item.status === "rejected");
  if (failed) throw failed.reason;
  const list = resolve(CACHE, "concat.txt");
  const files = results.map((item) => item.value);
  writeFileSync(
    list,
    files
      .map(
        (file) =>
          `file '${file.replaceAll("\\", "/").replaceAll("'", "'\\''")}'`,
      )
      .join("\n"),
  );
  const pending = resolve(OUTPUT, ".pending.mp4");
  execFileSync(
    "ffmpeg",
    [
      "-y",
      "-v",
      "error",
      "-f",
      "concat",
      "-safe",
      "0",
      "-i",
      list,
      "-i",
      resolve(AUDIO, "master.m4a"),
      "-map",
      "0:v:0",
      "-map",
      "1:a:0",
      "-c:v",
      "copy",
      "-c:a",
      "copy",
      "-t",
      "60",
      "-movflags",
      "+faststart",
      pending,
    ],
    { stdio: "inherit", windowsHide: true },
  );
  const probe = JSON.parse(
    execFileSync(
      "ffprobe",
      ["-v", "error", "-show_streams", "-show_format", "-of", "json", pending],
      { encoding: "utf8", windowsHide: true },
    ),
  );
  const video = probe.streams.find((s) => s.codec_type === "video"),
    audio = probe.streams.find((s) => s.codec_type === "audio");
  if (
    !video ||
    !audio ||
    Number(video.nb_frames) !== total ||
    video.width !== spec.width ||
    video.height !== spec.height ||
    Math.abs(Number(probe.format.duration) - 60) > 0.05
  )
    throw new Error("成片规格或音轨不符合要求");
  execFileSync("ffmpeg", ["-v", "error", "-i", pending, "-f", "null", "-"], {
    stdio: "inherit",
    windowsHide: true,
  });
  const loudness = spawnSync(
    "ffmpeg",
    [
      "-hide_banner",
      "-i",
      pending,
      "-af",
      "loudnorm=I=-14:TP=-1:LRA=7:print_format=json",
      "-f",
      "null",
      "-",
    ],
    { encoding: "utf8", windowsHide: true },
  );
  const reading = loudness.stderr.match(/\{\s*"input_i"[\s\S]*?\}/)?.[0];
  if (loudness.status !== 0 || !reading)
    throw new Error("无法验证交付音轨响度");
  const sound = JSON.parse(reading);
  if (Math.abs(Number(sound.input_i) + 14) > 0.5 || Number(sound.input_tp) > -1)
    throw new Error(`交付音轨未达标：${reading}`);
  renameSync(pending, FINAL_VIDEO);
  for (const file of parts) unlinkSync(file);
  execFileSync(
    "ffmpeg",
    [
      "-y",
      "-v",
      "error",
      "-ss",
      "2",
      "-i",
      FINAL_VIDEO,
      "-frames:v",
      "1",
      resolve(OUTPUT, "poster.jpg"),
    ],
    { stdio: "inherit", windowsHide: true },
  );
  // 元数据换成 Qwen 管线来源：director-cues（句级）+ director-audit；不再读旧 cues.json/recording.json。
  const cues = JSON.parse(readFileSync(resolve(CACHE, "director-cues.json"), "utf8"));
  if (cues.provider !== "qwen-ssh" || cues.wordTiming !== false)
    throw new Error("director-cues.json 不是 Qwen 句级数据");
  const audit = JSON.parse(readFileSync(resolve(REPORTS, "director-audit.json"), "utf8"));
  const BEATS = [2.4, 6.4, 10.8, 13.5, 18.5, 24.5, 30, 35.3, 41.8, 46.8, 52.5, 57.7];
  for (const at of BEATS)
    execFileSync(
      "ffmpeg",
      ["-y", "-v", "error", "-ss", String(at), "-i", FINAL_VIDEO, "-frames:v", "1",
        resolve(REPORTS, `delivered-${at}.jpg`)],
      { stdio: "inherit", windowsHide: true },
    );
  writeFileSync(
    resolve(OUTPUT, "manifest.json"),
    JSON.stringify(
      {
        output: FINAL_VIDEO,
        renderedAt: new Date().toISOString(),
        spec,
        fps,
        frames: total,
        workers,
        audio: {
          provider: cues.provider,
          wordTiming: false,
          timingType: cues.timingType,
          measurement: sound,
          mastering: JSON.parse(readFileSync(resolve(AUDIO, "mastering.json"), "utf8")),
          clips: cues.segments.map((s) => ({
            id: s.id,
            durationMs: s.duration,
            windowMs: s.window,
            speechStartMs: s.speechStart,
            speechEndMs: s.speechEnd,
          })),
        },
        uiAudit: audit,
        video: probe,
        bundleSha256: createHash("sha256")
          .update(readFileSync(resolve(UI, "index.html")))
          .digest("hex"),
      },
      null,
      2,
    ),
  );
  assertSingleVideoDirectory(true);
  console.log(`录制完成：${FINAL_VIDEO}`);
} finally {
  await app.close();
}
