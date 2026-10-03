import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
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
import { SkillMenu } from "../../components/chat/SkillMenu";
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
import type { Bootstrap, QuestionAnswers, QuestionRequestInfo, RunApproval } from "../../ipc/types";
import { MotionSwitch } from "../../components/controls/MotionSwitch";
import { DisclosureChevron } from "../../components/controls/DisclosureChevron";
import { resolveConversationModel } from "../../models";
import { ContextMeter } from "../../components/chat/ContextMeter";
import { ComposerDeck } from "../../components/chat/ComposerDeck";
import { ComposerField } from "../../components/chat/ComposerField";
import { PendingRequestLayer } from "../../components/chat/QuestionCard";
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
    questionRequests,
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
  const [unread, setUnread] = useState(false);
  const [sending, setSending] = useState(false);
  const inputId = useId();
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
  const [now, setNow] = useState(Date.now());
  // 浮层可用高度跟随 composer 实际几何：短窗口/大输入框时不会越出顶部。
  const [layerMax, setLayerMax] = useState<number | undefined>(undefined);
  const [layerCenter, setLayerCenter] = useState<number | undefined>(undefined);
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
  const dock = useRef<HTMLDivElement>(null);
  const workspace = useRef<HTMLElement>(null);
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
    setStopping(false);
  }, [task?.id, busy]);
  useEffect(() => {
    if (!busy && !approval && !questionRequests.length) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [busy, approval, questionRequests]);
  useLayoutEffect(() => {
    const node = dock.current;
    if (!node) return;
    const update = () => {
      // 浮层锚在 dock 上方 10px；可用高度是 dock 上沿到 workspace 顶部的
      // 实际空间（再留 12px 上边距），没有下限撑出，tiny 空间靠内部滚动。
      const dockTop = node.getBoundingClientRect().top;
      const workspaceTop =
        workspace.current?.getBoundingClientRect().top ?? 0;
      const space = dockTop - workspaceTop;
      setLayerMax(Math.max(0, Math.floor(space - 24)));
      // 浮层垂直中心落在可用区中点 (workspace.top+dock.top)/2：
      // dock 内坐标即 -space/2（0 是有效值）。
      setLayerCenter(-Math.floor(space) / 2);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    if (workspace.current) observer.observe(workspace.current);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, []);
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
  const invocableSkills = useMemo(
    () =>
      bootstrap.skills.filter(
        (skill) => skill.enabled !== false && skill.available !== false,
      ),
    [bootstrap.skills],
  );
  // 技能菜单把 `$技能名` 插到光标处；运行时按这个前缀显式装载该技能。
  const insertSkill = (name: string) => {
    const marker = `$${name} `;
    const node = textarea.current;
    if (!node) {
      setDraft(marker + prompt);
      return;
    }
    const at = node.selectionStart ?? prompt.length;
    setDraft(prompt.slice(0, at) + marker + prompt.slice(at));
    requestAnimationFrame(() => {
      node.focus();
      const caret = at + marker.length;
      node.setSelectionRange(caret, caret);
    });
  };
  // 主消息只负责发起新请求；运行中的补充由独立问答面板或后台交接处理，
  // 输入框在运行期间禁用但保留草稿，交接结束自动恢复。
  const send = async () => {
    const value = prompt.trim();
    if (
      !value ||
      sending ||
      busy ||
      stopping ||
      task?.state === "cancelling" ||
      !bootstrap.models.length
    )
      return;
    const origin = conversationId;
    const retryKey = `${draftKey}:pending`;
    let pending: { key: string; prompt: string } | undefined;
    try {
      pending =
        JSON.parse(sessionStorage.getItem(retryKey) ?? "null") ?? undefined;
    } catch {
      /* 存的内容不是合法 JSON。 */
    }
    const clientKey =
      pending?.prompt === value ? pending.key : crypto.randomUUID();
    sessionStorage.setItem(retryKey, JSON.stringify({ key: clientKey, prompt: value }));
    setSending(true);
    follow.current = true;
    setError("");
    setDraft("");
    try {
      const run = await api.submitTask(
        value,
        origin,
        clientKey,
        origin ? undefined : selectedModel,
      );
      session(run.conversationId).start();
      if (alive.current && current.current === origin)
        onConversation(run.conversationId);
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
    : question || questionRequests.length || approval
      ? undefined
      : phaseLabel(task);
  // 结构化答复走专用 RPC；旧事件（无 journal 记录）保持 run.input 兼容。
  const answerQuestion = async (
    request: QuestionRequestInfo,
    answers: QuestionAnswers,
    clientKey: string,
  ) => {
    const origin = conversationId;
    if (request.legacy) {
      const text = answers.answer?.answers[0] ?? "";
      if (!text) throw new Error(t.chat.writeReply);
      await api.supplement(request.runId, text, clientKey);
    } else {
      await api.answerQuestion(request.runId, request.requestId, answers, clientKey);
    }
    // ACK 成功：在发起答复的那个会话里立即终结该请求，迟到的事件/快照
    // 都不能复活它；回调按 origin 归属，不写当前恰好打开的别的聊天。
    if (origin) session(origin).resolveQuestionRequest(request.runId, request.requestId);
  };
  const decideApproval = async (target: RunApproval, approved: boolean) => {
    setError("");
    try {
      await api.approve(target.id, approved);
      await reload();
    } catch (reason) {
      setError(readError(reason));
    }
  };
  return (
    <section
      ref={workspace}
      className={`chat-workspace${welcome ? " is-welcome" : ""}`}
    >
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
                {/* 取消与失败分别由对话记录标注。 */}
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
      <div className="composer-dock" ref={dock}>
        <PendingRequestLayer
          requests={questionRequests}
          approval={approval}
          now={now}
          conversationId={conversationId}
          {...(layerMax != null ? { maxHeight: layerMax } : {})}
          {...(layerCenter != null ? { centerTop: layerCenter } : {})}
          onAnswer={answerQuestion}
          onDecide={decideApproval}
        />
        {(error || data.error) && (
          <Toast
            message={error || data.error}
            // 连接断开是持续状态，提示不自动消失。
            duration={data.error ? 0 : 4000}
            onDismiss={() => setError("")}
          />
        )}
        <ComposerDeck>
          <ComposerField
            id={inputId}
            ref={textarea}
            aria-label={t.chat.message}
            placeholder={t.chat.composerPlaceholderNew}
            value={prompt}
            disabled={sending || busy}
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
            <SkillMenu
              skills={invocableSkills}
              disabled={sending || busy}
              onPick={insertSkill}
            />
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
              {/* 运行期间只有停止；发送按钮仅用于发起新请求。 */}
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
              {!busy && (
                <button
                  type="button"
                  className="send-action"
                  aria-label={t.chat.send}
                  title={
                    !bootstrap.models.length
                      ? t.chat.addModelFirst
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
