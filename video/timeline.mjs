// 镜头独立安排动作与阅读时间，供画面、音效和检查共用。
/** @typedef {{expectedDurationMs?: number, cuts: Array<{name: string, from: number, to: number, duration: number}>}} Timeline */

/** @param {Timeline} recipe */
export function durationMs(recipe) {
  let end = 0;
  for (const cut of recipe.cuts) {
    if (!(cut.duration > 0 && cut.to >= cut.from && cut.from >= end)) {
      throw new Error(`镜头时间配置无效：${cut.name}`);
    }
    end = cut.to;
  }
  const total = recipe.cuts.reduce((sum, cut) => sum + cut.duration, 0);
  if (
    recipe.expectedDurationMs != null &&
    total !== recipe.expectedDurationMs
  ) {
    throw new Error(
      `成片时长不符：要求 ${recipe.expectedDurationMs}ms，当前 ${total}ms`,
    );
  }
  return total;
}

/** @param {number} playback @param {Timeline} recipe */
export function storyAt(playback, recipe) {
  let start = 0;
  for (const cut of recipe.cuts) {
    if (playback < start + cut.duration) {
      const k = Math.max(0, (playback - start) / cut.duration);
      return cut.from + (cut.to - cut.from) * k;
    }
    start += cut.duration;
  }
  return recipe.cuts.at(-1)?.to ?? 0;
}

/** @param {number} story @param {Timeline} recipe */
export function playbackAt(story, recipe) {
  let start = 0;
  for (const cut of recipe.cuts) {
    if (story <= cut.to) {
      const k = Math.max(
        0,
        (story - cut.from) / Math.max(1, cut.to - cut.from),
      );
      return start + cut.duration * k;
    }
    start += cut.duration;
  }
  return start;
}
