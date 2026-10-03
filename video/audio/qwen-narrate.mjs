#!/usr/bin/env node
// 导演解说旁白合成（Qwen3-TTS via 笔记本 SSH 回环）。
// 每段一个合成请求：文本 JSON 经 SSH stdin 交给远端一次性 Python 连接器（python -B，base64 内嵌，
// 远端不落盘），连接器 POST {synth_url}，WAV 以二进制经 SSH stdout 流回本机。
// 本机缓存固定在 target/video-build/audio/voice/；manifest 写 manifest-qwen-ssh.json，
// 不触碰旧 Edge 管线的 manifest.json 与 mp3 缓存。独立运行，未接入制作主入口。
//
// 用法: node video/audio/qwen-narrate.mjs [--only n01,n02] [--force] [--selftest]
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const NARRATION_PATH = path.join(SCRIPT_DIR, 'director-narration.json');
const VOICE_DIR = path.join(ROOT, 'target', 'video-build', 'audio', 'voice');
const RUNTIME_DIR = path.join(ROOT, 'target', 'video-build', 'runtime'); // 本任务 TEMP 区，产物以 .part 暂存
const MANIFEST_PATH = path.join(VOICE_DIR, 'manifest-qwen-ssh.json');
const SSH_TIMEOUT_MS = 600_000;

// 能量 VAD 参数：帧 30ms / 步进 10ms，阈值 = 噪底(第 8 百分位) + 12dB，下限 -55dBFS，
// 连续 4 帧才算语音。只给段落级声学起止，不是逐词对齐。
const VAD = { frameMs: 30, hopMs: 10, floorLiftDb: 12, absFloorDb: -55, minRunFrames: 4, floorPercentile: 8 };

// 远端连接器：stdin 收 {url,text,voice,language,seed}，stdout 出 WAV 二进制，
// stderr 出一行 "TTSBRIDGE {json}"（ASCII，含 X-TTS-Cache 结果）。失败非零退出。
const CONNECTOR_PY = `
import json, sys, urllib.error, urllib.request
try:
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')
except Exception:
    pass
try:
    d = json.loads(sys.stdin.buffer.read().decode('utf-8'))
    body = {'text': d['text'], 'language': d.get('language', 'chinese'),
            'seed': int(d.get('seed', 1234)), 'cache': bool(d.get('cache', True))}
    if d.get('voice'):
        body['voice'] = d['voice']
    req = urllib.request.Request(
        d['url'], data=json.dumps(body).encode('utf-8'),
        headers={'Content-Type': 'application/json; charset=utf-8'})
    with urllib.request.urlopen(req, timeout=580) as r:
        wav = r.read()
        sys.stderr.write('TTSBRIDGE ' + json.dumps(
            {'http_status': r.status, 'bytes': len(wav),
             'server_cache': r.headers.get('X-TTS-Cache')}) + '\\n')
    if not wav:
        sys.stderr.write('TTSBRIDGE ' + json.dumps({'error': 'empty response body'}) + '\\n')
        sys.exit(4)
    sys.stdout.buffer.write(wav)
    sys.stdout.buffer.flush()
except urllib.error.HTTPError as e:
    detail = ''
    try:
        detail = e.read(300).decode('utf-8', 'replace')
    except Exception:
        pass
    sys.stderr.write('TTSBRIDGE ' + json.dumps(
        {'http_status': e.code, 'reason': str(e.reason), 'detail': detail}) + '\\n')
    sys.exit(2)
except Exception as e:
    sys.stderr.write('TTSBRIDGE ' + json.dumps({'error': repr(e)}) + '\\n')
    sys.exit(3)
`;

function usage() {
  console.log(`用法: node video/audio/qwen-narrate.mjs [--only n01,n02] [--force] [--selftest]
  --only ids   只处理指定段（逗号分隔 id）
  --force      忽略本机缓存，强制重新合成
  --selftest   本地自检：WAV 解析 + 能量 VAD + ffprobe 往返，不发起 SSH`);
}

function parseArgs(argv) {
  const out = { only: null, force: false, selftest: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--only') out.only = String(argv[++i] ?? '').split(',').map((s) => s.trim()).filter(Boolean);
    else if (a === '--force') out.force = true;
    else if (a === '--selftest') out.selftest = true;
    else if (a === '--help' || a === '-h') { usage(); process.exit(0); }
    else { console.error(`未知参数: ${a}`); usage(); process.exit(2); }
  }
  return out;
}

