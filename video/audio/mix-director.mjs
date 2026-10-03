// Qwen 旁白混音：12 段按导演窗口 start 用 adelay 放置（不 atempo、不裁语音），
// 语音总线 loudnorm -17 LUFS，音乐 volume .17 + 旁白侧链 duck，SFX volume .8，
// 三总线 amix 后两遍 loudnorm 到 -14 LUFS / TP -2，输出 AUDIO/master.m4a 与 mastering.json。
// 组合元数据来自 director-cues.json（句级/能量VAD），不读旧 cues.json。
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const CACHE = resolve(ROOT, "target", "video-build");
const AUDIO = resolve(CACHE, "audio");
const RUNTIME = resolve(CACHE, "runtime");
for (const k of ["TEMP", "TMP", "TMPDIR"]) process.env[k] = RUNTIME;
mkdirSync(RUNTIME, { recursive: true });
const ENV = { ...process.env };

const cues = JSON.parse(readFileSync(resolve(CACHE, "director-cues.json"), "utf8"));
if (cues.provider !== "qwen-ssh" || cues.durationMs !== 60000)
  throw new Error("director-cues.json 无效（provider/时长）");
const segs = cues.segments;
if (!segs.length) throw new Error("director-cues.json 没有段落");

// 输入序号：0=music 1=sfx 2..=旁白
const inputs = [
  "-i", resolve(AUDIO, "music.wav"),
  "-i", resolve(AUDIO, "sfx.wav"),
  ...segs.flatMap((s) => ["-i", s.file]),
];
const voiceTracks = segs.map((s, i) => `[voice${i}]`);
const filters = [];
segs.forEach((s, i) => {
  filters.push(
    `[${i + 2}:a]aresample=48000,adelay=${s.start}|${s.start}[voice${i}]`,
  );
});
filters.push(
  `${voiceTracks.join("")}amix=inputs=${segs.length}:normalize=0,` +
    "loudnorm=I=-17:TP=-2:LRA=7,aresample=48000,asplit=2[voice][side]",
);
filters.push("[0:a]atrim=0:60,asetpts=PTS-STARTPTS,volume=0.17[musicRaw]");
filters.push(
  "[musicRaw][side]sidechaincompress=threshold=0.025:ratio=7:attack=8:release=220[ducked]",
);
filters.push("[1:a]atrim=0:60,asetpts=PTS-STARTPTS,volume=0.8[sfx]");
filters.push(
  "[voice][ducked][sfx]amix=inputs=3:normalize=0,aformat=sample_fmts=s32:sample_rates=48000:channel_layouts=stereo[mix]",
);
const mixed = resolve(AUDIO, "mixed-director.wav");
execFileSync(
  "ffmpeg",
  ["-y", "-v", "error", ...inputs, "-filter_complex", filters.join(";"),
    "-map", "[mix]", "-t", "60", "-c:a", "pcm_s24le", mixed],
  { stdio: "inherit", windowsHide: true, env: ENV },
);

const measure = spawnSync(
  "ffmpeg",
  ["-hide_banner", "-i", mixed, "-af", "loudnorm=I=-14:TP=-2:LRA=7:print_format=json", "-f", "null", "-"],
  { encoding: "utf8", windowsHide: true, env: ENV },
);
const json = measure.stderr.match(/\{\s*"input_i"[\s\S]*?\}/)?.[0];
if (measure.status !== 0 || !json) throw new Error("无法测量混音响度");
const stats = JSON.parse(json);
if (!Number.isFinite(Number(stats.input_i))) throw new Error("混音为空，停止");

const secondPass =
  `loudnorm=I=-14:TP=-2:LRA=7:measured_I=${stats.input_i}:measured_TP=${stats.input_tp}` +
  `:measured_LRA=${stats.input_lra}:measured_thresh=${stats.input_thresh}` +
  `:offset=${stats.target_offset}:linear=true,aresample=48000`;
const master = resolve(AUDIO, "master.m4a");
execFileSync(
  "ffmpeg",
  ["-y", "-v", "error", "-i", mixed, "-af", secondPass, "-t", "60",
    "-c:a", "aac", "-b:a", "256k", master],
  { stdio: "inherit", windowsHide: true, env: ENV },
);

const verify = spawnSync(
  "ffmpeg",
  ["-hide_banner", "-i", master, "-af", "loudnorm=I=-14:TP=-2:LRA=7:print_format=json", "-f", "null", "-"],
  { encoding: "utf8", windowsHide: true, env: ENV },
);
const vjson = verify.stderr.match(/\{\s*"input_i"[\s\S]*?\}/)?.[0];
if (verify.status !== 0 || !vjson) throw new Error("无法复核 master 响度");
const finalStats = JSON.parse(vjson);
if (Math.abs(Number(finalStats.input_i) + 14) > 0.5)
  throw new Error(`master 未达标：${vjson}`);

writeFileSync(
  resolve(AUDIO, "mastering.json"),
  JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      layout: { voiceLoudnorm: "I=-17:TP=-2:LRA=7", musicVolume: 0.17,
        duck: "sidechaincompress threshold=.025 ratio=7 attack=8 release=220", sfxVolume: 0.8 },
      segments: segs.length,
      measured: stats,
      final: finalStats,
      outputs: { mixed, master },
    },
    null, 2,
  ) + "\n",
);
console.log(
  `master 完成：${segs.length} 段旁白，两遍 -14 LUFS；实测 integrated ${finalStats.input_i} LUFS / TP ${finalStats.input_tp} dBTP`,
);
