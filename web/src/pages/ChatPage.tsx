import { useEffect, useRef, useState } from "react";
import mascot from "../assets/moon-character.png";
import "./ChatPage.css";
import { Transcript } from "../components/Transcript";
import { api } from "../api";
import { CheckIcon, SendIcon, StopIcon } from "../components/icons";
import { Select } from "../components/Select";
import {
  isRunning,
  phaseLabel,
  readError,
  session,
  taskLabels,
  useSession,
} from "../session";
import { Toast } from "../components/Toast";
import type { Bootstrap } from "../types";

interface Props {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  /** 新对话里先挑好的模型，随第一条消息生效。 */
  modelId?: string | null | undefined;
  onConversation(id: string): void;
  reload(): Promise<void>;
}
export function ChatPage({
  bootstrap,
  conversationId,
  modelId,
  onConversation,
  reload,
}: Props) {
  const data = useSession(conversationId);
  const { messages, task, stream, question, approval, plan, loading } = data;
  const busy = isRunning(task);
  const draftKey = `sleepy-doll-draft:${conversationId ?? "new"}`;
  const modeKey = `sleepy-doll-mode:${conversationId ?? "new"}`;
  const [mode, setMode] = useState<"agent" | "chatOnly">(() =>
    localStorage.getItem(modeKey) === "chatOnly" ? "chatOnly" : "agent",
  );
  const [prompt, setPrompt] = useState(
    () => localStorage.getItem(draftKey) ?? "",
  );
  const [notice, setNotice] = useState("");
  const [unread, setUnread] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [saving, setSaving] = useState(false);
  const [now, setNow] = useState(Date.now());
  const current = useRef(conversationId);
  current.current = conversationId;
  const alive = useRef(true);
  const scroll = useRef<HTMLDivElement>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const follow = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    setPrompt(localStorage.getItem(draftKey) ?? "");
    setMode(
      localStorage.getItem(modeKey) === "chatOnly" ? "chatOnly" : "agent",
    );
    setError("");
    setNotice("");
    setSending(false);
    follow.current = true;
  }, [draftKey, modeKey]);
  useEffect(() => {
    setConfirming(false);
  }, [approval?.id]);
  useEffect(() => {
    if (!busy && !approval) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [busy, approval]);
  useEffect(() => {
    if (follow.current) {
      scroll.current?.scrollTo({ top: scroll.current.scrollHeight });
      setUnread(false);
    } else {
      setUnread(true);
    }
  }, [messages, stream, plan, task]);
  useEffect(() => {
    if (textarea.current) {
      textarea.current.style.height = "auto";
      textarea.current.style.height =
        Math.min(textarea.current.scrollHeight, 180) + "px";
    }
  }, [prompt]);
  const setDraft = (value: string) => {
    setPrompt(value);
    localStorage.setItem(draftKey, value);
  };
  const send = async (queue = false) => {
    const value = prompt.trim();
    if (!value || sending) return;
    const origin = conversationId;
    const retryKey = `${draftKey}:pending`;
    let pending: { key: string; prompt: string; runId?: string } | undefined;
    try {
      pending =
        JSON.parse(sessionStorage.getItem(retryKey) ?? "null") ?? undefined;
    } catch {
      /* invalid saved state */
    }
    const clientKey =
      pending?.prompt === value ? pending.key : crypto.randomUUID();
    const supplementRun =
      pending?.prompt === value
        ? pending.runId
        : busy && task && !queue
          ? task.id
          : undefined;
    sessionStorage.setItem(
      retryKey,
      JSON.stringify({ key: clientKey, prompt: value, runId: supplementRun }),
    );
    setSending(true);
    follow.current = true;
    setError("");
    setNotice("");
    setDraft("");
    try {
      if (supplementRun) {
        await api.supplement(supplementRun, value, clientKey);
        // 已经提交出去的外部动作不会因为这句话被撤销，只能等到下一个边界。
        setNotice("已收到，将在当前步骤结束后处理。");
      } else {
        const run = await api.submitTask(
          value,
          origin,
          clientKey,
          mode,
          origin ? undefined : modelId,
        );
        session(run.conversationId).start();
        if (alive.current && current.current === origin)
          onConversation(run.conversationId);
        if (queue) setNotice("已加入队列，会在当前运行结束后开始。");
      }
      sessionStorage.removeItem(retryKey);
      await reload();
    } catch (reason) {
      localStorage.setItem(draftKey, value);
      if (alive.current && current.current === origin) {
        setPrompt(value);
        setError(readError(reason));
      }
    } finally {
      if (alive.current && current.current === origin) setSending(false);
    }
  };
  const act = async (action: () => Promise<unknown>) => {
    setError("");
    try {
      await action();
      await reload();
    } catch (reason) {
      setError(readError(reason));
    }
  };
  const welcome = !conversationId && !messages.length;
  const elapsed = task
    ? Math.max(0, Math.floor((now - new Date(task.createdAt).getTime()) / 1000))
    : 0;
  const phase = question || approval ? undefined : phaseLabel(task);
  return (
    <section className={`chat-workspace${welcome ? " is-welcome" : ""}`}>
      <div
        ref={scroll}
        className="chat-scroll"
        onScroll={(event) => {
          const target = event.currentTarget;
          follow.current =
            target.scrollHeight - target.scrollTop - target.clientHeight < 100;
          if (follow.current) setUnread(false);
        }}
      >
        {welcome ? (
          <div className="chat-welcome">
            <img
              className="welcome-mascot"
              src={mascot}
              alt="蜷坐在月亮上熟睡的木偶"
            />
            <h2>开始一项新任务</h2>
          </div>
        ) : (
          <div className="conversation-scene">
            {loading && !messages.length && <p className="muted">载入中…</p>}
            <div className="conversation-flow">
              <Transcript
                messages={messages}
                stream={stream}
                phase={phase}
                seconds={elapsed}
              />
              {plan && (
                <details className="run-plan">
                  <summary>执行计划 · {plan.steps.length} 步</summary>
                  <ol>
                    {plan.steps.map((step) => (
                      <li key={step.id}>
                        {step.title}
                        {step.outcome && (
                          <span className="muted">
                            {" "}
                            ·{" "}
                            {step.outcome === "active"
                              ? "进行中"
                              : step.outcome === "verifiedSucceeded"
                                ? "已完成"
                                : step.outcome}
                          </span>
                        )}
                      </li>
                    ))}
                  </ol>
                </details>
              )}
              {question && (
                <section className="run-question">
                  <h3>需要补充信息</h3>
                  <p>{question}</p>
                </section>
              )}
              {approval && (
                <section className="run-approval">
                  <h3>确认执行</h3>
                  <p>
                    {approval.request.binding?.description ??
                      approval.request.methodId}
                  </p>
                  <details>
                    <summary>操作参数</summary>
                    <pre>
                      {JSON.stringify(approval.request.arguments, null, 2)}
                    </pre>
                  </details>
                  <div className="detail-actions">
                    <button
                      className="primary-action"
                      disabled={confirming || now >= approval.expiresAt * 1000}
                      onClick={() => {
                        setConfirming(true);
                        void act(() => api.approve(approval.id, true)).finally(
                          () => setConfirming(false),
                        );
                      }}
                    >
                      允许
                    </button>
                    <button
                      className="secondary-action"
                      disabled={confirming || now >= approval.expiresAt * 1000}
                      onClick={() => {
                        setConfirming(true);
                        void act(() => api.approve(approval.id, false)).finally(
                          () => setConfirming(false),
                        );
                      }}
                    >
                      拒绝
                    </button>
                  </div>
                  {now >= approval.expiresAt * 1000 && (
                    <p className="muted">确认已过期</p>
                  )}
                </section>
              )}
              {task &&
                !busy &&
                ["failed", "cancelled", "needsReview", "partial"].includes(
                  task.state,
                ) && (
                  <section className="run-error">
                    <h3>{taskLabels[task.state]}</h3>
                    <p>{task.error || task.result}</p>
                    {task.state === "needsReview" && (
                      <button
                        className="secondary-action"
                        onClick={() => void act(() => api.resume(task.id))}
                      >
                        重新核对
                      </button>
                    )}
                  </section>
                )}
              {task?.state === "succeeded" && plan && (
                <button
                  className="subtle-action"
                  disabled={
                    saving ||
                    bootstrap.workflows.some(
                      (flow) =>
                        flow.lastRunId === task.id ||
                        flow.sourceConversationId === task.conversationId,
                    )
                  }
                  onClick={() => {
                    setSaving(true);
                    void act(() =>
                      plan.steps.every((step) => !!step.tool)
                        ? api.extractWorkflow(task.id, plan.goal)
                        : api.extractStrategy(task.id, plan.goal),
                    ).finally(() => setSaving(false));
                  }}
                >
                  <CheckIcon className="button-icon" />
                  保存为快捷任务
                </button>
              )}
            </div>
          </div>
        )}
      </div>
      {unread && busy && (
        <button
          className="chat-unread"
          onClick={() => {
            follow.current = true;
            setUnread(false);
            scroll.current?.scrollTo({ top: scroll.current.scrollHeight });
          }}
        >
          有新内容 · 回到最新
        </button>
      )}
      <div className="composer-dock">
        {notice && <Toast message={notice} onDismiss={() => setNotice("")} />}
        {(error || data.error) && (
          <Toast
            message={error || data.error}
            // 连接断了是个持续状态，不自动消失，免得用户还没看清就没了。
            duration={data.error ? 0 : 4000}
            action={{
              label: "刷新状态",
              onAction: () => {
                setError("");
                if (conversationId) session(conversationId).start();
                void reload();
              },
            }}
            onDismiss={() => setError("")}
          />
        )}
        {busy && (
          <p className="composer-hint">
            当前运行保持它开始时的模式（{task?.chatOnly ? "仅聊天" : "Agent"}
            ）； 切换只影响之后的发送，要立刻停下来请点停止。
          </p>
        )}
        {data.queued.length > 0 && (
          <div className="queued-list">
            {data.queued.map((run) => (
              <div key={run.id}>
                <span>排队中 · {run.prompt}</span>
                <button
                  className="subtle-action"
                  onClick={() => void act(() => api.cancelTask(run.id))}
                >
                  取消
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="command-deck">
          <textarea
            ref={textarea}
            rows={2}
            aria-label="消息"
            placeholder={
              question
                ? "回复…"
                : busy
                  ? "补充说明…（Enter 发送，Shift+Enter 换行）"
                  : "告诉我你想完成什么"
            }
            value={prompt}
            disabled={sending}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return;
              const modified =
                localStorage.getItem("sleepy-doll-send-key") === "modifier";
              if (
                event.key === "Enter" &&
                !event.shiftKey &&
                (modified
                  ? event.ctrlKey || event.metaKey
                  : !event.ctrlKey && !event.metaKey)
              ) {
                event.preventDefault();
                void send();
              }
            }}
          />
          <div className="composer-actions">
            <div className="composer-approval">
              <Select
                label="审批级别"
                value={bootstrap.permission.mode}
                options={bootstrap.permission.levels.map((level) => ({
                  value: level.value,
                  label: level.label,
                  description: level.description,
                }))}
                onChange={(mode) =>
                  void act(async () => {
                    await api.setPermission(mode);
                    await reload();
                  })
                }
              />
            </div>
            <div className="composer-mode" role="group" aria-label="对话模式">
              <button
                className="subtle-action"
                aria-pressed={mode === "agent"}
                title="可以使用工具，真正去读文件、改配置、跑任务"
                onClick={() => {
                  setMode("agent");
                  localStorage.setItem(modeKey, "agent");
                }}
              >
                Agent
              </button>
              <button
                className="subtle-action"
                aria-pressed={mode === "chatOnly"}
                title="只回答，不调用任何外部工具"
                onClick={() => {
                  setMode("chatOnly");
                  localStorage.setItem(modeKey, "chatOnly");
                }}
              >
                仅聊天
              </button>
            </div>
            <span>
              {busy && (
                <button
                  className="subtle-action"
                  disabled={!prompt.trim() || sending}
                  title="等当前运行结束后再开始"
                  onClick={() => void send(true)}
                >
                  排队发送
                </button>
              )}
            </span>
            <div className="composer-submit">
              {busy && (
                <button
                  type="button"
                  className="send-action"
                  aria-label="停止生成"
                  title="停止生成"
                  disabled={task?.state === "cancelling"}
                  onClick={() =>
                    task && void act(() => api.cancelTask(task.id))
                  }
                >
                  <StopIcon className="button-icon" />
                </button>
              )}
              {(!busy || prompt.trim()) && (
                <button
                  type="button"
                  className="send-action"
                  aria-label={busy ? "发送补充" : "发送"}
                  title={busy ? "发送补充" : "发送"}
                  disabled={sending || !prompt.trim()}
                  onClick={() => void send()}
                >
                  <SendIcon className="button-icon" />
                </button>
              )}
            </div>
          </div>
        </div>
        {welcome && (
          <div className="chat-examples">
            {["查看游戏状态", "查找可用路线"].map((example) => (
              <button
                key={example}
                onClick={() => {
                  setDraft(example);
                  textarea.current?.focus();
                }}
              >
                {example}
              </button>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