function loadNarration() {
  const cfg = JSON.parse(fs.readFileSync(NARRATION_PATH, 'utf8'));
  if (!Array.isArray(cfg.segments) || cfg.segments.length === 0) throw new Error('director-narration.json 缺少 segments');
  const ids = new Set();
  for (const s of cfg.segments) {
    if (!s.id || typeof s.start !== 'number' || typeof s.end !== 'number' || typeof s.text !== 'string' || !(s.end > s.start)) {
      throw new Error(`segment 配置不完整: ${JSON.stringify(s)}`);
    }
    if (ids.has(s.id)) throw new Error(`segment id 重复: ${s.id}`);
    ids.add(s.id);
  }
  return cfg;
}

function requestKey(cfg, seg) {
  return createHash('sha256').update(JSON.stringify({
    text: seg.text, voice: cfg.voice, language: cfg.language, seed: cfg.seed,
  })).digest('hex');
}

function parseBridgeLine(stderr) {
  const m = stderr.match(/^TTSBRIDGE (\{.*\})\s*$/m);
  if (!m) return null;
  try { return JSON.parse(m[1]); } catch { return { raw: m[1] }; }
}

function sshSynth(cfg, seg) {
  const started = Date.now();
  const payload = JSON.stringify({
    url: cfg.ssh.synth_url, text: seg.text, voice: cfg.voice,
    language: cfg.language, seed: cfg.seed, cache: true,
  });
  const b64 = Buffer.from(CONNECTOR_PY, 'utf8').toString('base64');
  // base64 内嵌绕开 cmd/PowerShell 引号转义；-B 防止远端生成 .pyc。
  const remoteCmd = `${cfg.ssh.python} -B -c "import base64;exec(base64.b64decode('${b64}').decode('utf-8'))"`;
  return new Promise((resolve, reject) => {
    const child = spawn('ssh', ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', cfg.ssh.host, remoteCmd],
      { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const chunks = [];
    let stderr = '';
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`SSH 合成超时（>${SSH_TIMEOUT_MS / 1000}s），已终止段 ${seg.id}`));
    }, SSH_TIMEOUT_MS);
    child.stdout.on('data', (c) => chunks.push(c));
    child.stderr.on('data', (c) => { stderr += c.toString('utf8'); });
    child.on('error', (e) => { clearTimeout(timer); reject(new Error(`无法启动 ssh: ${e.message}`)); });
    child.on('close', (code) => {
      clearTimeout(timer);
      const bridge = parseBridgeLine(stderr);
      if (code !== 0) {
        const why = bridge ? JSON.stringify(bridge) : stderr.trim().split('\n').slice(-3).join(' | ');
        reject(new Error(`段 ${seg.id} SSH/远端失败（exit ${code}）: ${why}`));
        return;
      }
      resolve({ wav: Buffer.concat(chunks), bridge, wallClockMs: Date.now() - started });
    });
    child.stdin.on('error', () => {}); // 远端提前退出时 EPIPE 不该掩盖真实错误
    child.stdin.end(Buffer.from(payload, 'utf8'));
  });
}

function ascii(buf, off) { return String.fromCharCode(buf[off], buf[off + 1], buf[off + 2], buf[off + 3]); }

function validateWav(buf, what) {
  const fail = (msg) => { throw new Error(`无效 WAV（${what}）: ${msg}`); };
  if (buf.length < 44) fail(`仅 ${buf.length} 字节`);
  if (ascii(buf, 0) !== 'RIFF' || ascii(buf, 8) !== 'WAVE') fail('缺少 RIFF/WAVE 头');
  let off = 12;
  let fmt = null; let data = null;
  while (off + 8 <= buf.length) {
    const id = ascii(buf, off);
    const size = buf.readUInt32LE(off + 4);
    if (id === 'fmt ' && !fmt) {
      if (off + 8 + 16 > buf.length) fail('fmt 块截断');
      fmt = {
        audioFormat: buf.readUInt16LE(off + 8),
        channels: buf.readUInt16LE(off + 10),
        sampleRate: buf.readUInt32LE(off + 12),
        byteRate: buf.readUInt32LE(off + 16),
        bitsPerSample: buf.readUInt16LE(off + 22),
      };
    } else if (id === 'data' && !data) {
      data = { offset: off + 8, size };
    }
    off += 8 + size + (size % 2);
  }
  if (!fmt) fail('缺少 fmt 块');
  if (!data) fail('缺少 data 块');
  if (data.offset + data.size > buf.length) fail(`data 块不完整（声明 ${data.size}，实际剩 ${buf.length - data.offset}）`);
  if (fmt.channels < 1 || fmt.sampleRate < 8000 || fmt.sampleRate > 96000 || fmt.byteRate === 0) {
    fail(`异常格式参数 ${JSON.stringify(fmt)}`);
  }
  const durationSec = data.size / fmt.byteRate;
  if (durationSec < 0.1) fail(`时长异常 ${durationSec.toFixed(3)}s`);
  return { ...fmt, dataOffset: data.offset, dataSize: data.size, durationSec };
}

