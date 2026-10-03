// 仅清理明确列出的旧制作目录；遇到链接或越界路径时整体拒绝。
import {
  existsSync,
  lstatSync,
  readdirSync,
  realpathSync,
  rmSync,
  readFileSync,
} from "node:fs";
import { extname, isAbsolute, relative, resolve, sep } from "node:path";
import {
  ROOT,
  OUTPUT,
  CACHE,
  LEGACY_DIRECTORIES,
  LEGACY_AUDIO_FILES,
  LEGACY_VIDEO_FILES,
  MEDIA_EXTENSIONS,
  UI,
  AUDIO,
  REPORTS,
  RUNTIME,
} from "./paths.mjs";

export function contained(root, candidate) {
  const rel = relative(root, candidate);
  return (
    rel !== "" &&
    !isAbsolute(rel) &&
    rel !== ".." &&
    !rel.startsWith(`..${sep}`)
  );
}

export function inventory(directory) {
  if (!existsSync(directory)) return { files: 0, bytes: 0, videos: [] };
  const result = { files: 0, bytes: 0, videos: [] };
  const walk = (path) => {
    const stat = lstatSync(path);
    if (stat.isSymbolicLink())
      throw new Error(`清理范围内存在链接，已停止：${path}`);
    if (stat.isDirectory()) {
      for (const name of readdirSync(path)) walk(resolve(path, name));
    } else {
      result.files++;
      result.bytes += stat.size;
      if (MEDIA_EXTENSIONS.has(extname(path).toLowerCase()))
        result.videos.push(path);
    }
  };
  walk(directory);
  return result;
}

// 待选设计产物，用户未选定风格前必须保留。
export const DESIGN_ARTIFACTS = new Set([
  "design-01.png",
  "design-02.png",
  "design-03.png",
  "design-04.png",
  "design-05.png",
  "design-directions.json",
  "design-preview.html",
]);

export function cleanupPlan() {
  const physicalRoot = realpathSync(ROOT);
  const targets = [
    ...LEGACY_DIRECTORIES.map((path) => resolve(ROOT, path)),
    ...LEGACY_AUDIO_FILES.map((path) => resolve(ROOT, path)),
    ...LEGACY_VIDEO_FILES.map((path) => resolve(ROOT, path)),
    OUTPUT,
  ];
  return targets.map((path) => {
    if (!contained(ROOT, path)) throw new Error(`清理目标越界：${path}`);
    if (existsSync(path) && !contained(physicalRoot, realpathSync(path)))
      throw new Error(`实际清理路径越界：${path}`);
    if (path === OUTPUT && existsSync(path)) {
      const known = new Set(["sleepy-doll.mp4", "manifest.json", "poster.jpg", "subtitles.zh-CN.srt",
        ".pending.mp4", ...[0, 1, 2, 3, 4, 5].map((i) => `.part-${i}.mp4`),
        ...DESIGN_ARTIFACTS]);
      const unknown = readdirSync(path).filter((name) => !known.has(name));
      if (unknown.length) throw new Error(`输出目录存在未知文件，未执行清理：${unknown.join("、")}`);
    }
    return { path, ...inventory(path) };
  });
}

export function assertSingleVideoDirectory(requireFinal = false) {
  const videos = [
    ...inventory(resolve(ROOT, "video")).videos,
    ...inventory(CACHE).videos,
    ...inventory(resolve(ROOT, "target/promo")).videos,
  ];
  for (const path of LEGACY_DIRECTORIES.filter((path) =>
    path.startsWith("target/"),
  )) {
    videos.push(...inventory(resolve(ROOT, path)).videos);
  }
  const outside = videos.filter(
    (path) => relative(OUTPUT, path).includes(sep) || !contained(OUTPUT, path),
  );
  if (outside.length)
    throw new Error(`发现输出目录以外的视频，已停止：\n${outside.join("\n")}`);
  if (
    requireFinal &&
    (videos.length !== 1 || relative(OUTPUT, videos[0]) !== "sleepy-doll.mp4")
  ) {
    throw new Error(
      "制作结束后，video/output 必须只包含 sleepy-doll.mp4 一个视频文件。",
    );
  }
  return videos;
}

export function cleanOldOutputs() {
  const plan = cleanupPlan();
  // 先验证所有目标，再执行任何删除。
  for (const item of plan) {
    if (!existsSync(item.path)) continue;
    if (item.path === OUTPUT) {
      // 输出目录只删除逐个核实的已知媒体/清单文件，保留待选设计产物，不整体删除目录。
      let removed = 0;
      for (const name of readdirSync(OUTPUT)) {
        if (DESIGN_ARTIFACTS.has(name)) continue;
        const target = resolve(OUTPUT, name);
        if (lstatSync(target).isSymbolicLink())
          throw new Error(`输出目录存在链接，已停止：${target}`);
        rmSync(target, { force: false });
        removed++;
      }
      console.log(`已清理：${relative(ROOT, item.path)}（${removed} 个文件）`);
      continue;
    }
    rmSync(item.path, { recursive: true, force: false });
    console.log(`已清理：${relative(ROOT, item.path)}（${item.files} 个文件）`);
  }
  assertSingleVideoDirectory();
}

// 清理逐帧构建的临时资料；保留 Qwen 旁白缓存（nXX-hash.wav + manifest-qwen-ssh.json）。
export function cleanIntermediateOutputs() {
  if (existsSync(CACHE) && !contained(realpathSync(ROOT), realpathSync(CACHE)))
    throw new Error(`实际缓存路径越界：${CACHE}`);
  const targets = [
    UI, REPORTS, RUNTIME, resolve(CACHE, "concat.txt"),
    ...["music.wav", "sfx.wav", "mixed.wav", "mixed-director.wav", "master.m4a", "mastering.json"].map((name) => resolve(AUDIO, name)),
  ];
  for (const path of targets) {
    if (!contained(CACHE, path))
      throw new Error(`中间文件清理路径越界：${path}`);
    if (existsSync(path) && !contained(realpathSync(CACHE), realpathSync(path)))
      throw new Error(`实际中间文件清理路径越界：${path}`);
    inventory(path);
  }
  for (const path of targets) {
    if (existsSync(path)) rmSync(path, { recursive: true, force: false });
  }
  // Qwen 缓存完整时，按清单移除旧 Edge 管线的哈希 mp3/json、旧 manifest 与旧 cues.json。
  // 只匹配列出的模式；未知文件一律不动，target/promo、声源录音与用户数据不在范围。
  const voice = resolve(AUDIO, "voice");
  const qwenManifest = resolve(voice, "manifest-qwen-ssh.json");
  if (existsSync(qwenManifest)) {
    inventory(voice);
    const qwen = JSON.parse(readFileSync(qwenManifest, "utf8"));
    const complete =
      qwen.provider === "qwen-ssh" &&
      Array.isArray(qwen.segments) && qwen.segments.length === 12 &&
      !qwen.failures?.length;
    if (complete) {
      for (const name of readdirSync(voice)) {
        if (name === "manifest-qwen-ssh.json") continue;
        if (/^[a-z]\d{2}-[0-9a-f]{12}\.wav$/.test(name)) continue; // 当前 Qwen 12 段
        if (name === "manifest.json" || /^[0-9a-f]{20}\.(mp3|json|partial)$/.test(name))
          rmSync(resolve(voice, name), { force: false });
      }
      const legacyCues = resolve(CACHE, "cues.json");
      if (existsSync(legacyCues)) {
        if (!contained(CACHE, legacyCues)) throw new Error(`旧 cues 清理路径越界：${legacyCues}`);
        rmSync(legacyCues, { force: false });
      }
    }
  }
}
