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
import type { MessageInfo, TaskInfo } from "../../ipc/types";
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
  runId?: string;
  /** 轮次首条消息的时刻，只在开轮时写入。 */
  createdAt?: string;
  /** 轮次内最新一条消息的时刻，随消息推进。 */
  lastAt?: string;
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
  // 每条消息的思考放在该消息正文之前，保留跨消息的真实执行顺序。
  turn.parts.push({ kind: "reasoning", text });
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
        ...(message.runId ? { runId: message.runId } : {}),
        ...(message.createdAt ? { createdAt: message.createdAt } : {}),
      };
      turns.push(nextTurn);
      turn = nextTurn;
    }
    if (!turn.runId && message.runId) turn.runId = message.runId;
    if (message.createdAt) turn.lastAt = message.createdAt;
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
  active,
}: {
  activities: Activity[];
  labels: ToolLabels;
  active: boolean;
}) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const pending = activities.some(
    (activity) => outcome(activity) === "running",
  );
  const running = active && pending;
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
        ) : failed || pending ? (
          <AlertIcon className="activity-icon" />
        ) : (
          <CheckIcon className="activity-icon" />
        )}
        <span className="activity-label">{label}</span>
        {subject && <span className="activity-subject">{subject}</span>}
        {(pending || failed) && (
          <span className="activity-outcome">
            {running
              ? t.chat.statusRunning
              : pending
                ? "未收到结果"
                : t.transcript.callFailed}
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

/** 运行总用时。task 缺席时退回轮次首尾消息的间隔。 */
function elapsedLabel(
  turn: Turn,
  task: TaskInfo | undefined,
  t: ReturnType<typeof useT>,
): string | undefined {
  const started = new Date(task?.createdAt ?? turn.createdAt ?? "").getTime();
  const finished = new Date(task?.updatedAt ?? turn.lastAt ?? "").getTime();
  if (!Number.isFinite(started) || !Number.isFinite(finished)) return undefined;
  const elapsed = Math.max(0, Math.round((finished - started) / 1000));
  if (elapsed <= 0) return undefined;
  const minutes = Math.floor(elapsed / 60);
  const seconds = elapsed % 60;
  return elapsed < 60
    ? t.transcript.turnElapsedS(elapsed)
    : elapsed >= 3600
      ? t.transcript.turnElapsedHms(
          Math.floor(elapsed / 3600),
          Math.floor(minutes % 60),
          seconds,
        )
      : t.transcript.turnElapsedM(minutes, seconds);
}

/** 轮次结束后把思考与工具调用收进一行；说明文字与回复正文留在时间线上。 */
function ProcessDisclosure({
  label,
  elapsed,
  children,
}: {
  label: string;
  elapsed?: string | undefined;
  children: ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  return (
    <section className="process-disclosure" data-expanded={expanded}>
      <button
        type="button"
        className="process-disclosure-summary"
        aria-expanded={expanded}
        onClick={() => setExpanded((open) => !open)}
      >
        <span className="process-disclosure-label">{label}</span>
        {elapsed && <time>{elapsed}</time>}
        <DisclosureChevron expanded={expanded} className="process-caret" />
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

/** 思考状态行：只报进度，思考原文不进界面。 */
function ProcessGroup({
  steps,
  running,
  stopping,
}: {
  steps: number;
  running: boolean;
  stopping: boolean;
}) {
  const t = useT();
  return (
    <div className="process-status" role="status">
      {stopping
        ? "正在停止"
        : running
          ? t.transcript.thinkingRunning
          : steps > 0
            ? t.transcript.thinkingSteps(steps)
            : t.transcript.thinking}
    </div>
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
  tasks = [],
  currentTask,
  running = Boolean(phase),
}: {
  messages: MessageInfo[];
  stream: string;
  phase?: string | undefined;
  seconds: number;
  toolLabels: ToolLabels;
  tasks?: TaskInfo[];
  currentTask?: TaskInfo | undefined;
  running?: boolean;
}) {
  const t = useT();
  const turns = buildTurns(messages, stream);
  // 每个运行只在它的最后一轮收尾处显示一次总用时。
  const isRunLast = turns.map(() => false);
  const seenRuns = new Set<string>();
  for (let i = turns.length - 1; i >= 0; i -= 1) {
    const runId = turns[i]!.runId;
    if (!runId || seenRuns.has(runId)) continue;
    seenRuns.add(runId);
    isRunLast[i] = true;
  }
  const last = turns.at(-1);
  return (
    <>
      {turns.map((turn, index) => {
        const copy = turnCopyText(turn);
        const time = formatMessageTime(turn.createdAt);
        const turnActive =
          turn.role === "assistant" && index === turns.length - 1 && running;
        // 说明文字常驻时间线；思考与工具默认收进「过程」一行，运行中的那几条留在行外。
        type TextPart = Extract<Part, { kind: "text" | "stream" }>;
        const terminalPart = turn.parts.at(-1);
        const finalTextIndex =
          !turnActive &&
          (terminalPart?.kind === "text" || terminalPart?.kind === "stream")
            ? turn.parts.length - 1
            : -1;
        type Block =
          | { kind: "reasoning"; text: string }
          | { kind: "tool"; activity: Activity }
          | { kind: "commentary"; parts: TextPart[] };
        const blocks: Block[] = [];
        for (let i = 0; i < turn.parts.length; i += 1) {
          const part = turn.parts[i];
          if (!part) continue;
          if (i === finalTextIndex) continue;
          const tail = blocks.at(-1);
          if (part.kind === "reasoning") {
            blocks.push({ kind: "reasoning", text: part.text });
          } else if (part.kind === "activities") {
            for (const activity of part.activities)
              blocks.push({ kind: "tool", activity });
          } else if (tail?.kind === "commentary") {
            tail.parts.push(part);
          } else {
            blocks.push({ kind: "commentary", parts: [part] });
          }
        }
        const hasProcess = blocks.length > 0;
        const finalText =
          finalTextIndex >= 0 ? turn.parts[finalTextIndex] : undefined;
        const task =
          tasks.find((task) => task.id === turn.runId) ??
          (currentTask?.id === turn.runId ? currentTask : undefined);
        type ProcessBlock = Exclude<Block, { kind: "commentary" }>;
        const blockDone = (block: ProcessBlock) =>
          block.kind !== "tool" || outcome(block.activity) !== "running";
        // 折叠行只列这段实际调用的工具；思考原文不进界面。
        const segmentLabel = (process: ProcessBlock[]) => {
          const names: string[] = [];
          for (const block of process) {
            if (block.kind !== "tool") continue;
            const name = toolLabels[block.activity.name] ?? block.activity.name;
            if (!names.includes(name)) names.push(name);
          }
          const label = names.slice(0, 3).join(" · ");
          return process.filter((block) => block.kind === "tool").length > 3
            ? `${label} …`
            : label;
        };
        // 说明文字常驻时间线，并把过程按它分段：两次说明之间的思考与工具
        // 收成一行，运行中正在进行的那几条留在行外。
        type Segment = { commentary?: Block; process: ProcessBlock[] };
        const segments: Segment[] = [];
        let current: Segment = { process: [] };
        for (const block of blocks) {
          if (block.kind === "commentary") {
            segments.push(current);
            current = { commentary: block, process: [] };
          } else {
            current.process.push(block);
          }
        }
        segments.push(current);
        const lastSegment = segments.at(-1);
        const renderBlock = (block: ProcessBlock) =>
          block.kind === "reasoning" ? (
            <ProcessGroup
              key="thinking"
              steps={1}
              running={turnActive}
              stopping={turnActive && phase === "正在停止"}
            />
          ) : (
            <ActivityGroup
              key={block.activity.id}
              activities={[block.activity]}
              labels={toolLabels}
              active={turnActive}
            />
          );
        return (
          <article key={index} className={`message-turn ${turn.role}`}>
            <div className="message-content">
              {segments.map((segment, segmentIndex) => {
                const isLast = segment === lastSegment;
                // 运行中：最后一段里未完成的块（以及正在思考）保持平铺。
                const live =
                  turnActive &&
                  isLast &&
                  segment.process.some((block) => !blockDone(block));
                const visibleBlocks = live
                  ? segment.process.filter((block) => !blockDone(block))
                  : [];
                // 折叠行只收工具调用；思考不占界面。
                const archivedBlocks = (live
                  ? segment.process.filter((block) => blockDone(block))
                  : segment.process
                ).filter((block) => block.kind === "tool");
                const elapsed =
                  !turnActive &&
                  isLast &&
                  isRunLast[index] &&
                  archivedBlocks.length > 0
                    ? elapsedLabel(turn, task, t)
                    : undefined;
                return (
                  <div key={segmentIndex} className="turn-segment">
                    {segment.commentary?.kind === "commentary" && (
                      <div
                        className={`assistant-message${segment.commentary.parts.some((part) => part.kind === "stream") ? " is-streaming" : ""}`}
                      >
                        <MarkdownText
                          text={segment.commentary.parts
                            .map((part) => part.text)
                            .join("\n\n")}
                          streaming={segment.commentary.parts.some(
                            (part) => part.kind === "stream",
                          )}
                        />
                      </div>
                    )}
                    {visibleBlocks.map(renderBlock)}
                    {archivedBlocks.length > 0 && (
                      <ProcessDisclosure
                        label={segmentLabel(archivedBlocks)}
                        elapsed={elapsed}
                      >
                        {archivedBlocks.map((block) => (
                          <ActivityGroup
                            key={block.activity.id}
                            activities={[block.activity]}
                            labels={toolLabels}
                            active={false}
                          />
                        ))}
                      </ProcessDisclosure>
                    )}
                  </div>
                );
              })}
              {finalText?.kind === "text" ? (
                <div
                  className={
                    turn.role === "user" ? "user-message" : "assistant-message"
                  }
                  data-final={turn.role === "assistant" && hasProcess}
                >
                  <MarkdownText text={finalText.text} />
                </div>
              ) : finalText?.kind === "stream" ? (
                <div
                  className={`assistant-message${running ? " is-streaming" : ""}`}
                >
                  <MarkdownText text={finalText.text} streaming />
                </div>
              ) : null}
              {turn.role === "assistant" &&
                index === turns.length - 1 &&
                phase &&
                !stream &&
                !hasProcess && (
                  <div className="response-phase" role="status">
                    {phase}
                    <time>{seconds}s</time>
                  </div>
                )}
            </div>
            {/* 运行结束前不出时间和复制；结束后悬停整行都能唤出。 */}
            {!running && (copy || time) ? (
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
            {phase}
            <time>{seconds}s</time>
          </div>
        </article>
      )}
    </>
  );
});
