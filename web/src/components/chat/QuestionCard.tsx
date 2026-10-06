import { useEffect, useRef, useState } from "react";
import { CheckIcon, HelpIcon } from "../icons";
import { readError } from "../../session";
import { useT } from "../../i18n";
import { MarkdownText } from "./Transcript";
import type {
  QuestionAnswers,
  QuestionInfo,
  QuestionRequestInfo,
  RunApproval,
} from "../../ipc/types";
import { approvalPresentation, safeApprovalArguments } from "./approval-display";
import "./QuestionCard.css";

/** 自由文字的选择值：选择按选项下标结构化存储，-1 表示自由文字，
 *  不拿真实选项 label 当哨兵，避免与用户选项文本撞车。 */
const FREE_PICK = -1;

interface DraftState {
  clientKey: string;
  /** 最近一次提交的答复签名；内容没变才复用 clientKey，改了答案就换新 key。 */
  signature: string;
  index: number;
  collapsed: boolean;
  /** 题目 id → 选项下标（-1=自由文字）。 */
  picks: Record<string, number>;
  /** 题目 id → 自由文字。 */
  texts: Record<string, string>;
}

const draftKey = (
  conversationId: string | undefined,
  request: QuestionRequestInfo,
) =>
  `sleepy-doll-question:${conversationId ?? "new"}:${request.runId}:${request.requestId}`;

/** 只取自有属性，杜绝 __proto__/constructor 之类的原型链值混进草稿。 */
function sanitizeRecord<T extends string | number>(
  value: unknown,
  accept: (entry: unknown) => entry is T,
): Record<string, T> {
  const out = Object.assign(Object.create(null) as Record<string, T>);
  if (value && typeof value === "object")
    for (const [key, entry] of Object.entries(value as object))
      if (accept(entry)) Object.defineProperty(out, key, {
        value: entry,
        enumerable: true,
        writable: true,
        configurable: true,
      });
  return out;
}
const isString = (v: unknown): v is string => typeof v === "string";
const isFiniteNumber = (v: unknown): v is number =>
  typeof v === "number" && Number.isFinite(v);

function loadDraft(key: string): DraftState {
  try {
    const raw = sessionStorage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<DraftState>;
      if (typeof parsed.clientKey === "string")
        return {
          clientKey: parsed.clientKey,
          signature: typeof parsed.signature === "string" ? parsed.signature : "",
          index: typeof parsed.index === "number" ? parsed.index : 0,
          collapsed: parsed.collapsed === true,
          picks: sanitizeRecord(parsed.picks, isFiniteNumber),
          texts: sanitizeRecord(parsed.texts, isString),
        };
    }
  } catch {
    /* 存的内容不是合法 JSON。 */
  }
  return {
    clientKey: crypto.randomUUID(),
    signature: "",
    index: 0,
    collapsed: false,
    picks: Object.assign(Object.create(null)),
    texts: Object.assign(Object.create(null)),
  };
}

function ownGet<T>(record: Record<string, T>, key: string): T | undefined {
  return Object.prototype.hasOwnProperty.call(record, key)
    ? record[key]
    : undefined;
}

/** 单题当前的有效答复文本；没答完返回空串。
 *  选项题：选了选项按选项作答；直接输入文字（或没选）按文字作答。 */
function answerText(question: QuestionInfo, draft: DraftState): string {
  const text = (ownGet(draft.texts, question.id) ?? "").trim();
  if (!question.options?.length) return text;
  const pick = ownGet(draft.picks, question.id);
  if (pick !== undefined && pick !== FREE_PICK)
    // 下标必须落在真实选项里；越界的存量草稿按未答处理。
    return question.options[pick]?.label ?? "";
  return text;
}

/** 无原型污染风险的答复表：键来自后端题目 id，也防御 id=__proto__ 之类。 */
function buildAnswers(
  questions: QuestionInfo[],
  draft: DraftState,
): QuestionAnswers {
  return Object.fromEntries(
    questions.map((question) => [
      question.id,
      { answers: [answerText(question, draft)] },
    ]),
  );
}

