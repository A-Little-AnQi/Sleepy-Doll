import { useEffect, useRef, useState } from "react";
import mascot from "../assets/moon-character.webp";
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
import { MotionSwitch } from "../components/MotionSwitch";
import { resolveConversationModel } from "../models";
import { ContextMeter } from "../components/ContextMeter";
import { ComposerDeck } from "../components/ComposerDeck";
import { ComposerField } from "../components/ComposerField";
import { estimateMessagesTokens } from "../context-usage";

interface Props {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  onConversation(id: string): void;
  reload(): Promise<void>;
  onComposerDraft?(active: boolean): void;
  onOpenHelp?(): void;
}
export function ChatPage({
  bootstrap,
  conversationId,
  onConversation,
  reload,
  onComposerDraft,
  onOpenHelp,
}: Props) {
  const data = useSession(conversationId);
  const { messages, task, stream, question, approval, plan, loading } = data;
  const busy = isRunning(task);
  const draftKey = `sleepy-doll-draft:${conversationId ?? "new"}`;
  const [prompt, setPrompt] = useState(
    () => localStorage.getItem(draftKey) ?? "",
  );
  const [promptKey, setPromptKey] = useState(draftKey);
  if (promptKey !== draftKey) {
    setPromptKey(draftKey);
    setPrompt(localStorage.getItem(draftKey) ?? "");
  }
  const [notice, setNotice] = useState("");
  const [unread, setUnread] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [saving, setSaving] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [pendingModel, setPendingModel] = useState<string | null>(null);
  const conversation = bootstrap.conversations.find(
    (entry) => entry.id === conversationId,
  );
  const selectedModel = resolveConversationModel(
    bootstrap.models,
    conversation,
    pendingModel,
  );
  const contextWindow =
    task?.contextWindow ||
    bootstrap.models.find((entry) => entry.id === selectedModel)
      ?.contextWindow ||
    200_000;
  const contextUsed =
    task?.contextTokens ??
    estimateMessagesTokens(messages, [stream, prompt]);
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
    setError("");
    setNotice("");
    setSending(false);
    setPendingModel(null);
    follow.current = true;
  }, [draftKey]);
  useEffect(() => {
    onComposerDraft?.(!conversationId && prompt.trim().length > 0);
  }, [conversationId, prompt, onComposerDraft]);
  useEffect(() => {
    return () => onComposerDraft?.(false);
  }, [onComposerDraft]);
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
  const setDraft = (value: string) => {
    setPrompt(value);
    localStorage.setItem(draftKey, value);
  };
  const send = async (queue = false) => {
    const value = prompt.trim();
    if (!value || sending || !bootstrap.models.length) return;
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
          origin ? undefined : selectedModel,
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
      <MotionSwitch
        viewKey={conversationId ?? "new"}
        className="chat-scene-switch"
      >
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
            {!bootstrap.models.length && (
              <p className="muted">先在设置里添加模型服务</p>
            )}
            {onOpenHelp && (
              <button type="button" className="subtle-action" onClick={onOpenHelp}>
                使用说明
              </button>
            )}
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
      </MotionSwitch>
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
            onDismiss={() => setError("")}
          />
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
        <ComposerDeck>
          <ComposerField
            ref={textarea}
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
            <div className="composer-menu composer-approval">
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
            <div className="composer-submit">
              <ContextMeter
                used={contextUsed}
                window={contextWindow}
                compacted={Boolean(task?.contextCompacted)}
                cacheRead={task?.cacheReadTokens ?? 0}
              />
              <div className="composer-menu composer-model">
                <Select
                  label="模型"
                  value={selectedModel}
                  disabled={sending || !bootstrap.models.length}
                  options={bootstrap.models.map((model) => ({
                    value: model.id,
                    label: model.name,
                    description: model.model,
                  }))}
                  onChange={(id) => {
                    if (!conversationId) {
                      setPendingModel(id);
                      return;
                    }
                    void act(async () => {
                      await api.setConversationModel(conversationId, id);
                    });
                  }}
                />
              </div>
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
                  title={
                    !bootstrap.models.length
                      ? "先在设置里添加模型"
                      : busy
                        ? "发送补充"
                        : "发送"
                  }
                  disabled={
                    sending || !prompt.trim() || !bootstrap.models.length
                  }
                  onClick={() => void send()}
                >
                  <SendIcon className="button-icon" />
                </button>
              )}
            </div>
          </div>
        </ComposerDeck>
      </div>
    </section>
  );
}
