import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import type { Bootstrap, MessageInfo, TaskInfo, TaskSummary } from "../../web/src/ipc/types";
import { PREVIEW_PERMISSION } from "../../web/src/ipc/types";
import { AppShell } from "../../web/src/components/shell/AppShell";
import { TasksPage } from "../../web/src/pages/tasks/TasksPage";
import { Transcript } from "../../web/src/components/chat/Transcript";
import { RunPlanCard } from "../../web/src/pages/chat/ChatPage";
import { ComposerDeck } from "../../web/src/components/chat/ComposerDeck";
import { ComposerField } from "../../web/src/components/chat/ComposerField";
import { Select } from "../../web/src/components/controls/Select";
import { ContextMeter } from "../../web/src/components/chat/ContextMeter";
import { SendIcon } from "../../web/src/components/icons";
import { useT } from "../../web/src/i18n";
import { estimateMessagesTokens } from "../../web/src/session/context-usage";
import { bootstrapAt } from "../film";
import type { Page } from "../../web/src/App";
import type { Plan } from "../../web/src/session";

/**
 * 风格试验的真实组件状态页：?design-native=1&state=…
 * 直接复用当前 AppShell / Transcript / RunPlanCard / ComposerDeck / TaskCard，
 * 数据是“演示配置组”演示数据，不打真实 IPC。1440×900 原尺寸渲染，
 * 供 design-lab.mjs 一次性截图并测量 send / approval / 计划行 / 任务按钮的归一化 rect。
 */

export type NativeState =
  | "compose"
  | "message"
  | "plan"
  | "approval"
  | "run"
  | "submitted";
export const NATIVE_STATES: NativeState[] = [
  "compose",
  "message",
  "plan",
  "approval",
  "run",
  "submitted",
];
export const NATIVE_VIEWPORT = { width: 1440, height: 900 } as const;

const STAMP = "2026-10-02T10:24:00";
const PROMPT = "运行演示配置组";

const TOOL_LABELS: Record<string, string> = {
  "bgi.get_status": "检查游戏状态",
  "bgi.run_script_group": "运行演示配置组",
};

const DEMO_TOOLS: Bootstrap["tools"] = [
  {
    name: "bgi.get_status",
    label: "检查游戏状态",
    description: "读取 BetterGI 当前宿主与游戏状态",
    source: "bettergi",
  },
  {
    name: "bgi.run_script_group",
    label: "运行演示配置组",
    description: "按 groupName 启动 BetterGI 的演示配置组",
    source: "bettergi",
  },
];

const CONVERSATION = {
  id: "conv-demo",
  title: "演示配置组",
  createdAt: STAMP,
  updatedAt: STAMP,
  taskCount: 1,
};

function demoPlan(stage: "plan" | "approval"): Plan {
  const outcome = (index: number) =>
    stage === "approval" && index === 0 ? "verifiedSucceeded" : undefined;
  return {
    goal: "启动 BetterGI 的演示配置组",
    steps: [
      { id: "p1", title: "读取当前状态", outcome: outcome(0) },
      { id: "p2", title: "确认演示配置组", outcome: outcome(1) },
      { id: "p3", title: "提交运行", outcome: outcome(2) },
    ],
  };
}

function demoMessages(state: NativeState): MessageInfo[] {
  if (state === "compose") return [];
  const messages: MessageInfo[] = [
    { role: "user", content: PROMPT, createdAt: STAMP },
  ];
  if (state === "approval") {
    messages.push({
      role: "assistant",
      content: "",
      createdAt: STAMP,
      toolCalls: [
        { id: "c1", name: "bgi.get_status", arguments: {} },
      ],
    });
    messages.push({
      role: "tool",
      content: '{"ok":true}',
      toolCallId: "c1",
    });
  }
  if (state === "submitted") {
    messages.push({
      role: "assistant",
      content: "",
      createdAt: STAMP,
      toolCalls: [
        {
          id: "c2",
          name: "bgi.run_script_group",
          arguments: { groupName: "演示配置组", wait: false },
        },
      ],
    });
    messages.push({
      role: "tool",
      content: '{"accepted":true}',
      toolCallId: "c2",
    });
    messages.push({
      role: "assistant",
      content: "已提交运行「演示配置组」。",
      createdAt: STAMP,
    });
  }
  return messages;
}

function demoWorkflow(stateLabel: string): TaskSummary {
  return {
    id: "wf-demo",
    name: "演示配置组",
    description: "读取状态，确认项目，然后提交运行",
    state: "readyVerified",
    stateLabel,
    actionLabel: "运行",
    shortcut: {
      applicationName: "BetterGI",
      targetName: "演示配置组",
      action: {
        tool: "bgi.run_script_group",
        arguments: { groupName: "演示配置组" },
      },
    },
    runnable: true,
    pinned: true,
    sourceConversationId: CONVERSATION.id,
    sourceTitleSnapshot: CONVERSATION.title,
    sourceDeleted: false,
    publishedRevision: 1,
    revision: 1,
    modelUsage: "none",
    zeroToken: true,
    nodeCount: 1,
    updatedAt: STAMP,
    lastRunId: null,
    issue: null,
  };
}

function demoTask(): TaskInfo {
  return {
    id: "run-demo",
    conversationId: CONVERSATION.id,
    prompt: PROMPT,
    state: "executing",
    createdAt: STAMP,
    updatedAt: STAMP,
    contextTokens: 1284,
    contextWindow: 128000,
    inputTokens: 96,
    outputTokens: 210,
    source: {
      kind: "savedWorkflow",
      workflowId: "wf-demo",
      workflowRevision: 1,
    },
  };
}

