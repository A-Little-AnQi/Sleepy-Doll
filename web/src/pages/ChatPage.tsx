import { useCallback, useEffect, useRef, useState } from "react";

import character from "../assets/moon-character-v5-feet.png";
import { api } from "../api";
import { SendIcon, StopIcon } from "../components/icons";
import type { Bootstrap, MessageInfo, TaskInfo, RunApproval } from "../types";

interface ChatPageProps {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  onConversation(id: string): void;
  reload(): Promise<void>;
}

/** Tool identifiers are machine names. These are what each one actually does,
 * in the words a user would use. */
const toolNames: Record<string, string> = {
  "plan.update": "更新执行计划",
  "user.ask": "向你确认信息",
  "resource.search": "查找可用的资源",
  "bgi.state.get": "读取游戏状态",
  "bgi.capability.search": "查找能做的操作",
  "bgi.capability.describe": "确认操作的用法",
  "bgi.capability.invoke": "执行游戏操作",
  "bgi.job.get": "查看执行结果",
  "bgi.job.cancel": "停止执行",
  "skills.search": "查找技能",
  "skills.read": "读取技能说明",
  "plugins.list": "查看已装插件",
  "operation.propose": "准备一项操作",
  "operation.get": "查看操作状态",
};
const stepOutcomeLabels: Record<string, string> = {
  active: "进行中",
  verifiedSucceeded: "已完成并确认",
  verifiedFailed: "完成但结果不符",
  failed: "失败",
  unknown: "结果未确认",
};

const EXAMPLES = ["看看游戏现在是什么情况", "有哪些路线可以跑？"];

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