// 返回单声道 Float32Array（[-1,1]）；仅支持 PCM16 / float32，其余返回 null 走 ffmpeg。
function decodeMono(buf, fmt) {
  const n = Math.floor(fmt.dataSize / (fmt.bitsPerSample / 8) / fmt.channels);
  if (n < 1) return null;
  const raw = buf.subarray(fmt.dataOffset, fmt.dataOffset + fmt.dataSize);
  if ((fmt.audioFormat === 1 && fmt.bitsPerSample === 16) || (fmt.audioFormat === 3 && fmt.bitsPerSample === 32)) {
    const aligned = (raw.byteOffset % (fmt.bitsPerSample / 8) === 0);
    const view = aligned ? raw : Buffer.from(raw); // 对齐失败时复制一份
    const src = fmt.bitsPerSample === 16
      ? new Int16Array(view.buffer, view.byteOffset, n * fmt.channels)
      : new Float32Array(view.buffer, view.byteOffset, n * fmt.channels);
    if (fmt.channels === 1) {
      const outArr = new Float32Array(n);
      for (let i = 0; i < n; i++) outArr[i] = fmt.bitsPerSample === 16 ? src[i] / 32768 : src[i];
      return outArr;
    }
    const outArr = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      let acc = 0;
      for (let c = 0; c < fmt.channels; c++) {
        const v = src[i * fmt.channels + c];
        acc += fmt.bitsPerSample === 16 ? v / 32768 : v;
      }
      outArr[i] = acc / fmt.channels;
    }
    return outArr;
  }
  return null;
}

function ffmpegDecodeMono(file, sampleRate) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', ['-v', 'error', '-i', file, '-f', 's16le', '-acodec', 'pcm_s16le', '-ac', '1', '-ar', String(sampleRate), 'pipe:1'],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    const chunks = [];
    let err = '';
    child.on('error', (e) => reject(new Error(`无法启动 ffmpeg: ${e.message}`)));
    child.stderr.on('data', (c) => { err += c.toString('utf8'); });
    child.stdout.on('data', (c) => chunks.push(c));
    child.on('close', (code) => {
      if (code !== 0) { reject(new Error(`ffmpeg 解码失败: ${err.trim()}`)); return; }
      const all = Buffer.concat(chunks);
      const i16 = new Int16Array(all.buffer, all.byteOffset, all.length >> 1);
      const outArr = new Float32Array(i16.length);
      for (let i = 0; i < i16.length; i++) outArr[i] = i16[i] / 32768;
      resolve(outArr);
    });
  });
}

// 能量 VAD：返回段落级声学起止（秒）。无语音段返回 null。
function energyVad(samples, sampleRate) {
  const win = Math.round((VAD.frameMs / 1000) * sampleRate);
  const hop = Math.round((VAD.hopMs / 1000) * sampleRate);
  if (samples.length < win + hop) return null;
  const dbs = [];
  for (let p = 0; p + win <= samples.length; p += hop) {
    let acc = 0;
    for (let i = p; i < p + win; i++) acc += samples[i] * samples[i];
    dbs.push(20 * Math.log10(Math.sqrt(acc / win) + 1e-9));
  }
  const sorted = [...dbs].sort((a, b) => a - b);
  const floorDb = sorted[Math.min(sorted.length - 1, Math.floor((VAD.floorPercentile / 100) * sorted.length))];
  const thresholdDb = Math.max(floorDb + VAD.floorLiftDb, VAD.absFloorDb);
  const voiced = dbs.map((d) => d >= thresholdDb);
  let onsetFrame = -1; let offsetFrame = -1; let run = 0;
  for (let i = 0; i < voiced.length; i++) {
    run = voiced[i] ? run + 1 : 0;
    if (run >= VAD.minRunFrames) {
      if (onsetFrame < 0) onsetFrame = i - VAD.minRunFrames + 1;
      offsetFrame = i;
    }
  }
  if (onsetFrame < 0) return { onsetSec: null, offsetSec: null, noiseFloorDb: floorDb, thresholdDb, frames: dbs.length };
  const onsetSec = onsetFrame * hop / sampleRate;
  const offsetSec = Math.min((offsetFrame * hop + win) / sampleRate, samples.length / sampleRate);
  return { onsetSec, offsetSec, noiseFloorDb: floorDb, thresholdDb, frames: dbs.length };
}

