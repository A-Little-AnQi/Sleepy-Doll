// 用户入口：Qwen 旁白制作管线（SSH 远端合成，客户端无 Python/Edge）。
// --check 只读检查；--prepare 跑到混音不录片并保留构建；--director 仅 tsc/vite/导演截图；
// 默认完整制作并导出唯一成片（coverage 校验通过 DirectorFilm 12000 + LateFilm 60000）。
import { execFileSync, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  openSync,
  closeSync,
  writeFileSync,
  readFileSync,
  unlinkSync,
} from "node:fs";
import { resolve } from "node:path";
import { ROOT, OUTPUT, CACHE, AUDIO, REPORTS, RUNTIME, FINAL_VIDEO } from "./paths.mjs";
import {
  cleanupPlan,
  cleanOldOutputs,
  assertSingleVideoDirectory,
  cleanIntermediateOutputs,
} from "./cleanup.mjs";

const NPM_CACHE = resolve(CACHE, ".cache");
// 必须在任何子进程（依赖探测、ffmpeg、vite、node）启动之前统一指向项目内目录；
// --check 只读模式不创建这些目录。
process.env.TEMP = RUNTIME;
process.env.TMP = RUNTIME;
process.env.TMPDIR = RUNTIME;
process.env.npm_config_cache = NPM_CACHE;

const args = process.argv.slice(2);
const allowed = new Set(["--check", "--preview", "--prepare", "--director"]);
const unknown = args.filter((arg) => !allowed.has(arg));

function command(file, parameters, options = {}) {
  return execFileSync(file, parameters, {
    cwd: ROOT,
    windowsHide: true,
    stdio: "inherit",
    ...options,
  });
}

function dependencyStatus() {
  const probe = (file, parameters) =>
    spawnSync(file, parameters, { cwd: ROOT, windowsHide: true, encoding: "utf8" });
  const [major, minor] = process.versions.node.split(".").map(Number);
  return {
    checks: [
      ["Node.js >=22.13", major > 22 || (major === 22 && minor >= 13)],
      ["FFmpeg", probe("ffmpeg", ["-version"]).status === 0],
      ["ffprobe", probe("ffprobe", ["-version"]).status === 0],
      ["SSH 客户端（Qwen 远端合成）", probe("ssh", ["-V"]).error === undefined],
      [
        "项目 npm 依赖",
        ["playwright", "typescript", "vite", "react"].every((name) =>
          existsSync(resolve(ROOT, `node_modules/${name}/package.json`)),
        ),
      ],
    ],
  };
}

function narrationConfig() {
  return JSON.parse(readFileSync(resolve(ROOT, "video/audio/director-narration.json"), "utf8"));
}

function narrationCacheComplete() {
  try {
    const manifest = JSON.parse(
      readFileSync(resolve(AUDIO, "voice/manifest-qwen-ssh.json"), "utf8"),
    );
    const narration = narrationConfig();
    return (
      manifest.provider === "qwen-ssh" &&
      !manifest.failures?.length &&
      narration.segments.every((seg) => manifest.segments?.some((s) => s.id === seg.id))
    );
  } catch {
    return false;
  }
}

// 前段常量 12000 + LateFilm 60000 + main 已使用 LateFilm，三者齐备才允许录全片；
// 不修改导演文件、不回落旧 ProductFilm。
function assertFullCoverage() {
  const read = (path) => readFileSync(resolve(ROOT, path), "utf8");
  if (!/DIRECTOR_COVERAGE_MS\s*=\s*12000\b/.test(read("video/director/DirectorFilm.tsx")))
    throw new Error("DirectorFilm 覆盖常量应为 12000（前段 0-12s），当前不符，需人工核对。");
  let late;
  try {
    late = read("video/director/LateFilm.tsx");
  } catch {
    throw new Error("video/director/LateFilm.tsx 尚未就绪：禁止导出不完整成片。");
  }
  if (!/LATE_COVERAGE_MS\s*=\s*60000\b/.test(late))
    throw new Error("LateFilm 的 LATE_COVERAGE_MS 必须为 60000。");
  if (!/LateFilm/.test(read("video/main.tsx")))
    throw new Error("video/main.tsx 尚未使用 LateFilm。");
}

function acquireLock() {
  const lock = resolve(ROOT, ".video-build.lock");
  const payload = JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() });
  try {
    const handle = openSync(lock, "wx");
    closeSync(handle);
    writeFileSync(lock, payload);
    return lock;
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
  }
  // 精确 PID：持有者进程仍存活则退出；已退出则接管陈旧锁。
  let pid;
  try {
    pid = JSON.parse(readFileSync(lock, "utf8")).pid;
  } catch {
    throw new Error(".video-build.lock 内容无效，请确认无制作进程后删除该文件。");
  }
  if (typeof pid !== "number")
    throw new Error(".video-build.lock 缺少 PID，请确认无制作进程后删除该文件。");
  try {
    process.kill(pid, 0);
    throw new Error(
      `另一个制作任务（PID ${pid}）可能正在运行。请先等待；确认无制作进程后删除 .video-build.lock。`,
    );
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
  console.log("发现陈旧锁（持有进程已退出），接管。");
  writeFileSync(lock, payload);
  return lock;
}