function ToolActivity({ message }: { message: MessageInfo }) {
  if (message.role === "assistant" && message.toolCalls?.length) {
    return (
      <div className="tool-sequence">
        {message.toolCalls.map((call) => (
          <details className="tool-pass" key={call.id}>
            <summary>
              <span>
                <strong>{toolNames[call.name] ?? call.name}</strong>
                <em>查看详情</em>
              </span>
            </summary>
            <div className="technical-evidence">
              <p className="evidence-note">这一步发出的原始请求：</p>
              <code>{call.name}</code>
              <pre>{JSON.stringify(call.arguments, null, 2)}</pre>
            </div>
          </details>
        ))}
      </div>
    );
  }
  if (message.role === "tool") {
    return (
      <details className="tool-result">
        <summary>查看这一步的返回内容</summary>
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
    task &&
    (bootstrap.strategies.some(
      (strategy) => strategy.sourceRunId === task.id,
    ) ||
      bootstrap.workflows.some(
        (workflow) => workflow.verifiedFromRun === task.id,
      )),
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
  const approvalExpired = Boolean(
    approval && clock >= approval.expiresAt * 1000,
  );
  const canReplay = Boolean(plan?.steps.every((step) => Boolean(step.tool)));

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
      setLoadingHistory(false);
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

  const sendError = error || connectionError;
  const showWelcome =
    !conversationId && messages.length === 0 && !busy && !loadingHistory;

  return (
    <section className="chat-workspace">
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
          <div className="history-loading">正在载入对话…</div>
        ) : showWelcome ? (
          <div className="chat-welcome">
            <img className="chat-welcome-art" src={character} alt="" />
            <h2>你想让 Sleepy Doll 做什么？</h2>
            <p>用一句话说清楚目标就行。它要操作游戏时，会先停下来问你。</p>
            <div className="chat-examples">
              {EXAMPLES.map((example) => (
                <button
                  key={example}
                  type="button"
                  onClick={() => {
                    setPrompt(example);
                    chatScroll.current?.scrollTo({ top: 0 });
                  }}
                >
                  {example}
                </button>
              ))}
            </div>
            {bootstrap.conversations.length > 0 ? (
              <div className="chat-resume">
                <h3>继续之前的对话</h3>
                <ul>
                  {bootstrap.conversations.slice(0, 5).map((conversation) => (
                    <li key={conversation.id}>
                      <button
                        type="button"
                        onClick={() => onConversation(conversation.id)}
                      >
                        {conversation.title}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
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
                <details className="run-plan" open={busy}>
                  <summary>
                    {busy ? "正在执行的步骤" : "这次的执行步骤"}（{plan.goal}）
                  </summary>
                  <ol>
                    {plan.steps.map((step) => (
                      <li key={step.id}>
                        {step.title}
                        {step.outcome
                          ? ` · ${stepOutcomeLabels[step.outcome] ?? "未确认"}`
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
                  <h3>Sleepy Doll 需要你补充信息</h3>
                  <p>{question}</p>
                  <p className="muted">
                    在下面的输入框里回复，发送后它会继续。
                  </p>
                </section>
              ) : null}
              {approval ? (
                <section className="run-approval">
                  <h3>这一步要操作游戏，需要你同意</h3>
                  <p className="approval-what">
                    {approval.request.binding?.description ??
                      approval.request.methodId}
                  </p>
                  {approval.request.arguments &&
                  Object.keys(approval.request.arguments).length > 0 ? (
                    <details>
                      <summary>这一步会用到的参数</summary>
                      <pre>
                        {JSON.stringify(approval.request.arguments, null, 2)}
                      </pre>
                    </details>
                  ) : null}
                  <div className="approval-actions">
                    <button
                      className="primary-action"
                      disabled={approvalSubmitting || approvalExpired}
                      onClick={() => {
                        setApprovalSubmitting(true);
                        void api.approve(approval.id, true).catch((e) => {
                          setError(String(e));
                          setApprovalSubmitting(false);
                        });
                      }}
                    >
                      {approvalSubmitting ? "正在提交…" : "同意，执行这一步"}
                    </button>
                    <button
                      disabled={approvalSubmitting || approvalExpired}
                      onClick={() => {
                        setApprovalSubmitting(true);
                        void api.approve(approval.id, false).catch((e) => {
                          setError(String(e));
                          setApprovalSubmitting(false);
                        });
                      }}
                    >
                      不同意
                    </button>
                  </div>
                  {approvalExpired ? (
                    <p className="muted">
                      这次确认已经超时失效了。重新发一条消息让它再来一次。
                    </p>
                  ) : (
                    <p className="muted">
                      同意只对这一次有效，下次操作还会再问你。
                    </p>
                  )}
                </section>
              ) : null}
              {busy && !streamText && !question && !approval ? (
                <div className="run-activity">
                  {task?.state === "cancelling"
                    ? "正在停止…"
                    : "正在处理，请稍等…"}
                </div>
              ) : null}
              {task &&
              ["failed", "cancelled", "needsReview", "partial"].includes(
                task.state,
              ) ? (
                <div className="run-error">
                  <strong>
                    {task.state === "failed"
                      ? "这次没有成功"
                      : task.state === "cancelled"
                        ? "这次已停止"
                        : task.state === "needsReview"
                          ? "结果还没确认"
                          : "只完成了部分目标"}
                  </strong>
                  <p>
                    {task.error ??
                      (task.state === "needsReview"
                        ? "Sleepy Doll 不确定游戏里实际发生了什么，需要再核对一次才能继续。"
                        : task.state === "partial"
                          ? "有些步骤没有完成。可以看看上面的步骤列表，再决定要不要重试。"
                          : "运行被停止了，没有继续执行。")}
                  </p>
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
                      重新核对并继续
                    </button>
                  ) : null}
                </div>
              ) : null}
              {task?.state === "succeeded" &&
              task.source?.kind !== "savedStrategy" &&
              task.source?.kind !== "savedWorkflow" &&
              plan ? (
                <div className="strategy-save">
                  <span>
                    这次成功了。
                    {canReplay
                      ? "可以存下来，以后一键重跑，不用再问模型。"
                      : "可以存下来，以后手动重跑。"}
                  </span>
                  <button
                    className="runtime-action"
                    disabled={savingStrategy || strategySaved}
                    onClick={() => {
                      setSavingStrategy(true);
                      void api[
                        canReplay ? "extractWorkflow" : "extractStrategy"
                      ](task.id, plan.goal)
                        .then(reload)
                        .catch((e) => setError(String(e)))
                        .finally(() => setSavingStrategy(false));
                    }}
                  >
                    {savingStrategy
                      ? "正在保存…"
                      : strategySaved
                        ? "已保存到运行记录"
                        : "保存到运行记录"}
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        )}
      </div>

      <div className="composer-dock">
        {sendError ? (
          <div className="inline-error" role="alert">
            <strong>出错了</strong>
            <span>{sendError}</span>
          </div>
        ) : null}
        {queuedRuns.length ? (
          <section className="run-input-actions" aria-label="排队中的消息">
            <p className="muted">一次只能跑一条，这些在排队：</p>
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
                  取消这条
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
              title={prompt.trim() ? undefined : "先输入内容才能排队"}
              onClick={() => void send(true)}
            >
              排队，等这条跑完再发
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
              <StopIcon className="button-icon" />
              停止当前运行
            </button>
          </div>
        ) : null}
        <div className="command-deck">
          <textarea
            aria-label="给 Sleepy Doll 的消息"
            placeholder={
              task?.state === "awaitingUser"
                ? "回复它上面的问题…"
                : "告诉 Sleepy Doll 你想做什么…"
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
          <button
            type="button"
            className="primary-action send-action"
            disabled={sending || !prompt.trim()}
            title={prompt.trim() ? undefined : "先输入内容才能发送"}
            onClick={() => void send()}
          >
            <SendIcon className="button-icon" />
            {sending ? "发送中…" : busy ? "发送补充" : "发送"}
          </button>
        </div>
        <p className="composer-hint">按 Enter 发送，Shift + Enter 换行。</p>
      </div>
    </section>
  );
}
