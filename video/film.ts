import fixture from "./fixtures/launch-film.json";
import type {
  Bootstrap,
  MessageInfo,
  TaskInfo,
  TaskSummary,
} from "../web/src/ipc/types";
import { PREVIEW_PERMISSION } from "../web/src/ipc/types";
import type { Plan } from "../web/src/session";
import { T } from "./manifest";
import { clamp01, seg } from "./clock";

/**
 * 把演示 fixture 沿时间轴折叠成产品在 t 时刻的真实状态。
 * 所有值都是 t 的纯函数：消息、计划、审批、任务、快捷任务列表。
 */

const fx = fixture;

export interface FilmSnapshot {
  page: "chat" | "tasks";
  conversationId?: string;
  welcome: boolean;
  welcomeFadeOut: number;
  conversationFadeIn: number;
  composerDraft: string;
  messages: MessageInfo[];
  phase?: string;
  seconds: number;
  task?: TaskInfo;
  plan?: Plan;
  planEnter: number;
  planShiftPx: number;
  approvalVisible: boolean;
  approvalEnter: number;
  approvalExit: number;
  saveVisible: boolean;
  saveDisabled: boolean;
  savedCardVisible: number;
  workflows: TaskSummary[];
  workflowBusy: boolean;
  toolLabels: Record<string, string>;
}

/** 词组输入：四组，按 2.4 秒窗口排开，组内 480 ms 打完，组间停 100 ms。 */
function typedText(t: number) {
  if (t < T.s02TypeStart) return "";
  if (t >= T.s02TypeEnd) return fx.userPrompt;
  const window = T.s02TypeEnd - T.s02TypeStart;
  const slot = window / fx.typingGroups.length;
  let text = "";
  for (let i = 0; i < fx.typingGroups.length; i += 1) {
    const group = fx.typingGroups[i]!;
    const local = clamp01((t - T.s02TypeStart - i * slot) / (slot - 100));
    if (local <= 0) break;
    text += group.slice(0, Math.ceil(group.length * local));
  }
  return text;
}

function stepOutcome(index: number, t: number): string | undefined {
  if (index === 0) {
    if (t < T.s03Step1Active) return undefined;
    return t < T.s03Step1Done ? "active" : "verifiedSucceeded";
  }
  if (index === 1) {
    if (t < T.s04Click + 100) return undefined;
    return t < T.s05RouteEnd ? "active" : "verifiedSucceeded";
  }
  // 第三步：S05 末进入运行，S06 39.2 完成；第二次运行走压缩时间线。
  if (t < T.s07ChatCut) {
    if (t < T.s05RouteEnd) return undefined;
    return t < T.s06SummaryIn ? "active" : "verifiedSucceeded";
  }
  const at = T.s07Run2Step;
  if (t < at[1]!) return undefined;
  if (t < at[2]!) return "active";
  return t < at[3]! ? undefined : "verifiedSucceeded";
}

function stepOutcomeRun2(index: number, t: number): string | undefined {
  const at = T.s07Run2Step;
  if (index === 0) return t >= at[0]! ? "verifiedSucceeded" : "active";
  if (index === 1) {
    if (t < at[1]!) return "active";
    return t < at[2]! ? "active" : "verifiedSucceeded";
  }
  if (t < at[2]!) return undefined;
  return t < at[3]! ? "active" : "verifiedSucceeded";
}

function planAt(t: number, run2 = false): Plan | undefined {
  if (t < T.s03PlanEnter) return undefined;
  const steps = fx.plan.steps.map((step, index) => ({
    ...step,
    outcome: run2 ? stepOutcomeRun2(index, t) : stepOutcome(index, t),
  }));
  return { goal: fx.plan.goal, steps };
}

/** 工具调用的出现与结果到达时间（毫秒）。 */
const CALLS = [
  {
    id: "c1",
    name: "bgi.gameStatus",
    args: {},
    start: T.s03Step1Active,
    done: T.s03Step1Done,
  },
  {
    id: "c2",
    name: "bgi.runRoute",
    args: { name: "每日委托路线" },
    start: T.s04Click + 100,
    done: T.s05RouteEnd,
  },
  {
    id: "c3",
    name: "bgi.verifyResult",
    args: {},
    start: T.s05RouteEnd,
    done: T.s06SummaryIn,
  },
];

