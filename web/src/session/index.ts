import { useEffect, useSyncExternalStore } from "react";
import { api } from "../ipc/api";
import type {
  ContextActivity,
  ShortcutProposal,
  MessageInfo,
  RunApproval,
  TaskInfo,
} from "../ipc/types";

/** 把任意抛出物变成给人看的一句话。 */
export function readError(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}

/** 运行状态文案，与运行时 `RunState::label` 一一对应。 */
export const taskLabels: Record<string, string> = {
  queued: "排队中",
  preflighting: "正在检查运行条件",
  deciding: "正在处理请求",
  running: "运行中",
  executing: "正在执行",
  awaitingUser: "等待补充信息",
  awaitingApproval: "等待确认操作",
  waitingJob: "等待工具完成",
  verifying: "正在核对结果",
  recovering: "正在恢复运行状态",
  cancelling: "正在停止",
  blocked: "暂时无法运行",
  cancelled: "已停止",
  answered: "已回答",
  succeeded: "已完成",
  failed: "执行失败",
  needsReview: "结果待核对",
  partial: "部分完成",
};

/** 仍在推进的运行。`needsReview` 不是「运行中」，但它仍占着互斥锁。 */
export const isRunning = (task?: TaskInfo) =>
  !!task &&
  ![
    "answered",
    "succeeded",
    "failed",
    "cancelled",
    "needsReview",
    "partial",
    "blocked",
  ].includes(task.state);

/** 快捷任务的运行是否在推进：对话（agent 来源）的运行不算，别点亮任务入口。 */
export const isTaskRunActive = (task?: TaskInfo) =>
  isRunning(task) && task?.source?.kind !== "agent";

