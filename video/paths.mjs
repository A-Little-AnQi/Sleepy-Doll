// 所有交付视频只使用 output；缓存目录只保存非视频中间文件。
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const OUTPUT = resolve(ROOT, "video/output");
export const CACHE = resolve(ROOT, "target/video-build");
export const UI = resolve(CACHE, "ui");
export const AUDIO = resolve(CACHE, "audio");
export const REPORTS = resolve(CACHE, "reports");
// 本任务运行期工具临时目录：Python、FFmpeg、Vite、Playwright 的 TEMP/TMP/TMPDIR 都指向这里。
export const RUNTIME = resolve(CACHE, "runtime");
export const FINAL_VIDEO = resolve(OUTPUT, "sleepy-doll.mp4");
export const MEDIA_EXTENSIONS = new Set([
  ".mp4",
  ".mov",
  ".mkv",
  ".webm",
  ".avi",
  ".m4v",
]);
export const LEGACY_DIRECTORIES = [
  "video/dist",
  "video/out",
  "target/video-recording",
  "target/video-recut",
  "target/video-reel",
  "target/video-sample",
  "target/video-ui",
  "target/video-virtual",
];
export const LEGACY_AUDIO_FILES = [
  "video/audio/music.wav",
  "video/audio/sfx.wav",
  "video/audio/voice.wav",
];
export const LEGACY_VIDEO_FILES = [
  "target/promo/Sleepy-Doll-promo.mp4",
  ...["bridge", "chat", "endcard", "extensions", "intro", "models",
    "page@994a2c8da52cd2102a127b33f95e54fe", "page@a485ac33a33826dba89497eea2b1683a"]
    .map((name) => `target/promo/clips/${name}.webm`),
];
