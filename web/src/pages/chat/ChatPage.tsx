import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import mascot from "../../brand/mascot.webp";
import "./ChatPage.css";
import { Transcript } from "../../components/chat/Transcript";
import { api } from "../../ipc/api";
import {
  HelpIcon,
  SendIcon,
  SettingsIcon,
  StopIcon,
} from "../../components/icons";
import { Select } from "../../components/controls/Select";
import {
  isRunning,
  phaseLabel,
  readError,
  session,
  taskLabels,
  useSession,
} from "../../session";
import { Toast } from "../../components/overlay/Toast";
import type { Bootstrap } from "../../ipc/types";
import { MotionSwitch } from "../../components/controls/MotionSwitch";
import { DisclosureChevron } from "../../components/controls/DisclosureChevron";
import { resolveConversationModel } from "../../models";
import { ContextMeter } from "../../components/chat/ContextMeter";
import { ComposerDeck } from "../../components/chat/ComposerDeck";
import { ComposerField } from "../../components/chat/ComposerField";
import { estimateMessagesTokens } from "../../session/context-usage";
import { useT } from "../../i18n";
import type { Plan } from "../../session";
import { DEFAULT_CONTEXT_WINDOW } from "../../models/presets";

interface Props {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  onConversation(id: string): void;
  reload(): Promise<void>;
  onComposerDraft?(active: boolean): void;
  onOpenModels?(): void;
}

export function RunPlanCard({ plan }: { plan: Plan }) {
  const [expanded, setExpanded] = useState(false);
  const t = useT();
  return (
    <section className="run-plan-cluster" data-expanded={expanded}>
      <div className="run-plan">
        <button
          type="button"
          className="run-plan-summary"
          aria-expanded={expanded}
          onClick={() => setExpanded((open) => !open)}
        >
          <span>{t.chat.planSummary(plan.steps.length)}</span>
          <DisclosureChevron expanded={expanded} />
        </button>
        <div
          className="run-plan-motion"
          aria-hidden={!expanded}
          inert={!expanded}
        >
          <div className="run-plan-motion-inner">
            <ol>
              {plan.steps.map((step) => (
                <li key={step.id}>
                  {step.title}
                  {step.outcome && (
                    <span className="muted">
                      {" "}
                      ·{" "}
                      {step.outcome === "active"
                        ? t.chat.statusRunning
                        : step.outcome === "verifiedSucceeded"
                          ? t.chat.statusDone
                          : step.outcome}
                    </span>
                  )}
                </li>
              ))}
            </ol>
          </div>
        </div>
      </div>
    </section>
  );
}

