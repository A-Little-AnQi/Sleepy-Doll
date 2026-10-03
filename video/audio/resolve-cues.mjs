// 将实际语音的词边界转换为动作提示点；不猜测关键词时间。
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ROOT, AUDIO, CACHE } from "../paths.mjs";

const normalize = (text) => text.replace(/[^\p{L}\p{N}]/gu, "").toLowerCase();

export function phraseTime(words, phrase) {
  const needle = normalize(phrase);
  if (!needle) throw new Error("关键词不能为空");
  let text = "";
  const positions = [];
  for (const word of words) {
    const value = normalize(word.text);
    positions.push({ from: text.length, to: text.length + value.length, word });
    text += value;
  }
  const index = text.indexOf(needle);
  if (index < 0) throw new Error(`语音词边界中找不到“${phrase}”`);
  const start = positions.find((item) => item.from <= index && item.to > index);
  const end = positions.find(
    (item) =>
      item.from < index + needle.length && item.to >= index + needle.length,
  );
  if (!start || !end) throw new Error(`无法定位“${phrase}”的完整边界`);
  return { start: start.word.start, end: end.word.end };
}

export function resolveCueSheet(recipe, clips) {
  const alignment = clips.map((clip) => {
    const room = (clip.end - clip.start) / 1000 - 0.12;
    const tempo = Math.max(1, clip.duration / room);
    if (!(room > 0) || tempo > 1.25)
      throw new Error(`旁白超过镜头容量：${clip.text}`);
    if (!clip.words?.length) throw new Error(`旁白缺少逐词数据：${clip.text}`);
    return {
      ...clip,
      tempo,
      words: clip.words.map((word) => ({
        text: word.text,
        start: clip.start + word.start / tempo,
        end: clip.start + word.end / tempo,
      })),
    };
  });
  const cues = {};
  const marks = [{ at: 0, story: 0 }];
  for (const binding of recipe.cueBindings) {
    const clip = alignment[binding.segment];
    if (!clip) throw new Error(`关键词 ${binding.id} 引用了不存在的旁白`);
    const timing = phraseTime(clip.words, binding.phrase);
    const at = Math.round(timing.start + (binding.leadMs ?? 0));
    cues[binding.id] = {
      at,
      wordAt: Math.round(timing.start),
      end: Math.round(timing.end),
      phrase: binding.phrase,
      segment: binding.segment,
    };
    if (binding.story != null) marks.push({ at, story: binding.story });
  }
  // 段落收束点保持产品切换与 60 秒交付边界。
  marks.push(
    { at: 39950, story: 31600 },
    { at: 41950, story: 33700 },
    { at: 56450, story: 33700 },
    { at: 60000, story: 36000 },
  );
  for (let i = 1; i < marks.length; i++) {
    if (marks[i].at <= marks[i - 1].at || marks[i].story < marks[i - 1].story)
      throw new Error(`旁白动作顺序冲突：${JSON.stringify(marks[i])}`);
  }
  return { version: 1, durationMs: 60000, cues, marks, alignment };
}

export function playbackForStory(story, marks) {
  for (let i = 1; i < marks.length; i++) {
    const a = marks[i - 1],
      b = marks[i];
    if (story <= b.story)
      return (
        a.at +
        (b.at - a.at) *
          Math.max(0, (story - a.story) / Math.max(1, b.story - a.story))
      );
  }
  return marks.at(-1).at;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const recipe = JSON.parse(
    readFileSync(resolve(ROOT, "video/recording.json"), "utf8"),
  );
  const clips = JSON.parse(
    readFileSync(resolve(AUDIO, "voice/manifest.json"), "utf8"),
  );
  const sheet = resolveCueSheet(recipe, clips);
  writeFileSync(resolve(CACHE, "cues.json"), JSON.stringify(sheet, null, 2));
  console.log(`已对齐 ${Object.keys(sheet.cues).length} 个关键词与动作。`);
}