function ffprobeFile(file) {
  return new Promise((resolve) => {
    const child = spawn('ffprobe', ['-v', 'error', '-print_format', 'json', '-show_format', '-show_streams', file],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    let out = ''; let err = '';
    child.on('error', () => resolve(null));
    child.stdout.on('data', (c) => { out += c.toString('utf8'); });
    child.stderr.on('data', (c) => { err += c.toString('utf8'); });
    child.on('close', (code) => {
      if (code !== 0) { resolve({ error: err.trim() || `ffprobe exit ${code}` }); return; }
      try {
        const j = JSON.parse(out);
        const st = (j.streams || []).find((s) => s.codec_type === 'audio') || {};
        resolve({
          durationSec: parseFloat(j.format?.duration ?? st.duration),
          formatName: j.format?.format_name,
          codecName: st.codec_name,
          sampleRate: st.sample_rate ? parseInt(st.sample_rate, 10) : undefined,
          channels: st.channels,
        });
      } catch (e) {
        resolve({ error: `ffprobe 输出解析失败: ${e.message}` });
      }
    });
  });
}

function codecName(fmt) {
  if (fmt.audioFormat === 1 && fmt.bitsPerSample === 16) return 'pcm_s16le';
  if (fmt.audioFormat === 1 && fmt.bitsPerSample === 24) return 'pcm_s24le';
  if (fmt.audioFormat === 1 && fmt.bitsPerSample === 32) return 'pcm_s32le';
  if (fmt.audioFormat === 3 && fmt.bitsPerSample === 32) return 'pcm_f32le';
  return `wav_tag_${fmt.audioFormat}_${fmt.bitsPerSample}bit`;
}

function buildSineWav() {
  const sr = 16000; // 静音 1s + 440Hz@-20dBFS 1s + 静音 1s
  const n = sr * 3;
  const pcm = Buffer.alloc(n * 2);
  for (let i = 0; i < n; i++) {
    const t = i / sr;
    const v = (t >= 1 && t < 2) ? Math.round(Math.sin(2 * Math.PI * 440 * t) * 0.1 * 32767) : 0;
    pcm.writeInt16LE(v, i * 2);
  }
  const h = Buffer.alloc(44);
  h.write('RIFF', 0); h.writeUInt32LE(36 + pcm.length, 4); h.write('WAVE', 8);
  h.write('fmt ', 12); h.writeUInt32LE(16, 16); h.writeUInt16LE(1, 20); h.writeUInt16LE(1, 22);
  h.writeUInt32LE(sr, 24); h.writeUInt32LE(sr * 2, 28); h.writeUInt16LE(2, 32); h.writeUInt16LE(16, 34);
  h.write('data', 36); h.writeUInt32LE(pcm.length, 40);
  return { buf: Buffer.concat([h, pcm]), sr };
}

async function selftest() {
  const { buf, sr } = buildSineWav();
  const fmt = validateWav(buf, 'selftest');
  const mono = decodeMono(buf, fmt);
  const vad = mono && energyVad(mono, sr);
  const okParse = Math.abs(fmt.durationSec - 3) < 1e-6 && fmt.sampleRate === sr && vad?.onsetSec !== null;
  const onsetOk = vad && Math.abs(vad.onsetSec - 1) < 0.06;
  const offsetOk = vad && Math.abs(vad.offsetSec - 2) < 0.11;
  const file = path.join(RUNTIME_DIR, 'selftest-qwen-narrate.wav');
  fs.writeFileSync(file, buf);
  const probe = await ffprobeFile(file);
  fs.rmSync(file, { force: true });
  const probeOk = probe && !probe.error && Math.abs(probe.durationSec - 3) < 0.02;
  console.log(`selftest WAV 解析: ${okParse ? 'PASS' : 'FAIL'}（duration=${fmt.durationSec.toFixed(3)}s rate=${fmt.sampleRate}）`);
  console.log(`selftest 能量 VAD: ${onsetOk && offsetOk ? 'PASS' : 'FAIL'}（onset=${vad?.onsetSec?.toFixed(3)}s offset=${vad?.offsetSec?.toFixed(3)}s 期望 1.0/2.0）`);
  console.log(`selftest ffprobe: ${probeOk ? 'PASS' : 'FAIL'}（${JSON.stringify(probe)}）`);
  if (!(okParse && onsetOk && offsetOk && probeOk)) process.exit(1);
}

async function processSegment(cfg, seg, force) {
  const reqHash = requestKey(cfg, seg);
  const file = path.join(VOICE_DIR, `${seg.id}-${reqHash.slice(0, 12)}.wav`);
  const windowDur = seg.end - seg.start;
  const result = {
    id: seg.id, text: seg.text,
    window: { start: seg.start, end: seg.end, duration: windowDur },
    request_sha256: reqHash,
  };

  let buf = null; let source; let serverCache = null; let wallClockMs = null;
  if (!force && fs.existsSync(file)) {
    buf = fs.readFileSync(file);
    try { validateWav(buf, `${seg.id} 缓存`); source = 'cache-reuse'; }
    catch (e) { console.warn(`[${seg.id}] 本机缓存无效，将重新合成: ${e.message}`); buf = null; }
  }
  if (!buf) {
    const r = await sshSynth(cfg, seg);
    validateWav(r.wav, `${seg.id} 远端响应`);
    const tmp = path.join(RUNTIME_DIR, `${seg.id}.part`);
    fs.writeFileSync(tmp, r.wav);
    fs.renameSync(tmp, file);
    buf = r.wav; source = 'remote-synth'; serverCache = r.bridge?.server_cache ?? null;
    wallClockMs = r.wallClockMs;
    console.log(`[${seg.id}] 远端合成完成: ${r.wav.length} 字节, server_cache=${serverCache}, ${r.wallClockMs}ms`);
  } else {
    console.log(`[${seg.id}] 命中本机缓存: ${file}`);
  }

  const fmt = validateWav(buf, seg.id);
  let mono = decodeMono(buf, fmt);
  if (!mono) {
    console.warn(`[${seg.id}] WAV 为 ${codecName(fmt)}，用 ffmpeg 解码做 VAD`);
    mono = await ffmpegDecodeMono(file, fmt.sampleRate);
  }
  const vad = energyVad(mono, fmt.sampleRate);
  if (!vad || vad.onsetSec === null) console.warn(`[${seg.id}] 能量 VAD 未检出语音（整段低于阈值）`);

  const probe = await ffprobeFile(file);
  const probeOk = probe && !probe.error;
  const probeMismatch = probeOk && Number.isFinite(probe.durationSec) && Math.abs(probe.durationSec - fmt.durationSec) > 0.02;
  if (probe?.error) console.warn(`[${seg.id}] ffprobe 失败: ${probe.error}`);
  if (probeMismatch) console.warn(`[${seg.id}] ffprobe 时长 ${probe.durationSec}s 与 WAV 头 ${fmt.durationSec}s 不一致`);

  const excess = fmt.durationSec - windowDur;
  const speechDur = vad?.onsetSec !== null && vad?.onsetSec !== undefined
    ? vad.offsetSec - vad.onsetSec : null;

  return {
    ...result,
    file: path.basename(file),
    path: file,
    bytes: buf.length,
    wav_sha256: createHash('sha256').update(buf).digest('hex'),
    format: { codec: codecName(fmt), sample_rate: fmt.sampleRate, channels: fmt.channels, bits: fmt.bitsPerSample },
    duration_sec: Number(fmt.durationSec.toFixed(3)),
    ffprobe: probeOk ? { duration_sec: Number(probe.durationSec.toFixed(3)), format: probe.formatName, codec: probe.codecName, mismatch: probeMismatch || undefined } : { error: probe?.error },
    speech: vad ? {
      onset_sec: vad.onsetSec === null ? null : Number(vad.onsetSec.toFixed(3)),
      offset_sec: vad.offsetSec === null ? null : Number(vad.offsetSec.toFixed(3)),
      head_silence_sec: vad.onsetSec === null ? null : Number(vad.onsetSec.toFixed(3)),
      tail_silence_sec: vad.offsetSec === null ? null : Number((fmt.durationSec - vad.offsetSec).toFixed(3)),
      duration_sec: speechDur === null ? null : Number(speechDur.toFixed(3)),
      noise_floor_db: Number(vad.noiseFloorDb.toFixed(1)),
      threshold_db: Number(vad.thresholdDb.toFixed(1)),
      method: `energy-vad frame=${VAD.frameMs}ms hop=${VAD.hopMs}ms floor=p${VAD.floorPercentile}+${VAD.floorLiftDb}dB`,
      note: '段落级声学起止（能量阈值），响应无逐词边界，不构成词对齐',
    } : null,
    source,
    server_cache: serverCache,
    wall_clock_ms: wallClockMs,
    window_fit: { ok: excess <= 0, excess_sec: excess > 0 ? Number(excess.toFixed(3)) : 0 },
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(VOICE_DIR, { recursive: true });
  fs.mkdirSync(RUNTIME_DIR, { recursive: true });
  for (const k of ['TEMP', 'TMP', 'TMPDIR']) process.env[k] = RUNTIME_DIR;

  if (args.selftest) { await selftest(); return; }

  // manifest 始终覆盖全部段落；--only 只限定允许强制重新合成的段，其余段一律走本机缓存。
  const cfg = loadNarration();
  const segs = cfg.segments;
  const forcedIds = new Set(args.force ? (args.only ?? segs.map((s) => s.id)) : (args.only ?? []));
  if (args.only) {
    const missing = [...forcedIds].filter((id) => !segs.some((s) => s.id === id));
    if (missing.length) throw new Error(`未知段 id: ${missing.join(', ')}`);
  }

  const results = []; const failures = [];
  for (const seg of segs) {
    try { results.push(await processSegment(cfg, seg, forcedIds.has(seg.id))); }
    catch (e) {
      failures.push({ id: seg.id, text: seg.text, error: e.message });
      console.error(`[${seg.id}] 失败: ${e.message}`);
    }
  }

  const manifest = {
    provider: 'qwen-ssh',
    tool: 'video/audio/qwen-narrate.mjs',
    narration: 'video/audio/director-narration.json',
    generated_at: new Date().toISOString(),
    run_scope: results.map((r) => r.id),
    voice_params: { voice: cfg.voice, language: cfg.language, seed: cfg.seed, server_cache_param: true },
    synth: { url: cfg.ssh.synth_url, ssh_host: cfg.ssh.host, remote_python: cfg.ssh.python },
    notes: [
      'wall_clock_ms 含 SSH 往返，且服务端可能有 5 分钟音频缓存命中（server_cache=hit），不作为推理速度依据',
      'speech 为能量 VAD 的段落级声学起止，响应无逐词边界，不构成词对齐',
      'window_fit 超窗只报告，供改文案/时序；音频未裁剪、未变速',
    ],
    segments: results,
    failures,
  };
  fs.writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2) + '\n');

  // manifest 已确认现行 12 段后，移除同命名方案下被替换的过期缓存（只匹配本脚本的 nXX-hash.wav）。
  if (failures.length === 0 && results.length === cfg.segments.length) {
    const active = new Set(results.map((r) => r.file));
    for (const name of fs.readdirSync(VOICE_DIR)) {
      if (!/^[a-z]\d{2}-[0-9a-f]{12}\.wav$/.test(name) || active.has(name)) continue;
      fs.unlinkSync(fs.join ? fs.join(VOICE_DIR, name) : path.join(VOICE_DIR, name));
      console.log(`已移除过期缓存: ${name}`);
    }
  }

  console.log('\n段ID  来源        服务端缓存  时长s   窗口s   适配          语音起止s      文件');
  for (const r of results) {
    const fit = r.window_fit.ok ? 'ok' : `超窗+${r.window_fit.excess_sec}s`;
    const span = r.speech && r.speech.onset_sec !== null ? `${r.speech.onset_sec}~${r.speech.offset_sec}` : 'n/a';
    console.log(`${r.id}  ${sourcePad(r.source)}  ${String(r.server_cache ?? '-').padEnd(4)}      ${String(r.duration_sec).padEnd(6)}  ${String(r.window.duration).padEnd(6)}  ${fit.padEnd(11)}  ${span.padEnd(12)}  ${r.file}`);
  }
  console.log(`\nmanifest: ${MANIFEST_PATH}`);
  if (failures.length) { console.error(`\n${failures.length} 段失败: ${failures.map((f) => f.id).join(', ')}`); process.exitCode = 1; }
}

function sourcePad(s) { return s === 'cache-reuse' ? 'cache-reuse' : 'remote-synth '; }

main().catch((e) => { console.error(`致命错误: ${e.message}`); process.exit(1); });