function QuestionRequestCard({
  request,
  conversationId,
  onAnswer,
}: {
  request: QuestionRequestInfo;
  conversationId: string | undefined;
  onAnswer(
    request: QuestionRequestInfo,
    answers: QuestionAnswers,
    clientKey: string,
  ): Promise<void>;
}) {
  const t = useT();
  const storageKey = draftKey(conversationId, request);
  const [draft, setDraft] = useState<DraftState>(() => loadDraft(storageKey));
  const [sending, setSending] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState("");
  // inFlight 只挡并发；acked 只在收到 ACK 后置位。失败保留草稿，
  // 卸载/切换后再回来仍能按「内容没变同 key、改内容新 key」继续。
  const inFlight = useRef(false);
  const acked = useRef(false);
  useEffect(() => {
    if (acked.current) sessionStorage.removeItem(storageKey);
  }, [storageKey]);
  const patch = (change: Partial<DraftState>) => {
    if (acked.current) return;
    // 同步落盘后再 setState：不依赖渲染/updater 时序做持久化。
    const next = { ...draft, ...change };
    sessionStorage.setItem(storageKey, JSON.stringify(next));
    setDraft(next);
  };
  const questions = request.questions;
  const index = Math.min(Math.max(draft.index, 0), questions.length - 1);
  const question = questions[index];
  if (!question) return null;
  const complete = questions.every((entry) => answerText(entry, draft));
  const submit = async () => {
    if (!complete || inFlight.current || acked.current) return;
    const payload = buildAnswers(questions, draft);
    const signature = JSON.stringify(payload);
    // 同内容重试用原 key；改过答案换新 key，避免 same-key-different-content。
    const clientKey =
      draft.signature === signature ? draft.clientKey : crypto.randomUUID();
    inFlight.current = true;
    setSending(true);
    setError("");
    // 调用前同步保存本次签名与 key；只有成功 ACK 才删草稿、置 acked。
    patch({ clientKey, signature });
    try {
      await onAnswer(request, payload, clientKey);
      acked.current = true;
      setSubmitted(true);
      sessionStorage.removeItem(storageKey);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      inFlight.current = false;
      setSending(false);
    }
  };
  const advanceOrSubmit = () => {
    // 空答复 Enter 既不进下一题也不提交。
    if (!answerText(question, draft)) return;
    if (index < questions.length - 1) patch({ index: index + 1 });
    else void submit();
  };
  if (submitted || draft.collapsed)
    return (
      <button
        type="button"
        className="pending-request-chip"
        data-submitted={submitted}
        onClick={() => patch({ collapsed: false })}
      >
        {submitted ? (
          <CheckIcon className="button-icon" />
        ) : (
          <HelpIcon className="button-icon" />
        )}
        {submitted
          ? t.chat.replySent
          : `${t.chat.answerToContinue} · ${t.chat.questionProgress(questions.length)}`}
      </button>
    );
  const textChanged = (value: string) =>
    patch(
      question.options?.length
        ? // 选项题里输入文字即放弃已选选项，按文字作答。
          {
            texts: { ...draft.texts, [question.id]: value },
            picks: { ...draft.picks, [question.id]: FREE_PICK },
          }
        : { texts: { ...draft.texts, [question.id]: value } },
    );
  const freeform = (autoFocus = false) => (
    <textarea
      className="pending-request-freeform"
      aria-label={t.chat.writeReply}
      placeholder={t.chat.answerPlaceholder}
      autoFocus={autoFocus}
      value={ownGet(draft.texts, question.id) ?? ""}
      disabled={sending}
      onChange={(event) => textChanged(event.target.value)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return;
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          advanceOrSubmit();
        }
      }}
    />
  );
  return (
    <section className="pending-request-card" aria-label={t.chat.needInfo}>
      <header className="pending-request-header">
        <h3>{question.header || t.chat.needInfo}</h3>
        {questions.length > 1 && (
          <span className="pending-request-progress">
            {t.chat.questionProgress(questions.length, index + 1)}
          </span>
        )}
        <span className="pending-request-badge">{t.chat.waitingForReply}</span>
        <button
          type="button"
          className="pending-request-collapse"
          aria-label={t.chat.collapsePanel}
          onClick={() => patch({ collapsed: true })}
        >
          −
        </button>
      </header>
      <div className="pending-request-body">
        {/* MarkdownText 会渲染块级元素，容器必须是 div 而不是 p。 */}
        <div className="pending-request-text">
          <MarkdownText text={question.question} />
        </div>
        {question.options?.length ? (
          <div
            className="pending-request-options"
            role="radiogroup"
            aria-label={question.header}
          >
            {/* 文字输入在选项之前，填写入口始终一眼可见。 */}
            {freeform()}
            {question.options.map((option, optionIndex) => (
              <label
                key={option.label}
                className="pending-request-option"
                data-checked={draft.picks[question.id] === optionIndex}
              >
                <input
                  type="radio"
                  name={`${storageKey}:${question.id}`}
                  disabled={sending}
                  checked={ownGet(draft.picks, question.id) === optionIndex}
                  onChange={() =>
                    patch({
                      picks: { ...draft.picks, [question.id]: optionIndex },
                      // 选选项即作答：清掉旧自由文字，两种答案不并存。
                      texts: { ...draft.texts, [question.id]: "" },
                    })
                  }
                />
                <span className="pending-request-option-label">
                  {option.label}
                </span>
                {option.description && (
                  <span className="pending-request-option-description">
                    {option.description}
                  </span>
                )}
              </label>
            ))}
          </div>
        ) : (
          freeform()
        )}
      </div>
      {error && (
        <p className="pending-request-error" role="alert">
          {error}
        </p>
      )}
      <footer className="pending-request-footer">
        {questions.length > 1 && (
          <button
            type="button"
            className="secondary-action"
            disabled={index === 0 || sending}
            onClick={() => patch({ index: index - 1 })}
          >
            {t.chat.prevQuestion}
          </button>
        )}
        {index < questions.length - 1 && (
          <button
            type="button"
            className="secondary-action"
            disabled={!answerText(question, draft) || sending}
            onClick={() => patch({ index: index + 1 })}
          >
            {t.chat.nextQuestion}
          </button>
        )}
        <button
          type="button"
          className="primary-action pending-request-submit"
          disabled={!complete || sending}
          onClick={() => void submit()}
        >
          {sending ? t.chat.sendingReply : t.chat.sendReply}
        </button>
      </footer>
    </section>
  );
}

