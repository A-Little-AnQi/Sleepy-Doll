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
import { SkillMenu, filterSkills } from "../../components/chat/SkillMenu";
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
import type {
  Bootstrap,
  QuestionAnswers,
  QuestionRequestInfo,
  RunApproval,
} from "../../ipc/types";
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

// 从光标往回找 `/查询词` 的起点；`/` 必须在文本空白分界之后，
// token 里再出现别的 `/`（URL、路径）就不算命令，返回 -1。
function slashTokenStart(value: string, caret: number): number {
  for (let i = Math.min(caret, value.length) - 1; i >= 0; i -= 1) {
    const char = value.charAt(i);
    if (char === "/") {
      if (i === 0 || /\s/.test(value.charAt(i - 1))) return i;
      return -1;
    }
    if (/\s/.test(char)) return -1;
  }
  return -1;
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
  // `/` 菜单：start 是输入框里 `/` 的下标，query 是 `/` 到光标之间的筛选词。
  const [slash, setSlash] = useState<{
    start: number;
    query: string;
    index: number;
  } | null>(null);
  useEffect(() => {
    setSlash(null);
  }, [draftKey]);
  const updateSlash = (value: string, caret: number) => {
    if (sending || busy || !invocableSkills.length) {
      setSlash(null);
      return;
    }
    const start = slashTokenStart(value, caret);
    if (start < 0) {
      setSlash(null);
      return;
    }
    const query = value.slice(start + 1, caret);
    // 同一段 token 的同词不重置高亮；只有 token 或查询词变了才回到第一项。
    setSlash((prev) =>
      prev && prev.start === start && prev.query === query
        ? prev
        : { start, query, index: 0 },
    );
  };
  // 技能菜单把 `$技能名` 插到光标处；运行时按这个前缀显式装载该技能。
  const insertSkill = (name: string) => {
    const marker = `$${name} `;
    const node = textarea.current;
    if (!node) {
      setDraft(marker + prompt);
      return;
    }
    const at = node.selectionStart ?? prompt.length;
    // 光标停在还没关掉的 `/查询词` 上时，+ 按钮选同一技能也走替换，别留下 "/$"。
    const token = slashTokenStart(prompt, at);
    if (token >= 0) {
      const query = prompt.slice(token + 1, at);
      if (
        filterSkills(invocableSkills, query).some(
          (skill) => skill.name === name,
        )
      ) {
        const caret = token + marker.length;
        setDraft(prompt.slice(0, token) + marker + prompt.slice(at));
        requestAnimationFrame(() => {
          node.focus();
          node.setSelectionRange(caret, caret);
        });
        return;
      }
    }
    setDraft(prompt.slice(0, at) + marker + prompt.slice(at));
    requestAnimationFrame(() => {
      node.focus();
      const caret = at + marker.length;
      node.setSelectionRange(caret, caret);
    });
  };
  // slash 菜单选中：只替换 `/查询词` 这一段，前后文字与光标位置都保留。
  const applySlashPick = (name: string) => {
    if (!slash) return;
    const node = textarea.current;
    const caret = node?.selectionStart ?? prompt.length;
    const marker = `$${name} `;
    const at = slash.start;
    setSlash(null);
    setDraft(prompt.slice(0, at) + marker + prompt.slice(caret));
    requestAnimationFrame(() => {
      node?.focus();
      node?.setSelectionRange(at + marker.length, at + marker.length);
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
    sessionStorage.setItem(
      retryKey,
      JSON.stringify({ key: clientKey, prompt: value }),
    );
    setSending(true);
    setSlash(null);
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
  // 等待用户作答/确认：busy 保持（不能发新消息），但要明说在等什么，
  // 取消入口仍可用，文案换成「取消任务」。
  const waitingForUser = Boolean(
    question || questionRequests.length || approval,
  );
  // 等待时把交互区可用高度写进 CSS 变量：min(320px, 工作区高度 50%,
  // 扣除 composer/等待行/padding 并给历史至少留 100px)。只算高度，无循环。
  useLayoutEffect(() => {
    const ws = workspace.current;
    const dk = dock.current;
    if (!ws || !dk || !waitingForUser) return;
    const update = () => {
      const wsH = ws.getBoundingClientRect().height;
      const dkH = dk.getBoundingClientRect().height;
      const waitH =
        ws.querySelector(".composer-waiting")?.getBoundingClientRect().height ??
        0;
      const dkPad = parseFloat(getComputedStyle(dk).paddingTop) || 0;
      const avail = wsH - dkH - waitH - dkPad - 100;
      const max = Math.max(
        0,
        Math.min(320, Math.floor(wsH * 0.5), Math.floor(avail)),
      );
      ws.style.setProperty("--pending-request-max-height", `${max}px`);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(ws);
    observer.observe(dk);
    return () => {
      observer.disconnect();
      ws.style.removeProperty("--pending-request-max-height");
    };
  }, [waitingForUser]);
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
      await api.answerQuestion(
        request.runId,
        request.requestId,
        answers,
        clientKey,
      );
    }
    // ACK 成功：在发起答复的那个会话里立即终结该请求，迟到的事件/快照
    // 都不能复活它；回调按 origin 归属，不写当前恰好打开的别的聊天。
    if (origin)
      session(origin).resolveQuestionRequest(request.runId, request.requestId);
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
                  waiting={
                    waitingForUser
                      ? approval
                        ? t.chat.composerWaitingApproval
                        : t.chat.composerWaitingQuestion
                      : undefined
                  }
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
      {/* 等待期间明示在等什么；ContextMeter（上下文用量）照常保留，不画 spinner。 */}
      {waitingForUser && (
        <p className="composer-waiting" role="status">
          {approval
            ? t.chat.composerWaitingApproval
            : t.chat.composerWaitingQuestion}
        </p>
      )}
      {/* 问答/审批独立 dock：与 composer 同宽，不覆盖历史与输入框。 */}
      <div className="pending-request-dock">
        <PendingRequestLayer
          requests={questionRequests}
          approval={approval}
          now={now}
          conversationId={conversationId}
          onAnswer={answerQuestion}
          onDecide={decideApproval}
        />
      </div>
      <div className="composer-dock" ref={dock} data-waiting={waitingForUser}>
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
            placeholder={
              waitingForUser
                ? t.chat.composerPlaceholderWaiting
                : t.chat.composerPlaceholderNew
            }
            value={prompt}
            disabled={sending || busy}
            onChange={(event) => {
              setDraft(event.target.value);
              const node = event.target;
              updateSlash(node.value, node.selectionStart ?? node.value.length);
            }}
            onKeyDown={(event) => {
              // IME 组合中的按键（含 keyCode 229）一律不拦截。
              if (event.nativeEvent.isComposing || event.keyCode === 229)
                return;
              const modified =
                localStorage.getItem("sleepy-doll-send-key") === "modifier";
              // 菜单选择永远是纯 Enter；发送快捷键才跟随用户偏好。
              const menuEnter =
                event.key === "Enter" &&
                !event.shiftKey &&
                !event.ctrlKey &&
                !event.metaKey &&
                !event.altKey;
              const plainEnter =
                event.key === "Enter" &&
                !event.shiftKey &&
                (modified
                  ? event.ctrlKey || event.metaKey
                  : !event.ctrlKey && !event.metaKey);
              if (slash) {
                const items = filterSkills(invocableSkills, slash.query);
                const index = items.length
                  ? Math.min(slash.index, items.length - 1)
                  : -1;
                // 只有看得到匹配项时才截获方向键，普通移动不抢。
                if (
                  (event.key === "ArrowDown" || event.key === "ArrowUp") &&
                  items.length
                ) {
                  event.preventDefault();
                  const next =
                    (index +
                      (event.key === "ArrowDown" ? 1 : -1) +
                      items.length) %
                    items.length;
                  setSlash({ ...slash, index: next });
                  return;
                }
                // 光标移出这段 token 就收起，避免菜单挂在过期的查询词上。
                if (
                  event.key === "ArrowLeft" ||
                  event.key === "ArrowRight" ||
                  event.key === "Home" ||
                  event.key === "End" ||
                  event.key === "PageUp" ||
                  event.key === "PageDown"
                ) {
                  setSlash(null);
                  return;
                }
                if (event.key === "Escape") {
                  setSlash(null);
                  return;
                }
                if (menuEnter && items.length) {
                  event.preventDefault();
                  const choice = items[index];
                  if (choice) applySlashPick(choice.name);
                  return;
                }
              }
              if (plainEnter) {
                event.preventDefault();
                void send();
              }
            }}
            onSelect={(event) => {
              const node = event.currentTarget;
              if ((node.selectionStart ?? 0) !== (node.selectionEnd ?? 0)) {
                setSlash(null);
                return;
              }
              updateSlash(node.value, node.selectionStart ?? node.value.length);
            }}
          />
          <div className="composer-actions">
            <SkillMenu
              skills={invocableSkills}
              disabled={sending || busy}
              onPick={insertSkill}
              slash={slash}
              onSlashPick={applySlashPick}
              onSlashClose={() => setSlash(null)}
              anchorElement={textarea.current}
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
                  aria-label={
                    interrupting
                      ? "正在停止"
                      : waitingForUser
                        ? t.chat.cancelTask
                        : t.chat.stop
                  }
                  title={
                    interrupting
                      ? "正在停止"
                      : waitingForUser
                        ? t.chat.cancelTask
                        : t.chat.stop
                  }
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
