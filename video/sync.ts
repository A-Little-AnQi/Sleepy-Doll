export interface CueSheet {
  version: number;
  durationMs: number;
  cues: Record<
    string,
    { at: number; wordAt: number; end: number; phrase: string; segment: number }
  >;
  marks: Array<{ at: number; story: number }>;
  alignment: Array<{ text: string; start: number; end: number;
    words: Array<{ text: string; start: number; end: number }> }>;
}

// 构建入口将实际旁白数据内联到页面，录制时不访问网络或墙钟。
const node = document.getElementById("video-cues");
if (!node?.textContent)
  throw new Error("缺少旁白提示点，请运行 make-video.bat。");
export const cueSheet = JSON.parse(node.textContent) as CueSheet;

export function cueTime(id: string) {
  const cue = cueSheet.cues[id];
  if (!cue) throw new Error(`旁白缺少动作提示点：${id}`);
  return cue.at;
}

export function syncedStoryAt(playback: number) {
  const marks = cueSheet.marks;
  for (let i = 1; i < marks.length; i++) {
    const a = marks[i - 1]!,
      b = marks[i]!;
    if (playback <= b.at) {
      const k = Math.max(0, Math.min(1, (playback - a.at) / (b.at - a.at)));
      return a.story + (b.story - a.story) * k;
    }
  }
  return marks[marks.length - 1]!.story;
}