function demoBootstrap(state: NativeState): Bootstrap {
  const base = structuredClone(bootstrapAt(0));
  const model = { ...base.models[0]!, id: "model-demo", name: "演示模型", model: "demo-fixture" };
  const workflows =
    state === "run" ? [demoWorkflow("运行中")] : [demoWorkflow("可以运行")];
  return {
    ...base,
    models: [model],
    tools: DEMO_TOOLS,
    conversations: state === "compose" ? [] : [{ ...CONVERSATION }],
    tasks: state === "run" ? [demoTask()] : [],
    workflows,
  };
}

function DemoChatPane({ state }: { state: NativeState }) {
  const t = useT();
  const scroll = useRef<HTMLDivElement>(null);
  const messages = demoMessages(state);
  const phase =
    state === "message" ? "正在处理请求" : state === "run" ? "正在执行" : undefined;
  const contextUsed = estimateMessagesTokens(messages, [""]);
  const bootstrap = demoBootstrap(state);
  const model = bootstrap.models[0]!;

  useEffect(() => {
    const el = scroll.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [state]);

  return (
    <section className="chat-workspace">
      <div className="chat-scene-switch">
        <div ref={scroll} className="chat-scroll">
          {state !== "compose" && (
            <div className="conversation-scene">
              <div className="conversation-flow">
                <div data-native-anchor={state === "submitted" ? "receipt" : "user-message"}>
                  <Transcript
                    key={messages.map((m) => `${m.role}:${m.content}`).join("|")}
                    messages={messages}
                    stream=""
                    phase={phase}
                    seconds={3}
                    toolLabels={TOOL_LABELS}
                  />
                </div>
                {(state === "plan" || state === "approval") && (
                  <div
                    data-native-anchor="plan"
                    style={{ width: "100%", display: "grid", justifyItems: "start" }}
                  >
                    <RunPlanCard plan={demoPlan(state)} />
                  </div>
                )}
                {state === "approval" && (
                  <section className="run-approval" data-native-anchor="approval">
                    <h3>{t.chat.confirmExec}</h3>
                    <p>启动 BetterGI 的演示配置组</p>
                    <div className="detail-actions">
                      <button type="button" className="primary-action" data-native-anchor="approval-allow" tabIndex={-1}>
                        允许
                      </button>
                      <button type="button" className="secondary-action" tabIndex={-1}>
                        拒绝
                      </button>
                    </div>
                  </section>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
      <div className="composer-dock" data-native-anchor="composer-dock">
        <ComposerDeck>
          <ComposerField
            aria-label={t.chat.message}
            placeholder={t.chat.composerPlaceholderNew}
            value={state === "compose" ? PROMPT : ""}
            tabIndex={-1}
            onChange={() => undefined}
          />
          <div className="composer-actions">
            <div className="composer-menu composer-approval">
              <Select
                label={t.chat.approvalLevel}
                value={bootstrap.permission.mode}
                options={bootstrap.permission.levels.map((level) => ({
                  value: level.value,
                  label: level.label,
                  description: level.description,
                }))}
                onChange={() => undefined}
              />
            </div>
            <div className="composer-submit">
              <ContextMeter used={contextUsed} window={model.contextWindow ?? 128000} />
              <div className="composer-menu composer-model">
                <Select
                  label={t.chat.model}
                  value={model.id}
                  options={[{ value: model.id, label: model.name, description: "演示模型（fixture）" }]}
                  onChange={() => undefined}
                />
              </div>
              <button
                type="button"
                className="send-action"
                aria-label={t.chat.send}
                title={t.chat.send}
                disabled={state !== "compose"}
                data-native-anchor="send"
                tabIndex={-1}
              >
                <SendIcon className="button-icon" />
              </button>
            </div>
          </div>
        </ComposerDeck>
      </div>
    </section>
  );
}

export function NativeScene({ state }: { state: NativeState }) {
  const noop = () => undefined;
  const bootstrap = demoBootstrap(state);
  const page: Page = state === "run" ? "tasks" : "chat";
  let content: ReactNode;
  if (state === "run") {
    content = (
      <TasksPage
        bootstrap={bootstrap}
        reload={async () => undefined}
        onOpenConversation={noop}
        onOpenTask={noop}
        onConnectTools={noop}
      />
    );
  } else {
    content = <DemoChatPane state={state} />;
  }
  return (
    <div
      data-native-root={state}
      style={{ width: NATIVE_VIEWPORT.width, height: NATIVE_VIEWPORT.height }}
    >
      {/* 内部 1000×625 逻辑视口 × 1.44 = 1440×900：真实组件字号放大，截图高清不糊。 */}
      <div
        style={{
          width: 1000,
          height: 625,
          transform: "scale(1.44)",
          transformOrigin: "0 0",
        }}
      >
        <AppShell
          bootstrap={bootstrap}
          page={page}
          detailsOpen={false}
          onPage={noop}
          onNew={noop}
          onConversation={noop}
          onToggleDetails={noop}
          reload={async () => undefined}
          composingNewChat={state === "compose"}
        >
          {content}
        </AppShell>
      </div>
    </div>
  );
}

export const NATIVE_PERMISSION = PREVIEW_PERMISSION;
