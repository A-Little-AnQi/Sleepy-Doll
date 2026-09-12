import { useEffect, useSyncExternalStore } from "react";
import { api } from "./api";
import type { MessageInfo, RunApproval, TaskInfo } from "./types";

export const taskLabels: Record<string, string> = {
  queued: "排队中",
  deciding: "响应中",
  running: "运行中",
  executing: "执行中",
  awaitingUser: "待回复",
  awaitingApproval: "待确认",
  waitingJob: "执行中",
  verifying: "验证中",
  recovering: "恢复中",
  cancelling: "停止中",
  cancelled: "已停止",
  answered: "已完成",
  succeeded: "已完成",
  failed: "失败",
  needsReview: "待核对",
  partial: "部分完成",
};
export const isRunning = (task?: TaskInfo) =>
  !!task &&
  ![
    "answered",
    "succeeded",
    "failed",
    "cancelled",
    "needsReview",
    "partial",
  ].includes(task.state);
export interface Plan {
  goal: string;
  steps: Array<{ id: string; title: string; tool?: string; outcome?: string }>;
}
interface Snapshot {
  messages: MessageInfo[];
  task: TaskInfo | undefined;
  queued: TaskInfo[];
  stream: string;
  question: string;
  approval: RunApproval | undefined;
  plan: Plan | undefined;
  loading: boolean;
  error: string;
}
const empty: Snapshot = {
  messages: [],
  task: undefined,
  queued: [],
  stream: "",
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
  private runs = new Map<string, TaskInfo>();
  private streams = new Map<string, string>();
  private completedStreams = new Set<string>();
  private questions = new Map<string, string>();
  private approvals = new Map<string, RunApproval>();
  private plans = new Map<string, Plan>();
  constructor(readonly id: string) {}
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    this.start();
    return () => {
      this.listeners.delete(listener);
    };
  };
  read = () => this.snapshot;
  private publish(patch: Partial<Snapshot>) {
    this.snapshot = { ...this.snapshot, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  start() {
    if (!this.running) {
      this.running = true;
      void this.consume();
    }
  }
  private async consume() {
    let failures = 0;
    let needsHistory = true;
    try {
      // The session owns its subscription. Unmounting a page never aborts a run.
      do {
        try {
          if (needsHistory) {
            const history = await api.conversation(this.id);
            this.publish({ messages: history.messages, loading: false });
            needsHistory = false;
          }
          const batch = await api.events(this.id, this.cursor);
          let refresh = false;
          for (const event of batch.events) {
            if (event.sequence <= this.cursor) continue;
            if (["run.created", "run.changed"].includes(event.kind)) {
              const run = event.data as unknown as TaskInfo;
              this.runs.set(run.id, run);
              refresh = true;
              if (!isRunning(run)) {
                this.approvals.delete(run.id);
                this.questions.delete(run.id);
              }
            }
            if (event.kind === "assistant.delta")
              this.streams.set(
                event.runId,
                (this.streams.get(event.runId) ?? "") +
                  String(event.data.text ?? ""),
              );
            if (event.kind === "assistant.completed") {
              this.completedStreams.add(event.runId);
              refresh = true;
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
          const runs = [...this.runs.values()];
          const task =
            runs.find((run) => isRunning(run) && run.state !== "queued") ??
            runs.find(isRunning) ??
            runs.at(-1);
          this.publish({
            task,
            queued: runs.filter((run) => run.state === "queued"),
            stream: task ? (this.streams.get(task.id) ?? "") : "",
            question:
              task?.state === "awaitingUser"
                ? (this.questions.get(task.id) ?? "")
                : "",
            approval:
              task?.state === "awaitingApproval"
                ? this.approvals.get(task.id)
                : undefined,
            plan: task ? this.plans.get(task.id) : undefined,
            error: "",
            loading: false,
          });
          if (refresh || failures) {
            const history = await api.conversation(this.id);
            for (const id of this.completedStreams) this.streams.delete(id);
            this.completedStreams.clear();
            this.publish({
              messages: history.messages,
              stream: task ? (this.streams.get(task.id) ?? "") : "",
            });
          }
          failures = 0;
        } catch (error) {
          failures++;
          this.publish({
            error: `连接中断，正在重连。${error instanceof Error ? error.message : String(error)}`,
            loading: false,
          });
          await new Promise((resolve) =>
            setTimeout(
              resolve,
              Math.min(1000 * 2 ** Math.min(failures - 1, 4), 10000),
            ),
          );
        }
      } while (this.listeners.size || [...this.runs.values()].some(isRunning));
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
export function session(id: string) {
  let entry = sessions.get(id);
  if (!entry) {
    entry = new Session(id);
    sessions.set(id, entry);
  }
  return entry;
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
