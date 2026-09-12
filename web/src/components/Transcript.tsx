import { memo, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { AlertIcon, BrandIcon, CheckIcon, ChevronIcon } from "./icons";
import type { MessageInfo } from "../types";
import "./Transcript.css";

type Call = NonNullable<MessageInfo["toolCalls"]>[number];
interface Activity extends Call {
  result?: string;
}
type Part =
  | { kind: "text"; text: string }
  | { kind: "activities"; activities: Activity[] };
export interface Turn {
  role: "user" | "assistant";
  parts: Part[];
}

const labels: Record<string, string> = {
  "bgi.state.get": "读取游戏状态",
  "bgi.capability.search": "检索可用能力",
  "bgi.capability.describe": "读取能力定义",
  "bgi.capability.invoke": "执行游戏操作",
  "bgi.job.get": "查询执行结果",
  "skills.read": "读取技能",
  "skills.search": "检索技能",
  "plugins.list": "读取插件列表",
  "tools.search": "检索工具",
  "resource.search": "查找资源",
};

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
    if (!message.content && !calls.length) continue;
    let turn = turns.at(-1);
    if (!turn || turn.role !== message.role || message.role === "user") {
      turn = { role: message.role, parts: [] };
      turns.push(turn);
    }
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
    turn.parts.push({ kind: "text", text: stream });
  }
  return turns;
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

const MarkdownText = memo(function MarkdownText({ text }: { text: string }) {
  return <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>;
});

function CopyReply({ text }: { text: string }) {
  const [label, setLabel] = useState("复制");
  return (
    <button
      className="reply-copy"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(
          () => setLabel("已复制"),
          () => setLabel("复制失败"),
        );
      }}
    >
      {label}
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
      {turns.map((turn, index) => (
        <article key={index} className={`message-turn ${turn.role}`}>
          {turn.role === "assistant" && <AssistantIdentity />}
          <div className="message-content">
            {turn.parts.map((part, partIndex) =>
              part.kind === "text" ? (
                <div
                  className={
                    turn.role === "user" ? "user-message" : "assistant-message"
                  }
                  key={partIndex}
                >
                  <MarkdownText text={part.text} />
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
          {turn.role === "assistant" &&
            (index < turns.length - 1 || !phase) &&
            turn.parts.some((part) => part.kind === "text") && (
              <CopyReply
                text={turn.parts
                  .flatMap((part) => (part.kind === "text" ? [part.text] : []))
                  .join("\n\n")}
              />
            )}
        </article>
      ))}
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