/** 阶段文案：显示当前在哪一步。 */
export function phaseLabel(task?: TaskInfo): string | undefined {
  if (!isRunning(task) || !task) return undefined;
  switch (task.state) {
    case "queued":
      return "排队中";
    case "preflighting":
      return "正在检查运行条件";
    case "cancelling":
      return "正在停止";
    case "waitingJob":
      return "等待工具完成";
    case "verifying":
      return "正在核对结果";
    case "recovering":
      return "正在恢复运行状态";
    case "executing":
      return "正在执行";
    case "awaitingUser":
      return "等待补充信息";
    case "awaitingApproval":
      return "等待确认操作";
    case "running":
      return "正在处理请求";
    default:
      return "等待模型响应";
  }
}
export interface Plan {
  goal: string;
  steps: Array<{ id: string; title: string; tool?: string; outcome?: string }>;
}
export interface Snapshot {
  shortcutProposal: ShortcutProposal | undefined;
  messages: MessageInfo[];
  task: TaskInfo | undefined;
  stream: string;
  contextActivities: ContextActivity[];
  question: string;
  approval: RunApproval | undefined;
  plan: Plan | undefined;
  loading: boolean;
  error: string;
}
const empty: Snapshot = {
  shortcutProposal: undefined,
  messages: [],
  task: undefined,
  stream: "",
  contextActivities: [],
  question: "",
  approval: undefined,
  plan: undefined,
  loading: false,
  error: "",
};
class Session {
  snapshot: Snapshot = { ...empty, loading: true };
  listeners = new Set<() => void>();
  private cursor = 0;
  private running = false;
  private disposed = false;
  private runs = new Map<string, TaskInfo>();
  private pendingRuns = new Map<string, TaskInfo>();
  private streams = new Map<
    string,
    Array<{ sequence: number; text: string }>
  >();
  private streamBoundaries = new Map<string, number>();
  private questions = new Map<string, string>();
  private approvals = new Map<string, RunApproval>();
  private plans = new Map<string, Plan>();
  private shortcutProposals = new Map<string, ShortcutProposal>();
  private contextActivities = new Map<number, ContextActivity>();
  private contextActivityBoundary = 0;
  constructor(readonly id: string) {}
  subscribe = (listener: () => void) => {
    if (this.disposed) return () => {};
    this.listeners.add(listener);
    this.start();
    return () => {
      this.listeners.delete(listener);
      if (
        this.disposed &&
        !this.listeners.size &&
        sessions.get(this.id) === this
      )
        sessions.delete(this.id);
    };
  };
  read = () => this.snapshot;
  private publish(patch: Partial<Snapshot>) {
    this.snapshot = { ...this.snapshot, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  private acceptRun(run: TaskInfo) {
    const previous = this.runs.get(run.id);
    if (previous && (previous.revision ?? 0) > (run.revision ?? 0)) return;
    this.runs.set(run.id, run);
    this.pendingRuns.set(run.id, run);
    if (!isRunning(run)) {
      this.approvals.delete(run.id);
      this.questions.delete(run.id);
      for (const activity of this.contextActivities.values())
        if (activity.runId === run.id && activity.state === "running")
          activity.state = "failed";
    }
  }
  private acceptRunSnapshot(history: { runs?: TaskInfo[] }) {
    for (const run of history.runs ?? []) this.acceptRun(run);
  }
  private currentTask() {
    const runs = [...this.runs.values()];
    return (
      runs.find((run) => isRunning(run) && run.state !== "queued") ??
      runs.find(isRunning) ??
      runs.at(-1)
    );
  }
  private acceptHistory(messages: MessageInfo[]) {
    for (const message of messages) {
      if (
        message.role !== "assistant" ||
        !message.runId ||
        message.streamBoundary == null
      )
        continue;
      const boundary = Math.max(
        this.streamBoundaries.get(message.runId) ?? 0,
        message.streamBoundary,
      );
      this.streamBoundaries.set(message.runId, boundary);
      const stream = this.streams.get(message.runId);
      if (stream)
        this.streams.set(
          message.runId,
          stream.filter((part) => part.sequence > boundary),
        );
    }
  }
  private streamFor(runId?: string) {
    return runId
      ? (this.streams.get(runId) ?? []).map((part) => part.text).join("")
      : "";
  }
  private acceptContext(history: {
    contextActivities?: ContextActivity[];
    contextActivityBoundary?: number;
  }) {
    if (history.contextActivities == null) return;
    this.contextActivities = new Map(
      history.contextActivities.map((activity) => [
        activity.id,
        { ...activity },
      ]),
    );
    this.contextActivityBoundary = history.contextActivityBoundary ?? 0;
  }
  start() {
    if (this.disposed) return;
    if (!this.running) {
      this.running = true;
      void this.consume();
    }
  }
  dispose() {
    const wasSubscribed = this.listeners.size > 0;
    this.disposed = true;
    this.runs.clear();
    this.pendingRuns.clear();
    this.streams.clear();
    this.streamBoundaries.clear();
    this.questions.clear();
    this.approvals.clear();
    this.plans.clear();
    this.contextActivities.clear();
    this.publish({ ...empty });
    this.listeners.clear();
    return wasSubscribed;
  }
  private async consume() {
    let failures = 0;
    let needsHistory = true;
    try {
      // 卸载页面不中断运行。
      do {
        try {
          if (needsHistory) {
            const history = await api.conversation(this.id);
            if (this.disposed) break;
            this.acceptHistory(history.messages);
            this.acceptContext(history);
            this.acceptRunSnapshot(history);
            const task = this.currentTask();
            this.publish({
              task,
              messages: history.messages,
              stream: this.streamFor(task?.id),
              contextActivities: [...this.contextActivities.values()].map(
                (activity) => ({ ...activity }),
              ),
              loading: false,
            });
            needsHistory = false;
          }
          const batch = await api.events(this.id, this.cursor);
          if (this.disposed) break;
          if (batch.snapshotRequired) {
            // 事件窗口被裁过，游标已经对不上：重取消息快照并从头续流。
            this.cursor = 0;
            this.streams.clear();
            this.contextActivities.clear();
            this.contextActivityBoundary = 0;
            needsHistory = true;
            continue;
          }
          let refresh = false;
          for (const event of batch.events) {
            if (event.sequence <= this.cursor) continue;
            if (["run.created", "run.changed"].includes(event.kind)) {
              const run = event.data as unknown as TaskInfo;
              this.acceptRun(run);
              refresh = true;
            }
            if (
              event.kind === "context.compaction.started" &&
              event.sequence > this.contextActivityBoundary
            ) {
              this.contextActivities.set(event.sequence, {
                id: event.sequence,
                runId: event.runId,
                state: "running",
              });
            }
            if (
              event.kind === "context.compacted" &&
              event.data.mode === "summary" &&
              event.sequence > this.contextActivityBoundary
            ) {
              const activity = [...this.contextActivities.values()].findLast(
                (item) =>
                  item.runId === event.runId && item.state === "running",
              );
              if (activity) activity.state = "completed";
              else
                this.contextActivities.set(event.sequence, {
                  id: event.sequence,
                  runId: event.runId,
                  state: "completed",
                });
            }
            if (
              event.kind === "assistant.delta" &&
              event.sequence > (this.streamBoundaries.get(event.runId) ?? 0)
            ) {
              const stream = this.streams.get(event.runId) ?? [];
              stream.push({
                sequence: event.sequence,
                text: String(event.data.text ?? ""),
              });
              this.streams.set(event.runId, stream);
            }
            if (event.kind === "assistant.completed") {
              // Each completion closes only its own model response. A later
              // delta in this batch belongs to the next response, not this one.
              this.streams.delete(event.runId);
              refresh = true;
            }
            if (event.kind === "shortcut.proposed")
              this.shortcutProposals.set(
                event.runId,
                event.data as unknown as ShortcutProposal,
              );
            if (event.kind === "shortcut.saved") {
              window.dispatchEvent(
                new CustomEvent("sleepy-doll:shortcut-saved"),
              );
            }
            if (event.kind === "cancel.requested") {
              const run = this.runs.get(event.runId);
              if (run && isRunning(run))
                this.runs.set(run.id, { ...run, state: "cancelling" });
            }
            if (["input.received", "tool.completed"].includes(event.kind))
              refresh = true;
            if (event.kind === "approval.requested")
              this.approvals.set(
                event.runId,
                event.data as unknown as RunApproval,
              );
            if (event.kind === "question")
              this.questions.set(
                event.runId,
                String(event.data.question ?? ""),
              );
            if (event.kind === "plan.changed")
              this.plans.set(event.runId, event.data as unknown as Plan);
            if (["step.started", "step.finished"].includes(event.kind)) {
              const plan = this.plans.get(event.runId);
              if (plan)
                this.plans.set(event.runId, {
                  ...plan,
                  steps: plan.steps.map((step) =>
                    step.id === event.data.id
                      ? {
                          ...step,
                          outcome:
                            event.kind === "step.started"
                              ? "active"
                              : String(event.data.outcome),
                        }
                      : step,
                  ),
                });
            }
            this.cursor = event.sequence;
          }
          // Commit persisted messages and their stream replacement together.
          // Publishing in between briefly duplicates answers and changes height.
          const history =
            refresh || failures ? await api.conversation(this.id) : undefined;
          if (this.disposed) break;
          if (history) {
            this.acceptHistory(history.messages);
            this.acceptContext(history);
            this.acceptRunSnapshot(history);
          }
          const task = this.currentTask();
          if (batch.events.length || history || this.snapshot.error)
            this.publish({
              ...(history ? { messages: history.messages } : {}),
              task,
              stream: this.streamFor(task?.id),
              contextActivities: [...this.contextActivities.values()].map(
                (activity) => ({ ...activity }),
              ),
              question:
                task?.state === "awaitingUser"
                  ? (this.questions.get(task.id) ?? "")
                  : "",
              approval:
                task?.state === "awaitingApproval"
                  ? this.approvals.get(task.id)
                  : undefined,
              plan: task ? this.plans.get(task.id) : undefined,
              shortcutProposal: task
                ? this.shortcutProposals.get(task.id)
                : undefined,
              error: "",
              loading: false,
            });
          failures = 0;
          for (const run of this.pendingRuns.values()) emitRun(run);
          this.pendingRuns.clear();
          if (!batch.events.length && !window.ipc) {
            await new Promise((resolve) => setTimeout(resolve, 1000));
          }
        } catch (error) {
          if (this.disposed) break;
          failures++;
          this.publish({
            error: `暂时无法同步对话，正在重试。${readError(error)}`,
            loading: false,
          });
          await new Promise((resolve) =>
            setTimeout(
              resolve,
              Math.min(1000 * 2 ** Math.min(failures - 1, 4), 10000),
            ),
          );
        }
      } while (
        !this.disposed &&
        (this.listeners.size || [...this.runs.values()].some(isRunning))
      );
    } catch (error) {
      this.publish({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
      });
    } finally {
      this.running = false;
    }
  }
}
const sessions = new Map<string, Session>();
export function forgetSession(id: string) {
  const entry = sessions.get(id);
  if (entry && !entry.dispose()) sessions.delete(id);
}
export function session(id: string) {
  let entry = sessions.get(id);
  if (!entry) {
    entry = new Session(id);
    sessions.set(id, entry);
  }
  return entry;
}
const runListeners = new Set<(task: TaskInfo) => void>();

function emitRun(task: TaskInfo) {
  runListeners.forEach((listener) => listener(task));
}

/** 运行状态变化时通知壳层，用于更新列表。 */
export function subscribeRuns(listener: (task: TaskInfo) => void) {
  runListeners.add(listener);
  return () => {
    runListeners.delete(listener);
  };
}

export function watchTasks(tasks: TaskInfo[]) {
  for (const task of tasks)
    if (isRunning(task)) session(task.conversationId).start();
}
const noSubscribe = () => () => {};
const readEmpty = () => empty;
export function useSession(id?: string) {
  const entry = id ? session(id) : undefined;
  useEffect(() => {
    entry?.start();
  }, [entry]);
  return useSyncExternalStore(
    entry?.subscribe ?? noSubscribe,
    entry?.read ?? readEmpty,
  );
}
