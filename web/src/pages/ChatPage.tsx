import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "../api";
import { SendActionButton } from "../components/actions/SleepyActionButtons";
import type { Bootstrap, MessageInfo, TaskInfo, RunApproval } from "../types";

interface ChatPageProps {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  onConversation(id: string): void;
  reload(): Promise<void>;
}

const toolNames: Record<string, string> = {
  "plan.update": "更新执行计划",
  "user.ask": "确认任务信息",
  "resource.search": "检索资源",
  "bgi.state.get": "读取游戏状态",
  "bgi.capability.search": "检索能力目录",
  "bgi.capability.describe": "确认能力契约",
  "bgi.capability.invoke": "执行 BetterGI 动作",
  "bgi.job.get": "读取执行结果",
  "bgi.job.cancel": "停止执行",
  "skills.search": "检索工作方法",
  "skills.read": "读取工作方法",
  "plugins.list": "读取扩展目录",
  "operation.propose": "准备领域操作",
  "operation.get": "读取操作状态",
};
const stepOutcomeLabels: Record<string, string> = {
  active: "当前步骤",
  verifiedSucceeded: "已验证",
  verifiedFailed: "验证未通过",
  failed: "失败",
  unknown: "结果未知",
};

function isBusy(task?: TaskInfo): boolean {
  return Boolean(
    task &&
    ![
      "answered",
      "succeeded",
      "partial",
      "needsReview",
      "failed",
      "cancelled",
    ].includes(task.state),
  );
}

function ActionMedallion({ kind }: { kind: "scan" | "route" }) {
  return <span className="tool-mark" aria-hidden="true">{kind === "scan" ? "◎" : "✧"}</span>;
}

function ToolActivity({ message }: { message: MessageInfo }) {
  if (message.role === "assistant" && message.toolCalls?.length) {
    return (
      <div className="tool-sequence">
        {message.toolCalls.map((call) => {
          const stateTool = call.name === "bgi.state.get";
          return (
            <details className="tool-pass" key={call.id}>
              <summary>
                <ActionMedallion kind={stateTool ? "scan" : "route"} />
                <span>
                  <strong>{toolNames[call.name] ?? "执行工具"}</strong>
                  <em>展开证据</em>
                </span>
              </summary>
              <div className="technical-evidence">
                <code>{call.name}</code>
                <pre>{JSON.stringify(call.arguments, null, 2)}</pre>
              </div>
            </details>
          );
        })}
      </div>
    );
  }
  if (message.role === "tool") {
    return (
      <details className="tool-result">
        <summary>查看返回内容</summary>
        <pre>{message.content}</pre>
      </details>
    );
  }
  return null;
}