const CALLS_RUN2 = [
  {
    id: "c4",
    name: "bgi.gameStatus",
    args: {},
    start: T.s07ChatCut,
    done: T.s07Run2Step[1]!,
  },
  {
    id: "c5",
    name: "bgi.runRoute",
    args: { name: "每日委托路线" },
    start: T.s07Run2Step[1]!,
    done: T.s07Run2Step[2]!,
  },
  {
    id: "c6",
    name: "bgi.verifyResult",
    args: {},
    start: T.s07Run2Step[2]!,
    done: T.s07Run2Step[3]!,
  },
];

function messagesAt(t: number): MessageInfo[] {
  if (t < T.s02UserMessage) return [];
  const stamp = "2026-09-21T19:30:10";
  const messages: MessageInfo[] = [
    { role: "user", content: fx.userPrompt, createdAt: stamp },
  ];
  const calls = t >= T.s07ChatCut ? [...CALLS, ...CALLS_RUN2] : CALLS;
  const active = calls.filter((call) => t >= call.start);
  if (active.length) {
    messages.push({
      role: "assistant",
      content: t >= T.s06SummaryIn ? fx.assistantSummary : "",
      createdAt: "2026-09-21T19:30:40",
      toolCalls: active.map((call) => ({
        id: call.id,
        name: call.name,
        arguments: call.args,
      })),
    });
    for (const call of active) {
      if (t >= call.done) {
        messages.push({
          role: "tool",
          content: '{"ok":true}',
          toolCallId: call.id,
        });
      }
    }
  }
  if (t >= T.s07Run2Succeeded) {
    messages.push({
      role: "assistant",
      content: fx.assistantSummaryShort,
      createdAt: "2026-09-21T19:32:20",
    });
  }
  return messages;
}

function taskAt(t: number): TaskInfo | undefined {
  if (t < T.s02UserMessage) return undefined;
  const base: TaskInfo = {
    id: "run-1",
    conversationId: fx.conversation.id,
    prompt: fx.userPrompt,
    state: "deciding",
    createdAt: "2026-09-21T19:30:10",
    updatedAt: "2026-09-21T19:30:10",
    contextTokens: 1284,
    contextWindow: fx.model.contextWindow,
    inputTokens: 96,
    outputTokens: 210,
    source: { kind: "agent" },
  };
  if (t >= T.s07RunClick) {
    return {
      ...base,
      id: "run-2",
      prompt: fx.workflow.name,
      createdAt: "2026-09-21T19:32:10",
      state:
        t >= T.s07Run2Succeeded
          ? "succeeded"
          : t >= T.s07ChatCut
            ? "executing"
            : "queued",
      source: {
        kind: "savedWorkflow",
        workflowId: fx.workflow.id,
        workflowRevision: 1,
      },
    };
  }
  let state: TaskInfo["state"] = "deciding";
  if (t >= T.s06SummaryIn) state = "succeeded";
  else if (t >= T.s05Merge) state = "verifying";
  else if (t >= T.s04Click + 100) state = "executing";
  else if (t >= T.s04ApprovalIn) state = "awaitingApproval";
  else if (t >= T.s03Step1Active) state = "executing";
  return { ...base, state };
}

const PHASES: Partial<Record<TaskInfo["state"], string>> = {
  deciding: "正在处理请求",
  executing: "正在执行",
  verifying: "正在核对结果",
};

