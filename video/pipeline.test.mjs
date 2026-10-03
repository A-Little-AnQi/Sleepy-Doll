// 纯逻辑检查；不删除文件、不合成语音、不启动录制。
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ROOT, OUTPUT, CACHE, UI, AUDIO, REPORTS, RUNTIME } from "./paths.mjs";
import { contained, cleanupPlan } from "./cleanup.mjs";

test("清理路径必须严格位于项目内", () => {
  assert.equal(contained(ROOT, ROOT), false);
  assert.equal(contained(ROOT, resolve(ROOT, "../another-project")), false);
  assert.equal(contained(ROOT, OUTPUT), true);
  const plan = cleanupPlan();
  assert.ok(plan.every((item) => contained(ROOT, item.path)));
  assert.ok(plan.some((item) => item.path === OUTPUT));
  assert.ok(!plan.some((item) => item.path === resolve(ROOT, "web")));
});

test("中间数据目录全部位于 target/video-build", () => {
  for (const path of [UI, AUDIO, REPORTS, RUNTIME])
    assert.ok(contained(CACHE, path));
});

test("Qwen 旁白缓存与导演叙事一致且不超窗", { skip: !existsSync(resolve(AUDIO, "voice/manifest-qwen-ssh.json")) }, () => {
  const narration = JSON.parse(
    readFileSync(resolve(ROOT, "video/audio/director-narration.json"), "utf8"),
  );
  const manifest = JSON.parse(
    readFileSync(resolve(AUDIO, "voice/manifest-qwen-ssh.json"), "utf8"),
  );
  assert.equal(manifest.provider, "qwen-ssh");
  assert.equal(manifest.failures.length, 0);
  const byId = new Map(manifest.segments.map((s) => [s.id, s]));
  let prevSpeechEnd = 0;
  for (const seg of narration.segments) {
    const m = byId.get(seg.id);
    assert.ok(m, `manifest 缺少 ${seg.id}`);
    assert.notEqual(m.speech?.onset_sec, null, `${seg.id} 缺能量 VAD 起止`);
    const speechStart = seg.start * 1000 + m.speech.onset_sec * 1000;
    const speechEnd = seg.start * 1000 + m.speech.offset_sec * 1000;
    assert.ok(speechStart >= prevSpeechEnd, `${seg.id} 语音重叠`);
    prevSpeechEnd = speechEnd;
    assert.ok(
      m.duration_sec <= seg.end - seg.start + 1e-6,
      `${seg.id} 超窗：${m.duration_sec}s > ${seg.end - seg.start}s`,
    );
  }
  assert.equal(narration.segments.at(-1).end, 60);
});

test("director-cues 为句级能量 VAD，不声称逐词对齐", { skip: !existsSync(resolve(CACHE, "director-cues.json")) }, () => {
  const cues = JSON.parse(readFileSync(resolve(CACHE, "director-cues.json"), "utf8"));
  assert.equal(cues.provider, "qwen-ssh");
  assert.equal(cues.wordTiming, false);
  assert.equal(cues.timingType, "sentence/energy-vad");
  assert.equal(cues.durationMs, 60000);
  assert.equal(cues.segments.length, 12);
  for (const seg of cues.segments) {
    assert.ok(seg.speechStart < seg.speechEnd, `${seg.id} 起止倒置`);
    assert.ok(seg.speechEnd <= 60000, `${seg.id} 语音超出 60s`);
  }
});

test("双击入口保持 ASCII，60 秒时长由导演叙事保证", () => {
  const launcher = readFileSync(resolve(ROOT, "make-video.bat"));
  assert.ok(launcher.every((value) => value < 128));
  const narration = JSON.parse(
    readFileSync(resolve(ROOT, "video/audio/director-narration.json"), "utf8"),
  );
  assert.equal(narration.segments.at(-1).end, 60);
  assert.equal(narration.segments.at(-1).start, 55);
});
