// 将真实逐词提示点、旁白与音效合成，并执行两遍响度归一。
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { ROOT, AUDIO, CACHE, OUTPUT } from "../paths.mjs";

const recipe = JSON.parse(
  readFileSync(resolve(ROOT, "video/recording.json"), "utf8"),
);
const sheet = JSON.parse(readFileSync(resolve(CACHE, "cues.json"), "utf8"));
const clips = sheet.alignment;
const duration = sheet.durationMs / 1000;
const tones = recipe.audioEvents
  .map(([id, hz]) => {
    const cue = sheet.cues[id];
    if (!cue) throw new Error(`音效缺少旁白提示点：${id}`);
    const at = cue.wordAt / 1000;
    return `if(between(t,${at},${at + 0.12}),.24*sin(2*PI*${hz}*(t-${at}))*exp(-45*(t-${at})),0)`;
  })
  .join("+");
const inputs = [
  "-i",
  resolve(AUDIO, "music.wav"),
  ...clips.flatMap((clip) => ["-i", clip.file]),
  "-f",
  "lavfi",
  "-i",
  `aevalsrc='${tones}':s=48000:d=${duration}`,
];
const filters = [],
  voiceTracks = [];
clips.forEach((clip, i) => {
  const room = (clip.end - clip.start) / 1000 - 0.12;
  filters.push(
    `[${i + 1}:a]atempo=${clip.tempo.toFixed(8)},apad,atrim=0:${room},asetpts=PTS-STARTPTS,adelay=${clip.start}|${clip.start}[voice${i}]`,
  );
  voiceTracks.push(`[voice${i}]`);
});
filters.push(
  `${voiceTracks.join("")}amix=inputs=${clips.length}:normalize=0,loudnorm=I=-17:TP=-2:LRA=7,aresample=48000,asplit=2[voice][side]`,
);
filters.push(
  `[0:a]atrim=0:${duration},asetpts=PTS-STARTPTS,volume=.20,afade=t=out:st=${duration - 1.3}:d=1.3[music]`,
);
filters.push(
  "[music][side]sidechaincompress=threshold=.025:ratio=7:attack=8:release=220[ducked]",
);
filters.push(
  `[ducked][voice][${clips.length + 1}:a]amix=inputs=3:normalize=0,aresample=48000[mix]`,
);
const mixed = resolve(AUDIO, "mixed.wav"),
  master = resolve(AUDIO, "master.m4a");
execFileSync(
  "ffmpeg",
  [
    "-y",
    "-v",
    "error",
    ...inputs,
    "-filter_complex",
    filters.join(";"),
    "-map",
    "[mix]",
    "-t",
    String(duration),
    "-c:a",
    "pcm_s24le",
    mixed,
  ],
  { stdio: "inherit", windowsHide: true },
);
const measured = spawnSync(
  "ffmpeg",
  [
    "-hide_banner",
    "-i",
    mixed,
    "-af",
    "loudnorm=I=-14:TP=-2:LRA=7:print_format=json",
    "-f",
    "null",
    "-",
  ],
  { encoding: "utf8", windowsHide: true },
);
if (measured.status !== 0) throw new Error(measured.stderr);
const json = measured.stderr.match(/\{\s*"input_i"[\s\S]*?\}/)?.[0];
if (!json) throw new Error("无法测量混音响度");
const stats = JSON.parse(json);
if (!Number.isFinite(Number(stats.input_i)))
  throw new Error("旁白混音为空，停止制作");
const filter = `loudnorm=I=-14:TP=-2:LRA=7:measured_I=${stats.input_i}:measured_TP=${stats.input_tp}:measured_LRA=${stats.input_lra}:measured_thresh=${stats.input_thresh}:offset=${stats.target_offset}:linear=true,aresample=48000`;
execFileSync(
  "ffmpeg",
  [
    "-y",
    "-v",
    "error",
    "-i",
    mixed,
    "-af",
    filter,
    "-c:a",
    "aac",
    "-b:a",
    "256k",
    master,
  ],
  { stdio: "inherit", windowsHide: true },
);
mkdirSync(OUTPUT, { recursive: true });
const stamp = (ms) =>
  new Date(ms).toISOString().slice(11, 23).replace(".", ",");
writeFileSync(
  resolve(OUTPUT, "subtitles.zh-CN.srt"),
  clips
    .map(
      (clip, i) =>
        `${i + 1}\n${stamp(clip.start)} --> ${stamp(clip.end)}\n${clip.text}\n`,
    )
    .join("\n"),
);
writeFileSync(resolve(AUDIO, "mastering.json"), JSON.stringify(stats, null, 2));
console.log(`混音完成：${clips.length} 句旁白，逐词音效提示点，两遍响度归一。`);
