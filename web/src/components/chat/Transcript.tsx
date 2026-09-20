import {
  Children,
  isValidElement,
  memo,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Components } from "react-markdown";
import { AlertIcon, CheckIcon, CopyIcon } from "../icons";
import { DisclosureChevron } from "../controls/DisclosureChevron";
import type { MessageInfo } from "../../ipc/types";
import "./Transcript.css";
import { useT } from "../../i18n";

type Call = NonNullable<MessageInfo["toolCalls"]>[number];
interface Activity extends Call {
  result?: string;
}
type Part =
  | { kind: "text"; text: string }
  | { kind: "reasoning"; text: string }
  | { kind: "stream"; text: string }
  | { kind: "activities"; activities: Activity[] };
export interface Turn {
  role: "user" | "assistant";
  parts: Part[];
  createdAt?: string;
}

/** 工具在界面上显示的名字。 */
type ToolLabels = Record<string, string>;

/** 参数里最能说明「动了哪个对象」的那个值。 */
const subjectKeys = [
  "path",
  "name",
  "query",
  "command",
  "methodId",
  "folderName",
  "id",
];

function subjectOf(args: unknown) {
  if (!args || typeof args !== "object") return "";
  const record = args as Record<string, unknown>;
  for (const key of subjectKeys) {
    const value = record[key];
    if (typeof value !== "string" || !value.trim()) continue;
    const text = value.trim().replace(/\s+/g, " ");
    return text.length > 56 ? `${text.slice(0, 55)}…` : text;
  }
  return "";
}

function pushReasoning(turn: Turn, text: string) {
  const existing = turn.parts.find((part) => part.kind === "reasoning");
  if (existing?.kind === "reasoning") {
    existing.text = `${existing.text}\n\n${text}`;
    return;
  }
  const anchor = turn.parts.findLastIndex(
    (part) => part.kind === "text" || part.kind === "stream",
  );
  const part = { kind: "reasoning" as const, text };
  // 插在第一条回答文本之前；没有文本时追加。part 顺序保持稳定，
  // 流式重挂载不再把答案顶得来回跳。
  if (anchor >= 0) turn.parts.splice(anchor, 0, part);
  else turn.parts.push(part);
}

/** 把消息组装成对话轮次。 */
export function buildTurns(messages: MessageInfo[], stream: string): Turn[] {
  const results = new Map(
    messages
      .filter((message) => message.role === "tool")
      .map((message) => [message.toolCallId, message.content]),
  );
  const turns: Turn[] = [];
  for (const message of messages) {
    if (message.role !== "user" && message.role !== "assistant") continue;
    const calls = (message.toolCalls ?? []).filter(
      (call) => !["plan.update", "user.ask"].includes(call.name),
    );
    // 只有推理的轮次也保留。
    const reasoning = message.reasoning?.text ?? "";
    if (!message.content && !calls.length && !reasoning) continue;
    let turn = turns.at(-1);
    if (!turn || turn.role !== message.role || message.role === "user") {
      const nextTurn: Turn = {
        role: message.role,
        parts: [],
        ...(message.createdAt ? { createdAt: message.createdAt } : {}),
      };
      turns.push(nextTurn);
      turn = nextTurn;
    } else if (message.createdAt) {
      turn.createdAt = message.createdAt;
    }
    if (reasoning) pushReasoning(turn, reasoning);
    if (message.content)
      turn.parts.push({ kind: "text", text: message.content });
    if (calls.length) {
      let part = turn.parts.at(-1);
      if (part?.kind !== "activities") {
        part = { kind: "activities", activities: [] };
        turn.parts.push(part);
      }
      part.activities.push(
        ...calls.map((call) => ({
          ...call,
          ...(results.has(call.id) ? { result: results.get(call.id)! } : {}),
        })),
      );
    }
  }
  if (stream) {
    let turn = turns.at(-1);
    if (turn?.role !== "assistant") {
      turn = { role: "assistant", parts: [] };
      turns.push(turn);
    }
    turn.parts.push({ kind: "stream", text: stream });
  }
  return turns;
}

export function turnCopyText(turn: Turn) {
  return turn.parts
    .flatMap((part) =>
      part.kind === "text" || part.kind === "stream" ? [part.text] : [],
    )
    .join("\n\n")
    .trim();
}