function ApprovalCard({
  approval,
  now,
  onDecide,
}: {
  approval: RunApproval;
  now: number;
  onDecide(approved: boolean): Promise<void>;
}) {
  const t = useT();
  const [confirming, setConfirming] = useState(false);
  const expired = now >= approval.expiresAt * 1000;
  const decide = (approved: boolean) => {
    setConfirming(true);
    void onDecide(approved).finally(() => setConfirming(false));
  };
  const presentation = approvalPresentation(approval.request);
  return (
    <section
      className="pending-request-card pending-approval-card"
      aria-label={presentation.title || t.chat.confirmExec}
    >
      <header className="pending-request-header">
        <h3>{presentation.title || t.chat.confirmExec}</h3>
        <span className="pending-request-badge">{t.chat.waitingApproval}</span>
      </header>
      <div className="pending-request-body">
        {/* summary/changes 由 helper 保证可读：presentation 优先，空则回退真实参数推导。 */}
        <div className="pending-request-text">{presentation.summary}</div>
        <dl className="pending-approval-changes">
          {presentation.changes.map((change, index) => (
            <div key={`${index}:${change.label}`} className="pending-approval-change">
              <dt>{change.label}</dt>
              <dd>{change.value}</dd>
            </div>
          ))}
        </dl>
        <details>
          <summary>{t.chat.approvalDetails}</summary>
          <pre>
            {JSON.stringify(safeApprovalArguments(approval.request.arguments), null, 2)}
          </pre>
        </details>
      </div>
      <footer className="pending-request-footer">
        <button
          className="secondary-action"
          disabled={confirming || expired}
          onClick={() => decide(false)}
        >
          {t.chat.deny}
        </button>
        <button
          className="primary-action pending-request-submit"
          disabled={confirming || expired}
          onClick={() => decide(true)}
        >
          {t.chat.allow}
        </button>
        {expired && <p className="muted">{t.chat.approvalExpired}</p>}
      </footer>
    </section>
  );
}

/**
 * 会话的待处理请求层：结构化问答与审批确认都在这里，作为独立 dock
 * 出现在 normal flow 里（chat-scroll 之后、composer 之前），不覆盖
 * 历史与输入框；高度超限在层内部滚动。
 */
export function PendingRequestLayer({
  requests,
  approval,
  now,
  conversationId,
  onAnswer,
  onDecide,
}: {
  requests: QuestionRequestInfo[];
  approval: RunApproval | undefined;
  now: number;
  conversationId: string | undefined;
  onAnswer(
    request: QuestionRequestInfo,
    answers: QuestionAnswers,
    clientKey: string,
  ): Promise<void>;
  onDecide(approval: RunApproval, approved: boolean): Promise<void>;
}) {
  const t = useT();
  if (!requests.length && !approval) return null;
  return (
    <div
      className="pending-request-layer"
      role="region"
      aria-label={t.chat.needInfo}
    >
      {approval && (
        <ApprovalCard
          approval={approval}
          now={now}
          onDecide={(approved) => onDecide(approval, approved)}
        />
      )}
      {requests.map((request) => (
        <QuestionRequestCard
          key={`${request.runId}:${request.requestId}`}
          request={request}
          conversationId={conversationId}
          onAnswer={onAnswer}
        />
      ))}
    </div>
  );
}
