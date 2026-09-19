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
import {
  AlertIcon,
  BrandIcon,
  CheckIcon,
  ChevronIcon,
  CopyIcon,
} from "../icons";
import type { MessageInfo } from "../../ipc/types";
import "./Transcript.css";

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
}

const labels: Record<string, string> = {
  "bgi.state.get": "读取游戏状态",
  "bgi.user.resolve": "查找可运行任务",
  "bgi.user.list": "查看用户资源",
  "bgi.user.read": "读取配置文件",
  "bgi.user.write": "写入配置文件",
  "bgi.user.inspect_script": "读取脚本说明",
  "bgi.api.search": "检索宿主接口",
  "bgi.api.describe": "读取接口说明",
  "bgi.api.read": "读取宿主状态",
  "bgi.api.invoke": "执行宿主操作",
  "bgi.capability.search": "检索插件能力",
  "bgi.capability.describe": "读取能力定义",
  "bgi.capability.invoke": "执行游戏操作",
  "bgi.job.get": "查询执行结果",
  "skills.read": "读取技能",
  "skills.reference": "读取技能说明",
  "skills.search": "检索技能",
  "plugins.list": "读取插件列表",
  "tools.search": "检索工具",
  "resource.search": "查找资源",
  "workspace.list": "查看软件目录",
  "workspace.read": "读取软件目录文件",
  "workspace.write": "写入软件目录文件",
  "workspace.delete": "删除软件目录文件",
  "workspace.shell": "执行 PowerShell",
};

function pushReasoning(turn: Turn, text: string) {
  const existing = turn.parts.find((part) => part.kind === "reasoning");
  if (existing?.kind === "reasoning") {
    existing.text = `${existing.text}\n\n${text}`;
    return;
  }
  turn.parts.unshift({ kind: "reasoning", text });
}

/** Results are attached by call ID, never displayed as a second independent card. */
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
    // 纯推理轮也要显示，否则整轮被静默丢掉。
    const reasoning = message.reasoning?.text ?? "";
    if (!message.content && !calls.length && !reasoning) continue;
    let turn = turns.at(-1);
    if (!turn || turn.role !== message.role || message.role === "user") {
      turn = { role: message.role, parts: [] };
      turns.push(turn);
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

/** Close a dangling fence so streaming markdown still paints as a code block. */
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

function ActivityGroup({ activities }: { activities: Activity[] }) {
  const running = activities.some(
    (activity) => outcome(activity) === "running",
  );
  const failed = activities.some((activity) => outcome(activity) === "failed");
  const label =
    activities.length === 1
      ? (labels[activities[0]!.name] ?? activities[0]!.name)
      : [
          ...new Set(
            activities.map(
              (activity) => labels[activity.name] ?? activity.name,
            ),
          ),
        ]
          .slice(0, 3)
          .join(" · ");
  return (
    <details className="activity-group">
      <summary>
        {running ? (
          <span className="activity-spinner" />
        ) : failed ? (
          <AlertIcon className="activity-check" />
        ) : (
          <CheckIcon className="activity-check" />
        )}
        <span className="activity-label">{label}</span>
        <span className="activity-outcome">
          {running ? "进行中" : failed ? "调用失败" : "已返回"}
        </span>
        <ChevronIcon className="activity-expand" />
      </summary>
      <div className="activity-detail">
        {activities.map((activity) => (
          <div className="activity-item" key={activity.id}>
            <strong>{labels[activity.name] ?? activity.name}</strong>
            <details>
              <summary>参数</summary>
              <pre>{JSON.stringify(activity.arguments, null, 2)}</pre>
            </details>
            {activity.result && (
              <details>
                <summary>返回数据</summary>
                <pre>{activity.result}</pre>
              </details>
            )}
          </div>
        ))}
      </div>
    </details>
  );
}

/** 默认折叠，与工具调用明细一致。推理文本通常很长，不该占满正文。 */
function ReasoningDisclosure({ text }: { text: string }) {
  return (
    <details className="activity-group reasoning-group">
      <summary>
        <span className="activity-label">思考过程</span>
        <ChevronIcon className="activity-expand" />
      </summary>
      <div className="activity-detail">
        <pre className="reasoning-text">{text}</pre>
      </div>
    </details>
  );
}

function AssistantIdentity() {
  return (
    <div className="message-identity">
      <span className="assistant-avatar">
        <BrandIcon />
      </span>
      <span>Sleepy Doll</span>
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
  const { text, lang } = fenceText(children);
  return (
    <div className="md-fence">
      <div className="md-fence-bar">
        <span className="md-fence-lang">{lang || "代码"}</span>
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
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <button
      type="button"
      className="copy-action"
      aria-label={copied ? "已复制" : "复制"}
      title={copied ? "已复制" : "复制"}
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

export const Transcript = memo(function Transcript({
  messages,
  stream,
  phase,
  seconds,
}: {
  messages: MessageInfo[];
  stream: string;
  phase?: string | undefined;
  seconds: number;
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
        return (
          <article key={index} className={`message-turn ${turn.role}`}>
            {turn.role === "assistant" && <AssistantIdentity />}
            <div className="message-content">
              {turn.parts.map((part, partIndex) =>
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
                ) : part.kind === "reasoning" ? (
                  <ReasoningDisclosure key={partIndex} text={part.text} />
                ) : part.kind === "stream" ? (
                  <div
                    className="assistant-message is-streaming"
                    key={partIndex}
                  >
                    <MarkdownText text={part.text} streaming />
                  </div>
                ) : (
                  <ActivityGroup key={partIndex} activities={part.activities} />
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
            {copy ? (
              <div
                className={`message-actions${turn.role === "user" ? " is-user" : ""}`}
              >
                <CopyButton text={copy} />
              </div>
            ) : null}
          </article>
        );
      })}
      {phase && last?.role !== "assistant" && (
        <article className="message-turn assistant">
          <AssistantIdentity />
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