/** 补上未闭合的代码围栏。 */
export function stabilizeMarkdown(text: string) {
  const fences = text.match(/^```/gm)?.length ?? 0;
  return fences % 2 === 1 ? `${text}\n\`\`\`` : text;
}

function outcome(activity: Activity) {
  if (activity.result === undefined) return "running";
  try {
    return JSON.parse(activity.result).ok === false ? "failed" : "done";
  } catch {
    return "done";
  }
}

function ActivityGroup({
  activities,
  labels,
}: {
  activities: Activity[];
  labels: ToolLabels;
}) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const running = activities.some(
    (activity) => outcome(activity) === "running",
  );
  const failed = activities.some((activity) => outcome(activity) === "failed");
  const label = [
    ...new Set(
      activities.map((activity) => labels[activity.name] ?? activity.name),
    ),
  ]
    .slice(0, 3)
    .join(" · ");
  const subject =
    activities.length === 1 ? subjectOf(activities[0]!.arguments) : "";
  return (
    <section className="activity-group" data-expanded={expanded}>
      <button
        type="button"
        className="activity-summary"
        aria-expanded={expanded}
        onClick={() => setExpanded((open) => !open)}
      >
        {running ? (
          <span className="activity-spinner" />
        ) : failed ? (
          <AlertIcon className="activity-icon" />
        ) : (
          <CheckIcon className="activity-icon" />
        )}
        <span className="activity-label">{label}</span>
        {subject && <span className="activity-subject">{subject}</span>}
        {(running || failed) && (
          <span className="activity-outcome">
            {running ? t.chat.statusRunning : t.transcript.callFailed}
          </span>
        )}
        <DisclosureChevron expanded={expanded} className="activity-expand" />
      </button>
      <div
        className="activity-disclosure-motion"
        aria-hidden={!expanded}
        inert={!expanded}
      >
        <div className="activity-disclosure-inner">
          <div className="activity-detail">
            {activities.map((activity) => (
              <div className="activity-item" key={activity.id}>
                <strong>{labels[activity.name] ?? activity.name}</strong>
                <ActivityDetailDisclosure label={t.transcript.params}>
                  {JSON.stringify(activity.arguments, null, 2)}
                </ActivityDetailDisclosure>
                {activity.result && (
                  <ActivityDetailDisclosure label={t.transcript.returnData}>
                    {activity.result}
                  </ActivityDetailDisclosure>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

function ActivityDetailDisclosure({
  label,
  children,
}: {
  label: string;
  children: string;
}) {
  const [expanded, setExpanded] = useState(false);
  return (
    <section className="activity-subdetails" data-expanded={expanded}>
      <button
        type="button"
        className="activity-subsummary"
        aria-expanded={expanded}
        onClick={() => setExpanded((open) => !open)}
      >
        <DisclosureChevron expanded={expanded} />
        <span>{label}</span>
      </button>
      <div
        className="activity-disclosure-motion"
        aria-hidden={!expanded}
        inert={!expanded}
      >
        <div className="activity-disclosure-inner">
          <pre>{children}</pre>
        </div>
      </div>
    </section>
  );
}

/** 单轮的执行过程（思考与工具调用）：默认折叠，回答是主角。 */
function ProcessGroup({
  steps,
  running,
  children,
}: {
  steps: number;
  running: boolean;
  children: ReactNode;
}) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  return (
    <section className="activity-group process-group" data-expanded={expanded}>
      <button
        type="button"
        className="activity-summary"
        aria-expanded={expanded}
        onClick={() => setExpanded((open) => !open)}
      >
        <span className="activity-label">
          {running
            ? t.transcript.thinkingRunning
            : steps > 0
              ? t.transcript.thinkingSteps(steps)
              : t.transcript.thinking}
        </span>
        {running && <span className="activity-spinner" />}
        <DisclosureChevron expanded={expanded} className="activity-expand" />
      </button>
      <div
        className="activity-disclosure-motion"
        aria-hidden={!expanded}
        inert={!expanded}
      >
        <div className="activity-disclosure-inner">{children}</div>
      </div>
    </section>
  );
}

function fenceText(children: ReactNode) {
  const code = Children.toArray(children).find((child) =>
    isValidElement(child),
  );
  if (!isValidElement<{ children?: ReactNode; className?: string }>(code)) {
    return { text: "", lang: "" };
  }
  const lang = /language-(\S+)/.exec(code.props.className ?? "")?.[1] ?? "";
  const text = String(code.props.children ?? "").replace(/\n$/, "");
  return { text, lang };
}

function CodeFence({ children }: { children?: ReactNode }) {
  const t = useT();
  const { text, lang } = fenceText(children);
  return (
    <div className="md-fence">
      <div className="md-fence-bar">
        <span className="md-fence-lang">{lang || t.transcript.code}</span>
        <CopyButton text={text} />
      </div>
      <pre>{children}</pre>
    </div>
  );
}

const markdownComponents: Components = {
  pre({ children }) {
    return <CodeFence>{children}</CodeFence>;
  },
  a({ href, children }) {
    const external = Boolean(href && /^https?:/i.test(href));
    return (
      <a
        href={href}
        {...(external ? { target: "_blank", rel: "noreferrer noopener" } : {})}
      >
        {children}
      </a>
    );
  },
};

const MarkdownText = memo(function MarkdownText({
  text,
  streaming = false,
}: {
  text: string;
  streaming?: boolean;
}) {
  return (
    <Markdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
      {streaming ? stabilizeMarkdown(text) : text}
    </Markdown>
  );
});

function CopyButton({ text }: { text: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <button
      type="button"
      className="copy-action"
      aria-label={copied ? t.common.copied : t.common.copy}
      title={copied ? t.common.copied : t.common.copy}
      onClick={(event) => {
        event.stopPropagation();
        void navigator.clipboard?.writeText(text).then(() => {
          setCopied(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1600);
        });
      }}
    >
      {copied ? (
        <CheckIcon className="copy-action-icon" />
      ) : (
        <CopyIcon className="copy-action-icon" />
      )}
    </button>
  );
}

const messageTime = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

function formatMessageTime(value?: string) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? "" : messageTime.format(date);
}

export const Transcript = memo(function Transcript({
  messages,
  stream,
  phase,
  seconds,
  toolLabels,
}: {
  messages: MessageInfo[];
  stream: string;
  phase?: string | undefined;
  seconds: number;
  toolLabels: ToolLabels;
}) {
  const turns = buildTurns(messages, stream);
  const last = turns.at(-1);
  const pendingActivity =
    last?.role === "assistant" &&
    last.parts.some(
      (part) =>
        part.kind === "activities" &&
        part.activities.some((activity) => activity.result === undefined),
    );
  return (
    <>
      {turns.map((turn, index) => {
        const copy = turnCopyText(turn);
        const time = formatMessageTime(turn.createdAt);
        // 过程（思考与工具调用）收进一个默认折叠的组：对话里回答是主角，
        // 过程按需展开。整组一次性渲染，流式增量不再引发布局重排。
        const process = turn.parts.filter(
          (part) => part.kind === "reasoning" || part.kind === "activities",
        );
        const content = turn.parts.filter(
          (part) => part.kind === "text" || part.kind === "stream",
        );
        const processSteps = process.reduce(
          (count, part) =>
            count + (part.kind === "activities" ? part.activities.length : 1),
          0,
        );
        // 只随「这一轮是否还在推进」变化；跟单条工具结果走会来回抖。
        const turnActive =
          index === turns.length - 1 &&
          (Boolean(stream) || Boolean(phase) || pendingActivity);
        return (
          <article key={index} className={`message-turn ${turn.role}`}>
            <div className="message-content">
              {process.length > 0 && (
                <ProcessGroup
                  steps={processSteps}
                  running={turnActive}
                >
                  {process.map((part, partIndex) =>
                    part.kind === "reasoning" ? (
                      <div key={partIndex} className="reasoning-entry">
                        <pre className="reasoning-text">{part.text}</pre>
                      </div>
                    ) : (
                      <ActivityGroup
                        key={partIndex}
                        activities={part.activities}
                        labels={toolLabels}
                      />
                    ),
                  )}
                </ProcessGroup>
              )}
              {content.map((part, partIndex) =>
                part.kind === "text" ? (
                  <div
                    className={
                      turn.role === "user"
                        ? "user-message"
                        : "assistant-message"
                    }
                    key={partIndex}
                  >
                    <MarkdownText text={part.text} />
                  </div>
                ) : (
                  <div
                    className="assistant-message is-streaming"
                    key={partIndex}
                  >
                    <MarkdownText text={part.text} streaming />
                  </div>
                ),
              )}
              {turn.role === "assistant" &&
                index === turns.length - 1 &&
                phase &&
                !stream &&
                !pendingActivity && (
                  <div className="response-phase" role="status">
                    <span className="activity-spinner" />
                    {phase}
                    <time>{seconds}s</time>
                  </div>
                )}
            </div>
            {copy || time ? (
              <div
                className={`message-actions${turn.role === "user" ? " is-user" : ""}`}
              >
                {time && (
                  <time className="message-time" dateTime={turn.createdAt}>
                    {time}
                  </time>
                )}
                {copy && <CopyButton text={copy} />}
              </div>
            ) : null}
          </article>
        );
      })}
      {phase && last?.role !== "assistant" && (
        <article className="message-turn assistant">
          <div className="response-phase" role="status">
            <span className="activity-spinner" />
            {phase}
            <time>{seconds}s</time>
          </div>
        </article>
      )}
    </>
  );
});
