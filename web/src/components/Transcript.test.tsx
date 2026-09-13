import { afterEach, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Transcript, buildTurns } from "./Transcript";
import type { MessageInfo } from "../types";

afterEach(cleanup);
const messages: MessageInfo[] = [
  { role: "user", content: "查看状态" },
  {
    role: "assistant",
    content: "",
    toolCalls: [{ id: "one", name: "bgi.state.get", arguments: {} }],
  },
  { role: "tool", content: '{"ok":true,"value":{}}', toolCallId: "one" },
  {
    role: "assistant",
    content: "",
    toolCalls: [{ id: "two", name: "skills.read", arguments: { name: "bgi" } }],
  },
  {
    role: "tool",
    content: '{"ok":true,"value":"instructions"}',
    toolCallId: "two",
  },
  { role: "assistant", content: "已读取状态。" },
];

it("combines adjacent calls and associates results without duplicate result cards", () => {
  const turns = buildTurns(messages, "");
  expect(turns).toHaveLength(2);
  expect(turns[1]?.parts).toHaveLength(2);
  const part = turns[1]?.parts[0];
  expect(part?.kind).toBe("activities");
  if (part?.kind === "activities") {
    expect(part.activities).toHaveLength(2);
    expect(part.activities[0]?.result).toContain('"ok":true');
    expect(part.activities[1]?.result).toContain("instructions");
  }
  const { container } = render(
    <Transcript messages={messages} stream="" seconds={0} />,
  );
  expect(container.querySelectorAll(".message-identity")).toHaveLength(1);
  expect(container.querySelectorAll(".activity-group")).toHaveLength(1);
  expect(container.querySelector(".activity-group")?.hasAttribute("open")).toBe(
    false,
  );
  expect(container.querySelectorAll(".tool-result")).toHaveLength(0);
});

it("keeps the same assistant turn while streaming grows", () => {
  const history = messages.slice(0, -1);
  const { container, rerender } = render(
    <Transcript
      messages={history}
      stream="正在"
      phase="等待响应"
      seconds={1}
    />,
  );
  const identity = container.querySelector(".message-identity");
  rerender(
    <Transcript
      messages={history}
      stream="正在逐段回复"
      phase="等待响应"
      seconds={2}
    />,
  );
  expect(container.querySelector(".message-identity")).toBe(identity);
  expect(container.querySelector(".assistant-message")?.textContent).toBe(
    "正在逐段回复",
  );
  expect(container.querySelector(".response-phase")).toBeNull();
});

it("shows reasoning collapsed above the answer", () => {
  const withReasoning: MessageInfo[] = [
    { role: "user", content: "看看状态" },
    {
      role: "assistant",
      content: "状态正常。",
      reasoning: {
        protocol: "anthropic-messages",
        blocks: [{ type: "thinking", thinking: "先读取状态", signature: "s" }],
        text: "先读取状态",
      },
    },
  ];
  const parts = buildTurns(withReasoning, "")[1]?.parts ?? [];
  expect(parts.map((part) => part.kind)).toEqual(["reasoning", "text"]);

  const { container } = render(
    <Transcript messages={withReasoning} stream="" seconds={0} />,
  );
  const group = container.querySelector(".reasoning-group");
  expect(container.querySelectorAll(".reasoning-group")).toHaveLength(1);
  expect(group?.hasAttribute("open")).toBe(false);
  expect(group?.querySelector("summary")?.textContent).toContain("思考过程");
  expect(group?.textContent).toContain("先读取状态");
});

it("drops reasoning that has no displayable text", () => {
  const empty: MessageInfo[] = [
    { role: "user", content: "看看状态" },
    {
      role: "assistant",
      content: "状态正常。",
      reasoning: { protocol: "anthropic-messages", blocks: [], text: "" },
    },
  ];
  expect(buildTurns(empty, "")[1]?.parts.map((part) => part.kind)).toEqual([
    "text",
  ]);
  const { container } = render(
    <Transcript messages={empty} stream="" seconds={0} />,
  );
  expect(container.querySelectorAll(".reasoning-group")).toHaveLength(0);
});

it("keeps a turn that produced only reasoning", () => {
  const onlyReasoning: MessageInfo[] = [
    { role: "user", content: "看看状态" },
    {
      role: "assistant",
      content: "",
      reasoning: { protocol: "anthropic-messages", blocks: [], text: "在想" },
    },
  ];
  expect(buildTurns(onlyReasoning, "")[1]?.parts.map((part) => part.kind)).toEqual(
    ["reasoning"],
  );
});