export function ChatPage({
  bootstrap,
  conversationId,
  onConversation,
  reload,
  onComposerDraft,
  onOpenModels,
}: Props) {
  const t = useT();
  const data = useSession(conversationId);
  const {
    messages,
    task,
    stream,
    contextActivities,
    question,
    approval,
    plan,
    loading,
  } = data;
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
  const [stopping, setStopping] = useState(false);
  const interrupting = stopping || task?.state === "cancelling";
  // 工具名取自工具定义里的 label，没有 label 的不进表。
  const toolLabels = useMemo(
    () => ({
      ...bootstrap.runtimeToolLabels,
      ...Object.fromEntries(
        bootstrap.tools
          .filter((tool) => tool.label)
          .map((tool) => [tool.name, tool.label] as const),
      ),
    }),
    [bootstrap.tools, bootstrap.runtimeToolLabels],
  );
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
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
    (busy ? task?.contextWindow : undefined) ||
    bootstrap.models.find((entry) => entry.id === selectedModel)
      ?.contextWindow ||
    DEFAULT_CONTEXT_WINDOW;
  const contextUsed =
    task?.contextTokens ?? estimateMessagesTokens(messages, [stream, prompt]);
  const current = useRef(conversationId);
  current.current = conversationId;
  const alive = useRef(true);
  const scroll = useRef<HTMLDivElement>(null);
  const flow = useRef<HTMLDivElement>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const follow = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useLayoutEffect(() => {
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
    setStopping(false);
  }, [task?.id, busy]);
  useEffect(() => {
    if (!busy && !approval) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [busy, approval]);
  useLayoutEffect(() => {
    if (!flow.current) return;
    const observer = new ResizeObserver(() => {
      if (follow.current) {
        scroll.current?.scrollTo({ top: scroll.current.scrollHeight });
        setUnread(false);
      } else {
        setUnread(true);
      }
    });
    observer.observe(flow.current);
    return () => observer.disconnect();
  }, [draftKey, loading]);
  const setDraft = (value: string) => {
    setPrompt(value);
    localStorage.setItem(draftKey, value);
  };
  const send = async (queue = false) => {
    const value = prompt.trim();
    if (
      !value ||
      sending ||
      stopping ||
      task?.state === "cancelling" ||
      !bootstrap.models.length
    )
      return;
    const origin = conversationId;
    const retryKey = `${draftKey}:pending`;
    let pending: { key: string; prompt: string; runId?: string } | undefined;
    try {
      pending =
        JSON.parse(sessionStorage.getItem(retryKey) ?? "null") ?? undefined;
    } catch {
      /* 存的内容不是合法 JSON。 */
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
        // 补充说明在当前步骤结束后处理。
        if (alive.current && current.current === origin)
          setNotice(t.chat.queuedStep);
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
        if (queue && alive.current && current.current === origin)
          setNotice(t.chat.queued);
      }
      sessionStorage.removeItem(retryKey);
      // The server has accepted this prompt. A shell refresh failure must not
      // restore the draft and invite the same message to be sent a second time.
      await reload().catch((reason: unknown) => {
        if (alive.current && current.current === origin)
          setError(readError(reason));
      });
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
  const phase = interrupting
    ? "正在停止"
    : question || approval
      ? undefined
      : phaseLabel(task);
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
              target.scrollHeight - target.scrollTop - target.clientHeight <
              100;
            if (follow.current) setUnread(false);
          }}
        >
          {welcome ? (
            <div className="chat-welcome">
              <img
                className="welcome-mascot"
                src={mascot}
                alt={t.chat.mascotAlt}
              />
              <h2>{t.chat.startNew}</h2>
              {!bootstrap.models.length &&
                (onOpenModels ? (
                  <button
                    type="button"
                    className="subtle-action"
                    onClick={onOpenModels}
                  >
                    <SettingsIcon className="button-icon" />
                    {t.chat.addModelFirst}
                  </button>
                ) : (
                  <p className="muted">{t.chat.addModelFirst}</p>
                ))}
              <button
                type="button"
                className="subtle-action"
                onClick={() =>
                  window.dispatchEvent(
                    new CustomEvent("sleepy-doll:open-release-notes", {
                      detail: "guide",
                    }),
                  )
                }
              >
                <HelpIcon className="button-icon" />
                {t.account.help}
              </button>
            </div>
          ) : (
            <div className="conversation-scene">
              {loading && !messages.length && (
                <p className="muted">{t.common.loading}</p>
              )}
              <div ref={flow} className="conversation-flow">
                <Transcript
                  messages={messages}
                  stream={stream}
                  phase={phase}
                  seconds={elapsed}
                  running={busy}
                  toolLabels={toolLabels}
                  tasks={bootstrap.tasks}
                  currentTask={task}
                  contextActivities={contextActivities}
                />
                {plan && <RunPlanCard plan={plan} />}
                {question && (
                  <section className="run-question">
                    <h3>{t.chat.needInfo}</h3>
                    <p>{question}</p>
                  </section>
                )}
                {approval && (
                  <section className="run-approval">
                    <h3>{t.chat.confirmExec}</h3>
                    <p>
                      {approval.request.binding?.description ??
                        approval.request.methodId}
                    </p>
                    <details>
                      <summary>{t.chat.opParams}</summary>
                      <pre>
                        {JSON.stringify(approval.request.arguments, null, 2)}
                      </pre>
                    </details>
                    <div className="detail-actions">
                      <button
                        className="primary-action"
                        disabled={
                          confirming || now >= approval.expiresAt * 1000
                        }
                        onClick={() => {
                          setConfirming(true);
                          void act(() =>
                            api.approve(approval.id, true),
                          ).finally(() => setConfirming(false));
                        }}
                      >
                        允许执行
                      </button>
                      <button
                        className="secondary-action"
                        disabled={
                          confirming || now >= approval.expiresAt * 1000
                        }
                        onClick={() => {
                          setConfirming(true);
                          void act(() =>
                            api.approve(approval.id, false),
                          ).finally(() => setConfirming(false));
                        }}
                      >
                        拒绝执行
                      </button>
                    </div>
                    {now >= approval.expiresAt * 1000 && (
                      <p className="muted">{t.chat.approvalExpired}</p>
                    )}
                  </section>
                )}
                {/* 取消与待核对分别由对话记录标注。 */}
                {task &&
                  !busy &&
                  ["failed", "partial", "blocked"].includes(task.state) && (
                    <section className="run-error" role="status">
                      <h3>{taskLabels[task.state]}</h3>
                      <p>{task.error || task.result}</p>
                      {task.state === "blocked" && (
                        <div className="detail-actions">
                          <button
                            className="primary-action"
                            onClick={() =>
                              void act(() => api.resumeTask(task.id))
                            }
                          >
                            重试任务
                          </button>
                          <button
                            className="secondary-action"
                            onClick={() =>
                              void act(() => api.cancelTask(task.id))
                            }
                          >
                            停止任务
                          </button>
                        </div>
                      )}
                    </section>
                  )}
              </div>
            </div>
          )}
        </div>
      </MotionSwitch>
      {unread && (
        <div className="chat-unread-anchor">
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
        </div>
      )}
      <div className="composer-dock">
        {notice && <Toast message={notice} onDismiss={() => setNotice("")} />}
        {(error || data.error) && (
          <Toast
            message={error || data.error}
            // 连接断开是持续状态，提示不自动消失。
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
                  取消排队
                </button>
              </div>
            ))}
          </div>
        )}
        <ComposerDeck>
          <ComposerField
            ref={textarea}
            aria-label={t.chat.message}
            placeholder={
              question
                ? t.chat.composerPlaceholderReply
                : busy
                  ? t.chat.composerPlaceholderBusy
                  : t.chat.composerPlaceholderNew
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
                label={t.chat.approvalLevel}
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
                disabled={
                  !prompt.trim() ||
                  sending ||
                  stopping ||
                  task?.state === "cancelling"
                }
                title={t.chat.waitCurrentRun}
                onClick={() => void send(true)}
              >
                {t.chat.queuedSend}
              </button>
            )}
            <div className="composer-submit">
              <ContextMeter
                used={contextUsed}
                window={contextWindow}
                cacheHit={
                  task?.promptCacheHit ? (task.promptCacheHitTokens ?? 0) : 0
                }
              />
              <div className="composer-menu composer-model">
                <Select
                  label={t.chat.model}
                  value={selectedModel}
                  disabled={sending || !bootstrap.models.length}
                  options={bootstrap.models.map((model) => ({
                    value: model.id,
                    label: model.name,
                    description: model.model,
                  }))}
                  footer={
                    onOpenModels
                      ? {
                          label: t.chat.configureModels,
                          onClick: () => onOpenModels(),
                        }
                      : undefined
                  }
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
                  aria-label={interrupting ? "正在停止" : t.chat.stop}
                  title={interrupting ? "正在停止" : t.chat.stop}
                  aria-busy={interrupting}
                  disabled={interrupting}
                  data-stopping={interrupting}
                  onClick={() => {
                    if (!task || stopping) return;
                    setStopping(true);
                    setError("");
                    void api
                      .cancelTask(task.id)
                      .then(reload)
                      .catch((reason: unknown) => {
                        if (
                          !alive.current ||
                          current.current !== task.conversationId
                        )
                          return;
                        setStopping(false);
                        setError(readError(reason));
                      });
                  }}
                >
                  {interrupting ? (
                    <span className="activity-spinner" aria-hidden="true" />
                  ) : (
                    <StopIcon className="button-icon" />
                  )}
                </button>
              )}
              {(!busy || prompt.trim()) && (
                <button
                  type="button"
                  className="send-action"
                  aria-label={busy ? t.chat.sendFollowUp : t.chat.send}
                  title={
                    !bootstrap.models.length
                      ? t.chat.addModelFirst
                      : busy
                        ? t.chat.sendFollowUp
                        : t.chat.send
                  }
                  disabled={
                    sending ||
                    stopping ||
                    task?.state === "cancelling" ||
                    !prompt.trim() ||
                    !bootstrap.models.length
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