export function ChatPage({
  bootstrap,
  conversationId,
  onConversation,
  reload,
}: ChatPageProps) {
  const [messages, setMessages] = useState<MessageInfo[]>([]);
  const [prompt, setPrompt] = useState("");
  const [task, setTask] = useState<TaskInfo>();
  const [streamText, setStreamText] = useState("");
  const [question, setQuestion] = useState("");
  const [approval, setApproval] = useState<RunApproval>();
  const [approvalSubmitting, setApprovalSubmitting] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const [queuedRuns, setQueuedRuns] = useState<TaskInfo[]>([]);
  const [connectionError, setConnectionError] = useState("");
  const [plan, setPlan] = useState<{
    goal: string;
    steps: Array<{
      id: string;
      title: string;
      tool?: string;
      capabilityId?: string;
      outcome?: string;
    }>;
  }>();
  const [sending, setSending] = useState(false);
  const generation = useRef(0);
  const [error, setError] = useState("");
  const [savingStrategy, setSavingStrategy] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(Boolean(conversationId));
  const chatScroll = useRef<HTMLDivElement>(null);
  const followTail = useRef(true);
  const busy = isBusy(task);
  const strategySaved = Boolean(
    task && (
      bootstrap.strategies.some(
        (strategy) => strategy.sourceRunId === task.id,
      ) || bootstrap.workflows.some((workflow) => workflow.verifiedFromRun === task.id)),
  );
  const controlCalls = new Set(
    messages.flatMap((m) =>
      (m.toolCalls ?? [])
        .filter((c) => c.name === "user.ask" || c.name === "plan.update")
        .map((c) => c.id),
    ),
  );
  const visibleMessages = messages.filter(
    (m) =>
      !(m.role === "tool" && controlCalls.has(m.toolCallId ?? "")) &&
      !(
        m.role === "assistant" &&
        !m.content &&
        m.toolCalls?.length &&
        m.toolCalls.every((c) => controlCalls.has(c.id))
      ),
  );

  const loadConversation = useCallback(async (id: string) => {
    const current = generation.current;
    try {
      const result = await api.conversation(id);
      if (current === generation.current) setMessages(result.messages);
    } catch (reason) {
      if (current === generation.current)
        setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (current === generation.current) setLoadingHistory(false);
    }
  }, []);

  useEffect(() => {
    generation.current++;
    followTail.current = true;
    setQueuedRuns([]);
    setConnectionError("");
    setError("");
    setStreamText("");
    setQuestion("");
    setApproval(undefined);
    setPlan(undefined);
    setTask(undefined);
    if (conversationId) {
      setLoadingHistory(true);
      void loadConversation(conversationId);
    } else {
      setMessages([]);
      setTask(undefined);
      setError("");
    }
  }, [conversationId, loadConversation]);

  useEffect(() => {
    const scroll = () => {
      if (!chatScroll.current) return;
      if (!messages.length && !busy) {
        chatScroll.current.scrollTo({ top: 0, behavior: "auto" });
      } else if (followTail.current) {
        chatScroll.current.scrollTo({
          top: chatScroll.current.scrollHeight,
          behavior: "auto",
        });
      }
    };
    const frame = requestAnimationFrame(scroll);
    const settle = window.setTimeout(scroll, 120);
    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(settle);
    };
  }, [messages, busy, task, streamText, question, approval, plan]);

  useEffect(() => {
    setApprovalSubmitting(false);
    setClock(Date.now());
    if (!approval) return;
    const timer = setInterval(() => setClock(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [approval?.id]);

  useEffect(() => {
    if (!conversationId) return;
    let stopped = false;
    let cursor = 0;
    const runs = new Map<string, TaskInfo>();
    const streams = new Map<string, string>();
    const approvals = new Map<string, RunApproval>();
    const questions = new Map<string, string>();
    const plans = new Map<string, NonNullable<typeof plan>>();
    const consume = async () => {
      while (!stopped) {
        try {
          const batch = await api.events(conversationId, cursor);
          if (stopped) return;
          setConnectionError("");
          let refresh = false;
          for (const event of batch.events) {
            if (event.sequence <= cursor) continue;
            cursor = event.sequence;
            if (event.kind === "input.received") refresh = true;
            if (event.kind === "run.created" || event.kind === "run.changed") {
              const run = event.data as unknown as TaskInfo;
              runs.set(run.id, run);
              if (event.kind === "run.created" || run.state === "deciding")
                refresh = true;
              if (!isBusy(run)) {
                streams.delete(run.id);
                approvals.delete(run.id);
                questions.delete(run.id);
                refresh = true;
              }
            }
            if (event.kind === "assistant.delta")
              streams.set(
                event.runId,
                (streams.get(event.runId) ?? "") +
                  String(event.data.text ?? ""),
              );
            if (event.kind === "assistant.completed") {
              streams.delete(event.runId);
              refresh = true;
            }
            if (event.kind === "tool.completed") {
              refresh = true;
              approvals.delete(event.runId);
              questions.delete(event.runId);
            }
            if (event.kind === "approval.requested")
              approvals.set(event.runId, event.data as unknown as RunApproval);
            if (event.kind === "question")
              questions.set(event.runId, String(event.data.question ?? ""));
            if (
              event.kind === "step.started" ||
              event.kind === "step.finished"
            ) {
              const current = plans.get(event.runId);
              if (current)
                plans.set(event.runId, {
                  ...current,
                  steps: current.steps.map((step) =>
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
            if (event.kind === "plan.changed")
              plans.set(
                event.runId,
                event.data as unknown as {
                  goal: string;
                  steps: Array<{ id: string; title: string }>;
                },
              );
          }
          const list = [...runs.values()];
          const active =
            list.find((r) => isBusy(r) && r.state !== "queued") ??
            list.find(isBusy) ??
            list.at(-1);
          if (refresh) {
            const persisted = await api.conversation(conversationId);
            if (stopped) return;
            setMessages(persisted.messages);
          }
          setPlan(active ? plans.get(active.id) : undefined);
          setQueuedRuns(list.filter((run) => run.state === "queued"));
          setTask(active);
          setStreamText(
            active && isBusy(active) ? (streams.get(active.id) ?? "") : "",
          );
          setApproval(
            active?.state === "awaitingApproval"
              ? approvals.get(active.id)
              : undefined,
          );
          setQuestion(
            active?.state === "awaitingUser"
              ? (questions.get(active.id) ?? "")
              : "",
          );
          if (refresh) {
            if (!stopped) void reload();
          }
        } catch (reason) {
          if (stopped) return;
          setConnectionError(
            reason instanceof Error ? reason.message : String(reason),
          );
          await new Promise((resolve) => setTimeout(resolve, 1500));
        }
      }
    };
    void consume();
    return () => {
      stopped = true;
    };
  }, [conversationId, loadConversation, reload]);

  const send = async (queued = false) => {
    const value = prompt.trim();
    if (!value || sending) return;
    setSending(true);
    followTail.current = true;
    setError("");
    setPrompt("");
    try {
      if (busy && task && !queued) {
        await api.supplement(task.id, value);
        return;
      }
      const created = await api.submitTask(value, conversationId);
      if (!busy) setTask(created);
      onConversation(created.conversationId);
      await reload();
    } catch (reason) {
      setPrompt(value);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSending(false);
    }
  };

  return (
    <section
      className={`chat-workspace ${messages.length ? "has-conversation" : "is-empty"}`}
    >
      <div
        className="chat-scroll"
        ref={chatScroll}
        onScroll={(event) => {
          const e = event.currentTarget;
          followTail.current =
            e.scrollHeight - e.scrollTop - e.clientHeight < 120;
        }}
      >
        {loadingHistory ? (
          <div className="history-loading">载入对话</div>
        ) : messages.length === 0 && !busy ? (
          <div className="history-loading">对话已就绪</div>
        ) : (
          <div className="conversation-scene">
            <div className="conversation-flow">
              {visibleMessages.map((message, index) => (
                <div
                  className={`timeline-entry ${message.role}`}
                  key={`${index}-${message.content}`}
                >
                  {message.role === "user" ? (
                    <blockquote>{message.content}</blockquote>
                  ) : message.role === "assistant" ? (
                    <article className="agent-turn">
                      <div>
                        {message.content ? <p>{message.content}</p> : null}
                        <ToolActivity message={message} />
                      </div>
                    </article>
                  ) : (
                    <ToolActivity message={message} />
                  )}
                </div>
              ))}
              {plan ? (
                <details className="run-plan">
                  <summary>{plan.goal}</summary>
                  <ol>
                    {plan.steps.map((step) => (
                      <li key={step.id}>
                        {step.title}
                        {step.outcome
                          ? ` · ${stepOutcomeLabels[step.outcome] ?? "待核对"}`
                          : ""}
                      </li>
                    ))}
                  </ol>
                </details>
              ) : null}
              {(task?.source?.kind === "savedStrategy" ||
                task?.source?.kind === "savedWorkflow") &&
              !busy &&
              task.result ? (
                <div className="manual-run-result">{task.result}</div>
              ) : null}
              {streamText ? (
                <article className="agent-turn">
                  <p>{streamText}</p>
                </article>
              ) : null}
              {question ? (
                <section className="run-question">
                  <p>{question}</p>
                  <p>在下方输入补充信息即可继续。</p>
                </section>
              ) : null}
              {approval ? (
                <section className="run-approval">
                  <h3>允许这次游戏操作？</h3>
                  <p>
                    {approval.request.binding?.description ??
                      approval.request.methodId}
                  </p>
                  {approval.request.arguments &&
                  Object.keys(approval.request.arguments).length > 0 ? (
                    <pre>
                      {JSON.stringify(approval.request.arguments, null, 2)}
                    </pre>
                  ) : null}
                  <div>
                    <button
                      disabled={
                        approvalSubmitting || clock >= approval.expiresAt * 1000
                      }
                      onClick={() => {
                        setApprovalSubmitting(true);
                        void api.approve(approval.id, true).catch((e) => {
                          setError(String(e));
                          setApprovalSubmitting(false);
                        });
                      }}
                    >
                      允许这一次
                    </button>
                    <button
                      disabled={
                        approvalSubmitting || clock >= approval.expiresAt * 1000
                      }
                      onClick={() => {
                        setApprovalSubmitting(true);
                        void api.approve(approval.id, false).catch((e) => {
                          setError(String(e));
                          setApprovalSubmitting(false);
                        });
                      }}
                    >
                      拒绝
                    </button>
                  </div>
                  {clock >= approval.expiresAt * 1000 ? (
                    <p>此次确认已过期。</p>
                  ) : null}
                </section>
              ) : null}
              {busy && !streamText && !question && !approval ? (
                <div className="run-activity">
                  <strong>
                    {task?.state === "cancelling" ? "正在停止" : "正在处理"}
                  </strong>
                </div>
              ) : null}
              {task &&
              ["failed", "cancelled", "needsReview", "partial"].includes(
                task.state,
              ) ? (
                <div className="run-error">
                  {task.error ??
                    (task.state === "needsReview"
                      ? "结果尚未确认，请核对执行证据。"
                      : task.state === "partial"
                        ? "仅部分目标已完成。"
                        : "本次运行已停止。")}
                  {task.state === "needsReview" ? (
                    <button
                      className="runtime-action"
                      onClick={() =>
                        void api
                          .resume(task.id)
                          .then(setTask)
                          .catch((e) => setError(String(e)))
                      }
                    >
                      核对并恢复
                    </button>
                  ) : null}
                </div>
              ) : null}
              {task?.state === "succeeded" &&
              task.source?.kind !== "savedStrategy" &&
              task.source?.kind !== "savedWorkflow" &&
              plan ? (
                <div className="strategy-save">
                  <span>这次运行已经验证成功，可以直接复用。</span>
                  <button
                    className="runtime-action"
                    disabled={savingStrategy || strategySaved}
                    onClick={() => {
                      setSavingStrategy(true);
                      void api
                        [plan.steps.every((step) => Boolean(step.tool))
                          ? "extractWorkflow"
                          : "extractStrategy"](task.id, plan.goal)
                        .then(reload)
                        .catch((e) => setError(String(e)))
                        .finally(() => setSavingStrategy(false));
                    }}
                  >
                    {savingStrategy
                      ? "正在保存"
                      : strategySaved
                        ? "已保存到运行库"
                        : "保存到运行库"}
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        )}
      </div>

      <div className="composer-dock">
        {error || connectionError ? (
          <div className="inline-error" role="alert">
            {error || connectionError}
          </div>
        ) : null}
        {queuedRuns.length ? (
          <section className="run-input-actions" aria-label="等待执行的消息">
            {queuedRuns.map((run, index) => (
              <div key={run.id}>
                <span>
                  {index + 1}. {run.prompt}
                </span>
                <button
                  className="runtime-action"
                  onClick={() =>
                    void api
                      .cancelTask(run.id)
                      .catch((e) => setError(String(e)))
                  }
                >
                  取消排队
                </button>
              </div>
            ))}
          </section>
        ) : null}
        {busy ? (
          <div className="run-input-actions">
            <button
              className="runtime-action"
              disabled={!prompt.trim() || sending}
              onClick={() => void send(true)}
            >
              下一条排队
            </button>
            <button
              className="runtime-action"
              onClick={() =>
                task &&
                void api
                  .cancelTask(task.id)
                  .then(setTask)
                  .catch((e) => setError(String(e)))
              }
            >
              停止当前运行
            </button>
          </div>
        ) : null}
        <div className="command-deck">
          <svg className="composer-quill" viewBox="0 0 36 36" aria-hidden="true">
            <path d="M29 4C17 6 9 14 7 29c8-7 15-14 22-25Z" />
            <path d="M7 29c6-4 11-8 16-13M7 29l-3 3" />
          </svg>
          <textarea
            aria-label="给 Agent 的消息"
            placeholder={
              task?.state === "awaitingUser"
                ? "输入补充信息，发送后继续"
                : task?.state === "needsReview"
                  ? "输入新的请求；原运行可在上方恢复"
                  : "给 Sleepy Doll 一个目标…"
            }
            value={prompt}
            disabled={sending}
            onChange={(event) => setPrompt(event.target.value)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return;
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void send();
              }
            }}
          />
          <SendActionButton
            label={busy ? "发送补充" : "发送"}
            disabled={sending || !prompt.trim()}
            onClick={() => void send()}
          />
        </div>
      </div>
    </section>
  );
}
