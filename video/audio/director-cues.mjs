// 由 manifest-qwen-ssh + director-narration 构建导演段组合元数据：
// 输出 CACHE/director-cues.json（句级/能量VAD起止，明确不是逐词对齐）
// 与 OUTPUT/subtitles.zh-CN.srt（按语音实际起止，不按整段窗口）。
// 不做 atempo、不裁语音；时长超窗只如实记录。
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const CACHE = resolve(ROOT, "target", "video-build");
const VOICE = resolve(CACHE, "audio", "voice");
const OUTPUT = resolve(ROOT, "video/output");
const RUNTIME = resolve(CACHE, "runtime");
for (const k of ["TEMP", "TMP", "TMPDIR"]) process.env[k] = RUNTIME;
mkdirSync(RUNTIME, { recursive: true });

const narration = JSON.parse(
  readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "director-narration.json"), "utf8"),
);
const manifest = JSON.parse(readFileSync(resolve(VOICE, "manifest-qwen-ssh.json"), "utf8"));
if (manifest.provider !== "qwen-ssh") throw new Error("manifest provider 不是 qwen-ssh");
if (manifest.failures?.length) throw new Error(`语音 manifest 存在失败段：${manifest.failures.map((f) => f.id).join(",")}`);
const byId = new Map(manifest.segments.map((s) => [s.id, s]));
const missing = narration.segments.filter((s) => !byId.has(s.id));
if (missing.length) throw new Error(`manifest 缺少段落：${missing.map((s) => s.id).join(",")}`);

const DURATION_MS = 60000;
const segments = [];
for (const seg of narration.segments) {
  const m = byId.get(seg.id);
  if (!m.speech || m.speech.onset_sec === null || m.speech.offset_sec === null)
    throw new Error(`段 ${seg.id} 缺少能量 VAD 起止，不能构建 cues`);
  const start = Math.round(seg.start * 1000);
  const speechStart = Math.round(seg.start * 1000 + m.speech.onset_sec * 1000);
  const speechEnd = Math.round(seg.start * 1000 + m.speech.offset_sec * 1000);
  if (speechEnd > DURATION_MS) throw new Error(`段 ${seg.id} 语音超出 60s：${speechEnd}ms`);
  const prev = segments[segments.length - 1];
  if (prev && speechStart < prev.speechEnd)
    throw new Error(`段 ${seg.id} 语音与 ${prev.id} 重叠`);
  segments.push({
    id: seg.id,
    text: seg.text,
    file: resolve(VOICE, m.file),
    start,
    speechStart,
    speechEnd,
    duration: Math.round(m.duration_sec * 1000),
    window: { start, end: Math.round(seg.end * 1000) },
    overflowMs: Math.max(0, Math.round(m.duration_sec * 1000) - Math.round((seg.end - seg.start) * 1000)),
  });
}

const cues = {
  version: 1,
  provider: "qwen-ssh",
  timingType: "sentence/energy-vad",
  wordTiming: false,
  durationMs: DURATION_MS,
  generatedAt: new Date().toISOString(),
  source: {
    narration: "video/audio/director-narration.json",
    manifest: "target/video-build/audio/voice/manifest-qwen-ssh.json",
    manifestGeneratedAt: manifest.generated_at,
  },
  note: "speechStart/End 为段落级声学起止（能量VAD），不是逐词对齐；语音未变速、未裁剪。",
  segments,
};
const cuesPath = resolve(CACHE, "director-cues.json");
writeFileSync(cuesPath, JSON.stringify(cues, null, 2) + "\n");

function stamp(ms) {
  return new Date(ms).toISOString().slice(11, 23).replace(".", ",");
}
mkdirSync(OUTPUT, { recursive: true });
writeFileSync(
  resolve(OUTPUT, "subtitles.zh-CN.srt"),
  segments
    .map((s, i) => `${i + 1}\n${stamp(s.speechStart)} --> ${stamp(s.speechEnd)}\n${s.text}\n`)
    .join("\n"),
);

const sha = createHash("sha256").update(readFileSync(cuesPath)).digest("hex");
const totalSpeech = segments.reduce((sum, s) => sum + (s.speechEnd - s.speechStart), 0);
const overflows = segments.filter((s) => s.overflowMs > 0);
console.log(`director-cues：${segments.length} 段 / ${DURATION_MS}ms / 语音合计 ${(totalSpeech / 1000).toFixed(2)}s`);
console.log(`sha256 ${sha.slice(0, 16)} · ${cuesPath}`);
console.log(`字幕：${resolve(OUTPUT, "subtitles.zh-CN.srt")}`);
if (overflows.length)
  console.log(`超窗报告（未裁剪）：${overflows.map((s) => `${s.id}+${s.overflowMs}ms`).join(" ")}`);
