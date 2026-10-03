/**
 * 音乐与 UI 音效合成（无第三方依赖，直接写 48kHz 16bit WAV）。
 * 108 BPM 轻律动；结构跟导演时间表：0.4s 暖电钢感和弦+稀疏低脉冲 → 4s 软鼓进
 * → 17s 交接加节拍 → 28s 归档半拍空白（28..28.28）→ 34s 运行加琶音层
 * → 40-50s 轻音色 → 50s 切出留连接音 → 55s 末和弦 → 58.9..59.8 淡出。
 * 输出到 target/video-build/audio（music.wav / sfx.wav，均约 60s）。
 */
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { AUDIO } from "../paths.mjs";

let randomState = 20260922;
const random = () => {
  randomState = (Math.imul(randomState, 1664525) + 1013904223) >>> 0;
  return randomState / 4294967296;
};
const SR = 48000;
const DUR = 60.5;
const N = Math.floor(SR * DUR);
const BPM = 108;
const BEAT = 60 / BPM;

const musicL = new Float64Array(N);
const musicR = new Float64Array(N);
const sfxL = new Float64Array(N);
const sfxR = new Float64Array(N);

const add = (buf, at, value) => {
  const i = Math.floor(at * SR);
  if (i >= 0 && i < buf.length) buf[i] += value;
};

const sec = (ms) => ms / 1000;

// ---------- 基础音色 ----------

/** 低鼓：150→48Hz 扫频 + 指数衰减。 */
function kick(L, R, at, gain = 0.9) {
  const len = 0.24;
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const env = Math.exp(-t * 26);
    const freq = 150 * Math.exp(-t * 18) + 48;
    const sample = Math.sin(2 * Math.PI * freq * t) * env * gain;
    add(L, at + t, sample);
    add(R, at + t, sample);
  }
}

/** 闭镲：高通噪声短促衰减。 */
function hat(L, R, at, gain = 0.16) {
  let noise = 0;
  const len = 0.05;
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const white = random() * 2 - 1;
    noise = 0.6 * noise + 0.4 * white; // 高通近似
    const sample = (white - noise) * Math.exp(-t * 90) * gain;
    add(L, at + t, sample);
    add(R, at + t, sample * 0.9);
  }
}

/** 低音：正弦 + 轻微三角泛音。 */
function bass(L, R, at, freq, len = BEAT * 0.9, gain = 0.24) {
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const env = Math.min(1, t * 60) * Math.exp(-t * 3.2);
    const sample =
      (Math.sin(2 * Math.PI * freq * t) +
        0.18 * Math.sin(2 * Math.PI * freq * 2 * t)) *
      env *
      gain;
    add(L, at + t, sample);
    add(R, at + t, sample);
  }
}

/** 长音铺底：多失谐正弦，慢起慢收。 */
function pad(L, R, at, freqs, len, gain = 0.05, attack = 1.2) {
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const env =
      Math.min(1, t / attack) * Math.min(1, Math.max(0, (len - t) / 1.6));
    freqs.forEach((f, index) => {
      const detune = 1 + (index % 2 === 0 ? 0.0012 : -0.0014);
      const sample =
        Math.sin(2 * Math.PI * f * detune * t + index * 1.7) *
        env *
        gain *
        (1 / freqs.length) *
        2.2;
      add(index % 2 === 0 ? L : R, at + t, sample);
    });
  }
}

/** 短促确认音。 */
function blip(L, R, at, freq, len = 0.12, gain = 0.3) {
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const env = Math.exp(-t * 34);
    const sample = Math.sin(2 * Math.PI * freq * t) * env * gain;
    add(L, at + t, sample);
    add(R, at + t, sample);
  }
}

/** 完成和弦（全片唯一一次，≤700ms）。 */
function chord(L, R, at, freqs, len = 0.7, gain = 0.2) {
  pad(L, R, at, freqs, len + 0.35, gain, 0.02);
  freqs.forEach((f) => blip(L, R, at, f, len, gain * 0.6));
}

/** 点击：非常短的中频点。 */
function click(L, R, at, gain = 0.22) {
  for (let i = 0; i < 0.03 * SR; i += 1) {
    const t = i / SR;
    const env = Math.exp(-t * 220);
    const sample = Math.sin(2 * Math.PI * 1400 * t) * env * gain;
    add(L, at + t, sample);
    add(R, at + t, sample * 0.92);
  }
}

