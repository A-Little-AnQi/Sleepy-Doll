/**
 * 60 秒时间线与全片文案。时间单位毫秒，与逐镜脚本一一对应。
 * 产品内容（对话、计划、任务卡）在 fixtures/launch-film.json。
 */

export const FILM = {
  width: 2560,
  height: 1440,
  fps: 60,
  durationMs: 60_000,
} as const;

export interface SceneSpan {
  id: string;
  start: number;
  end: number;
}

export const SCENES: SceneSpan[] = [
  { id: "S01", start: 0, end: 5500 },
  { id: "S02", start: 5500, end: 12000 },
  { id: "S03", start: 12000, end: 18000 },
  { id: "S04", start: 18000, end: 24000 },
  { id: "S05", start: 24000, end: 37000 },
  { id: "S06", start: 37000, end: 44500 },
  { id: "S07", start: 44500, end: 53500 },
  { id: "S08", start: 53500, end: 60000 },
];

/** 关键时间点（毫秒），逐镜脚本的逐帧动作落在这里。 */
export const T = {
  // S01 重复操作
  s01Loop1Start: 300,
  s01Loop1Click: 850,
  s01Loop1End: 1800,
  s01Loop2Start: 1800,
  s01Loop2Click: 2590,
  s01Loop2End: 3000,
  s01Loop3Start: 3000,
  s01Loop3Halt: 3980,
  s01DimStart: 4200,
  s01DimEnd: 5100,
  s01ZoomStart: 5100,
  s01Cut: 5400,
  // S02 输入目标
  s02WindowSettle: 6300,
  s02PushEnd: 7060,
  s02TypeStart: 7000,
  s02TypeEnd: 9400,
  s02SendPress: 10000,
  s02UserMessage: 10100,
  s02Planning: 11200,
  // S03 步骤生成
  s03PlanEnter: 12000,
  s03PlanEntered: 12700,
  s03StepAt: [12700, 13060, 13450],
  s03Step1Active: 14400,
  s03Step1Done: 15500,
  s03Caption: 16400,
  s03CameraDown: 17500,
  // S04 确认
  s04PlanShift: 18000,
  s04ApprovalIn: 18800,
  s04ApprovalSettled: 19500,
  s04Caption: 20800,
  s04CursorMove: 21800,
  s04Click: 22400,
  s04ApprovalOut: 22500,
  s04Split: 23300,
  // S05 执行
  s05Settled: 25000,
  s05Status1Done: 25000,
  s05RouteStart: 27000,
  s05Caption: 29000,
  s05CaptionOut: 31000,
  s05RouteEnd: 31500,
  s05BgiDim: 34500,
  s05Merge: 36000,
  // S06 检查结果
  s06Step3Done: 38400,
  s06SummaryIn: 39200,
  s06Caption: 40000,
  s06Push: 41600,
  s06Pan: 43000,
  // S07 保存复用
  s07SaveClick: 44500,
  s07MorphStart: 45100,
  s07MorphEnd: 46200,
  s07Caption: 47000,
  s07TimeJump: 48200,
  s07CursorMove: 48300,
  s07RunClick: 49300,
  s07ChatCut: 50100,
  s07Run2Step: [50100, 50400, 51000, 52200],
  s07Run2Succeeded: 52200,
  s07Dim: 53000,
  // S08 品牌
  s08Dim: 53500,
  s08DimSettled: 54300,
  s08Moon: 54300,
  s08Name: 55000,
  s08Slogan: 55700,
  s08Hold: 56800,
  s08FadeOut: 59000,
  s08Black: 59800,
} as const;

/** 场外字幕：一屏只留一个主信息，整行遮罩揭示。 */
export interface CaptionSpec {
  text: string;
  start: number;
  end: number;
  pos: "left-bottom" | "right" | "top";
  size: 72 | 54;
}

export const CAPTIONS: CaptionSpec[] = [
  {
    text: "重复操作，不必重来",
    start: T.s01DimStart,
    end: T.s01Cut,
    pos: "left-bottom",
    size: 54,
  },
  {
    text: "输入目标",
    start: 7400,
    end: 10000,
    pos: "right",
    size: 72,
  },
  {
    text: "步骤自动生成",
    start: T.s03Caption,
    end: 18000,
    pos: "right",
    size: 72,
  },
  {
    text: "关键操作，由你确认",
    start: T.s04Caption,
    end: 24000,
    pos: "right",
    size: 72,
  },
  {
    text: "执行进度，随时可见",
    start: T.s05Caption,
    end: T.s05CaptionOut,
    pos: "top",
    size: 54,
  },
  {
    text: "完成后，检查结果",
    start: T.s06Caption,
    end: 43800,
    pos: "right",
    size: 72,
  },
  {
    text: "保存一次，下次直接运行",
    start: T.s07Caption,
    end: 53500,
    pos: "right",
    size: 54,
  },
];

/** 旁白轨（首版不配音，仅出 srt 与后续录音对齐用）。 */
export const VOICEOVER: Array<{ start: number; end: number; text: string }> = [
  { start: 7000, end: 10800, text: "输入目标，步骤自动生成。" },
  { start: 19500, end: 23300, text: "关键操作，由你确认。" },
  { start: 27000, end: 31000, text: "执行进度，随时可见。" },
  { start: 39200, end: 43800, text: "完成后，检查结果。" },
  { start: 47000, end: 51800, text: "保存一次，下次直接运行。" },
  { start: 55000, end: 59200, text: "Sleepy Doll。用一句话，运行 BetterGI。" },
];

export const BRAND = {
  name: "Sleepy Doll",
  slogan: "用一句话，运行 BetterGI。",
} as const;