function workflowAt(t: number): TaskSummary[] {
  if (t < T.s07MorphEnd - 600) return [];
  const stateLabel =
    t >= T.s07Run2Succeeded
      ? fx.workflow.stateLabelDone
      : t >= T.s07RunClick
        ? fx.workflow.stateLabelRunning
        : fx.workflow.stateLabelReady;
  return [
    {
      id: fx.workflow.id,
      name: fx.workflow.name,
      description: "运行 BetterGI 已有的每日委托路线",
      state: "readyVerified",
      stateLabel,
      actionLabel: fx.workflow.actionLabel,
      shortcut: {
        applicationName: "BetterGI",
        targetName: "每日委托路线",
        action: { tool: "bgi.runRoute", arguments: { name: "每日委托路线" } },
      },
      runnable: true,
      pinned: true,
      sourceConversationId: fx.conversation.id,
      sourceTitleSnapshot: fx.conversation.title,
      sourceDeleted: false,
      publishedRevision: 1,
      revision: 1,
      modelUsage: "none",
      zeroToken: true,
      nodeCount: 1,
      updatedAt: fx.workflow.updatedAt,
      lastRunId: null,
      issue: null,
    },
  ];
}

export function filmAt(t: number): FilmSnapshot {
  const secondRun = t >= T.s07ChatCut;
  const task = taskAt(t);
  const plan = planAt(t, secondRun);
  const page: FilmSnapshot["page"] =
    (t >= T.s07MorphEnd && t < T.s07ChatCut) || t >= T.s07Run2Succeeded
      ? "tasks"
      : "chat";
  const approvalVisible =
    t >= T.s04ApprovalIn && t < T.s04ApprovalOut + 800 && !secondRun;
  const phase =
    task && !approvalVisible && PHASES[task.state]
      ? PHASES[task.state]
      : undefined;
  return {
    page,
    conversationId: t >= T.s02UserMessage ? fx.conversation.id : undefined,
    welcome: t < T.s02UserMessage + 500,
    welcomeFadeOut: seg(t, T.s02UserMessage, T.s02UserMessage + 500),
    conversationFadeIn: seg(t, T.s02UserMessage + 200, T.s02UserMessage + 1100),
    composerDraft: t < T.s02UserMessage ? typedText(t) : "",
    messages: messagesAt(t),
    phase,
    seconds: task
      ? Math.max(
          1,
          Math.floor(
            (t - (secondRun ? T.s07ChatCut : T.s02UserMessage)) / 1000,
          ),
        )
      : 0,
    task,
    plan,
    planEnter: seg(t, T.s03PlanEnter, T.s03PlanEntered),
    planShiftPx: 20 * seg(t, T.s04PlanShift, T.s04ApprovalIn),
    approvalVisible,
    approvalEnter: seg(t, T.s04ApprovalIn, T.s04ApprovalSettled),
    approvalExit: seg(t, T.s04ApprovalOut, T.s04ApprovalOut + 800),
    saveVisible: t >= T.s06SummaryIn && t < T.s07MorphStart,
    saveDisabled: t >= T.s07SaveClick,
    savedCardVisible: seg(t, T.s07MorphStart + 300, T.s07MorphEnd),
    workflows: workflowAt(t),
    workflowBusy: t >= T.s07RunClick && t < T.s07Run2Succeeded,
    toolLabels: fx.toolLabels,
  };
}

/** AppShell 用的 bootstrap：一份干净的演示数据，不含任何地址与密钥。 */
export function bootstrapAt(t: number): Bootstrap {
  return {
    configPath: "",
    models: [
      {
        id: fx.model.id,
        name: fx.model.name,
        protocol: "openai",
        model: fx.model.model,
        baseUrl: "",

        active: true,
        contextWindow: fx.model.contextWindow,
      },
    ],
    skills: [],
    plugins: fx.bootstrap.plugins as Bootstrap["plugins"],
    tools: fx.tools as Bootstrap["tools"],
    conversations:
      t >= T.s02UserMessage
        ? [
            { ...fx.conversation, taskCount: 1 },
            { ...fx.historyConversation, taskCount: 2 },
          ]
        : [{ ...fx.historyConversation, taskCount: 2 }],
    tasks: t >= T.s07RunClick ? [taskAt(t)!] : [],
    strategies: [],
    workflows: workflowAt(t),
    operations: [],
    resources: [],
    diagnostics: [],
    notifications: [],
    permission: {
      ...PREVIEW_PERMISSION,
      mode: "askEach",
      label: "请求审批",
    },
    bridge: fx.bootstrap.bridge as Bootstrap["bridge"],
  };
}