/** 柔和气流（转场用）。 */
function woosh(L, R, at, len = 0.3, gain = 0.06) {
  let noise = 0;
  for (let i = 0; i < len * SR; i += 1) {
    const t = i / SR;
    const white = random() * 2 - 1;
    noise = 0.96 * noise + 0.04 * white; // 低通
    const env = Math.sin(Math.PI * (t / len)) ** 2;
    const sample = noise * env * gain;
    add(L, at + t, sample);
    add(R, at + t, sample * 0.95);
  }
}

// ---------- 音乐结构（108 BPM，网格起点 0.4s）----------

const A1 = 55;
const A2 = 110;
const E2 = 82.41;
const C3 = 130.81;
const B2 = 123.47;
const G2 = 98;

const A0 = 0.4;
const bt = (k) => A0 + k * BEAT;
const kAt = (t) => Math.round((t - A0) / BEAT);
const CHORDS = [
  [110, 220, 261.63, 329.63], // Am
  [87.31, 174.61, 261.63, 349.23], // F
  [130.81, 196, 261.63, 392], // C
  [98, 196, 246.94, 293.66], // G
];

// 开场：0.4s 暖电钢感和弦 + 稀疏低脉冲（软鼓进入前只留呼吸感）。
pad(musicL, musicR, A0, CHORDS[0], 3.4, 0.05, 0.5);
for (let k = 0; bt(k) < 4; k += 2) kick(musicL, musicR, bt(k), 0.3);

// 4s（第 7 拍 ≈4.29s）软鼓进入；17s 交接起加力度（更强低鼓 + 八分镲 + 低音）。
const PUSH = kAt(17), LAYER = kAt(34), SOFT = kAt(40);
for (let k = 7; bt(k) < 40; k += 1) {
  const t = bt(k);
  if (t >= 28 && t < 28.4) continue; // 归档半拍空白区间不打
  const pushed = k >= PUSH;
  kick(musicL, musicR, t, pushed ? 0.58 : 0.45);
  bass(musicL, musicR, t, k % 8 < 6 ? A1 : G2, BEAT * 0.9, pushed ? 0.26 : 0.16);
  if (pushed) hat(musicL, musicR, t + BEAT / 2, 0.12);
  if (k >= LAYER) {
    // 34s 运行段：八分弱音琶音层。
    const arp = [440, 523.25, 659.25, 587.33];
    blip(musicL, musicR, t + BEAT / 2, arp[(k - LAYER) % 4], 0.16, 0.07);
    hat(musicL, musicR, t, 0.1);
  }
}
for (let k = 7, bar = 0; bt(k) < SOFT; k += 4, bar += 1)
  pad(musicL, musicR, bt(k), CHORDS[bar % 4], BEAT * 4 + 0.4, bt(k) >= 17 ? 0.045 : 0.038);

// 40-50s 配置段：轻音色，撤鼓留软铺底与稀疏镲。
for (let t = 40, bar = 0; t < 50; t += BEAT * 4, bar += 1)
  pad(musicL, musicR, t, CHORDS[bar % 4], BEAT * 4 + 0.4, 0.026);
for (let k = kAt(40); bt(k) < 50; k += 4) hat(musicL, musicR, bt(k), 0.06);

// 50s 切出：断开感之后只留一声连接音，随后静默到末和弦。
blip(musicL, musicR, 50.3, 660, 0.22, 0.1);
blip(musicL, musicR, 50.42, 880, 0.18, 0.07);

// 55s 末和弦，长尾收束。
chord(musicL, musicR, 55, [110, 220, 329.63, 440], 0.9, 0.18);
pad(musicL, musicR, 55, [110, 220, 329.63], 3.6, 0.05, 0.6);

// 28..28.28s 归档半拍空白：15ms 斜坡进出的静音缺口。
function gap(L, R, from = 28, to = 28.28, ramp = 0.015) {
  const a = Math.floor(from * SR), b = Math.floor((from + ramp) * SR);
  const c = Math.floor((to - ramp) * SR), d = Math.floor(to * SR);
  for (let i = a; i < d && i < N; i += 1) {
    const k = i < b ? (i - a) / (b - a) : i > c ? (d - i) / (d - c) : 0;
    L[i] *= k;
    R[i] *= k;
  }
}
gap(musicL, musicR);