async function main() {
  if (unknown.length) throw new Error(`未知参数：${unknown.join(" ")}`);
  process.chdir(ROOT);

  if (args.includes("--check")) {
    console.log("只读检查：不创建目录、不删除文件、不合成语音、不启动录制。");
    for (const item of cleanupPlan())
      console.log(
        `清理范围：${item.path} / ${item.files} 个文件 / ${(item.bytes / 1024 ** 3).toFixed(2)} GiB`,
      );
    const status = dependencyStatus();
    for (const [name, ready] of status.checks)
      console.log(`${ready ? "OK" : "MISSING"} ${name}`);
    console.log(`唯一视频输出目录：${OUTPUT}`);
    console.log(
      narrationCacheComplete()
        ? "Qwen 旁白缓存：完整（离线可制作）"
        : "Qwen 旁白缓存：不完整（下次制作需 SSH 笔记本在线）",
    );
    return;
  }

  // 以下模式才允许创建运行期目录。
  mkdirSync(RUNTIME, { recursive: true });
  mkdirSync(NPM_CACHE, { recursive: true });
  const env = { ...process.env, VIDEO_PIPELINE: "1" };

  if (args.includes("--director")) {
    // 导演段开发入口：类型检查 + UI 构建 + 导演截图；不生成语音、不录制、不导出成片。
    const directorEnv = { ...env, VIDEO_DIRECTOR: "1" };
    console.log("[导演段] 类型检查");
    command(process.execPath, [
      "node_modules/typescript/bin/tsc", "-p", "video/tsconfig.json", "--noEmit",
    ], { env: directorEnv });
    console.log("[导演段] 构建舞台 UI");
    command(process.execPath, [
      "node_modules/vite/bin/vite.js", "build", "--config", "video/vite.config.ts",
    ], { env: directorEnv });
    console.log("[导演段] 截取导演时点画面");
    command(process.execPath, ["video/capture/director-stills.mjs"], { env: directorEnv });
    console.log(`完成：${REPORTS}`);
    return;
  }

  const lock = acquireLock();
  let lockOwned = true;
  let outputOwned = false;
  let delivered = false;
  let prepared = false;
  try {
    console.log("[1/7] 清理全部旧视频和旧版制作目录");
    cleanOldOutputs();
    outputOwned = true;
    console.log("[2/7] 检查制作环境");
    mkdirSync(AUDIO, { recursive: true });
    mkdirSync(REPORTS, { recursive: true });
    mkdirSync(OUTPUT, { recursive: true });
    const status = dependencyStatus();
    const missing = status.checks.filter(([, ready]) => !ready).map(([name]) => name);
    if (missing.length)
      throw new Error(
        `缺少必要程序/依赖：${missing.join("、")}。安装并加入 PATH（依赖缺失时在项目目录执行 npm ci）后重试，不自动安装。`,
      );
    const { chromium } = await import("playwright");
    if (!existsSync(chromium.executablePath()))
      throw new Error("Playwright Chromium 未安装：请先执行 npx playwright install chromium。");
    if (!narrationCacheComplete()) {
      const host = narrationConfig().ssh?.host;
      const reachable = host
        ? spawnSync("ssh", ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", host, "exit 0"],
            { cwd: ROOT, windowsHide: true, encoding: "utf8" }).status === 0
        : false;
      if (!reachable)
        throw new Error(
          `旁白缓存不完整且 SSH 笔记本（${host ?? "未配置"}）不可达：首次合成需要笔记本在线。`,
        );
    }
    console.log("[3/7] 生成/复用 Qwen 旁白并构建导演提示点与配乐");
    command(process.execPath, ["video/audio/qwen-narrate.mjs"], { env });
    command(process.execPath, ["video/audio/director-cues.mjs"], { env });
    command(process.execPath, ["video/audio/gen-audio.mjs"], { env });
    console.log("[4/7] 编译当前产品 UI");
    command(process.execPath, [
      "node_modules/typescript/bin/tsc", "-p", "video/tsconfig.json", "--noEmit",
    ], { env });
    command(process.execPath, [
      "node_modules/vite/bin/vite.js", "build", "--config", "video/vite.config.ts",
    ], { env });
    console.log("[5/7] 导演审计与混音");
    command(process.execPath, ["video/capture/director-audit.mjs"], { env });
    command(process.execPath, ["video/audio/mix-director.mjs"], { env });
    if (args.includes("--prepare")) {
      prepared = true;
      console.log("制作准备完成（含构建与混音），尚未导出视频。");
      return;
    }
    console.log("[6/7] 校验全片覆盖并逐帧录制");
    assertFullCoverage();
    command(process.execPath, [
      "video/capture/render-virtual.mjs",
      ...(args.includes("--preview") ? ["--draft"] : []),
    ], { env });
    console.log("[7/7] 核对交付目录");
    assertSingleVideoDirectory(true);
    delivered = true;
  } finally {
    try {
      if (outputOwned && !prepared) {
        for (const name of [".pending.mp4", ...[0, 1, 2, 3, 4, 5].map((i) => `.part-${i}.mp4`)]) {
          const path = resolve(OUTPUT, name);
          if (existsSync(path)) unlinkSync(path);
        }
        if (!delivered) {
          for (const name of ["sleepy-doll.mp4", "poster.jpg", "subtitles.zh-CN.srt", "manifest.json"]) {
            const path = resolve(OUTPUT, name);
            if (existsSync(path)) unlinkSync(path);
          }
        }
        cleanIntermediateOutputs();
      }
    } finally {
      if (
        lockOwned &&
        existsSync(lock) &&
        JSON.parse(readFileSync(lock, "utf8")).pid === process.pid
      )
        unlinkSync(lock);
    }
  }
  if (delivered) console.log(`完成：${FINAL_VIDEO}`);
}

main().catch((error) => {
  console.error(`\n制作停止：${error.message}`);
  process.exitCode = 1;
});