// 全局淡出（58.9–59.8），片尾黑场静音。
function fadeOut(L, R, from = 58.9, to = 59.8) {
  const a = Math.floor(from * SR);
  const b = Math.floor(to * SR);
  for (let i = a; i < b && i < N; i += 1) {
    const k = 1 - (i - a) / (b - a);
    L[i] *= k;
    R[i] *= k;
  }
  for (let i = b; i < N; i += 1) {
    L[i] = 0;
    R[i] = 0;
  }
}
fadeOut(musicL, musicR);
fadeOut(sfxL, sfxR);

// 母线余量：音乐峰值留约 -3dB，混音叠加后由 loudnorm 统一到 -14 LUFS。
const MUSIC_GAIN = 0.5;
const SFX_GAIN = 0.7;
for (let i = 0; i < N; i += 1) {
  musicL[i] *= MUSIC_GAIN;
  musicR[i] *= MUSIC_GAIN;
  sfxL[i] *= SFX_GAIN;
  sfxR[i] *= SFX_GAIN;
}

// ---------- UI 音效（时间轴跟导演时间表；click 均 <80ms，动作用低通 whoosh/低频，不尖锐）----------

click(sfxL, sfxR, 5.85); // 发送
for (const at of [8.65, 8.85, 9.05]) blip(sfxL, sfxR, at, 1500, 0.05, 0.07); // 三步计划轻确认

kick(sfxL, sfxR, 14.2, 0.45); // 允许：稳重短促低频
blip(sfxL, sfxR, 14.2, 220, 0.1, 0.12);

woosh(sfxL, sfxR, 18.6, 0.35, 0.06); // 交接：门/移交低通气流 → 落定短点
click(sfxL, sfxR, 19.0, 0.18);

blip(sfxL, sfxR, 23.8, 660, 0.08, 0.12); // 回执：两声上行
blip(sfxL, sfxR, 23.92, 880, 0.08, 0.1);

click(sfxL, sfxR, 30.0, 0.18); // 保存：归档短促
blip(sfxL, sfxR, 30.05, 440, 0.12, 0.1);

kick(sfxL, sfxR, 34.9, 0.5); // 运行：低频触发 + 上行音
blip(sfxL, sfxR, 34.95, 330, 0.09, 0.1);
blip(sfxL, sfxR, 35.04, 495, 0.09, 0.08);

blip(sfxL, sfxR, 42.65, 1200, 0.05, 0.08); // 模型选择：轻点

click(sfxL, sfxR, 46.2, 0.12); // 插件两枚轻点
click(sfxL, sfxR, 47.0, 0.12);

blip(sfxL, sfxR, 51.0, 180, 0.25, 0.15); // 本地桥：低金属双音（轻微失谐）
blip(sfxL, sfxR, 51.02, 185, 0.25, 0.1);

chord(sfxL, sfxR, 56.1, [440, 523.25, 659.25], 0.5, 0.14); // 品牌：收束短和弦

// ---------- 写 WAV ----------

import { writeFileSync } from "node:fs";

function writeWav(L, R) {
  // 峰值归一到 -0.7dB：叠加超出时整体缩放，杜绝削波。
  let peak = 0;
  for (let i = 0; i < N; i += 1) {
    peak = Math.max(peak, Math.abs(L[i]), Math.abs(R[i]));
  }
  const norm = peak > 0.92 ? 0.92 / peak : 1;
  const bytes = 44 + N * 4;
  const buffer = Buffer.alloc(bytes);
  buffer.write("RIFF", 0);
  buffer.writeUInt32LE(bytes - 8, 4);
  buffer.write("WAVE", 8);
  buffer.write("fmt ", 12);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20); // PCM
  buffer.writeUInt16LE(2, 22); // 立体声
  buffer.writeUInt32LE(SR, 24);
  buffer.writeUInt32LE(SR * 4, 28);
  buffer.writeUInt16LE(4, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write("data", 36);
  buffer.writeUInt32LE(N * 4, 40);
  for (let i = 0; i < N; i += 1) {
    const l = Math.max(-1, Math.min(1, L[i] * norm));
    const r = Math.max(-1, Math.min(1, R[i] * norm));
    buffer.writeInt16LE(Math.round(l * 32767), 44 + i * 4);
    buffer.writeInt16LE(Math.round(r * 32767), 46 + i * 4);
  }
  return buffer;
}

const dir = AUDIO;
mkdirSync(dir, { recursive: true });
writeFileSync(resolve(dir, "music.wav"), writeWav(musicL, musicR));
writeFileSync(resolve(dir, "sfx.wav"), writeWav(sfxL, sfxR));
console.log("音乐与音效已生成到统一缓存目录。");
