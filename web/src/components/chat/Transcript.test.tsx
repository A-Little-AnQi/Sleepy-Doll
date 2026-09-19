import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Transcript, stabilizeMarkdown } from "./Transcript";
import type { MessageInfo } from "../../ipc/types";

afterEach(cleanup);

function assistant(content: string): MessageInfo {
  return { role: "assistant", content };
}

function call(name: string, args: unknown): MessageInfo {
  return {
    role: "assistant",
    content: "",
    toolCalls: [{ id: `${name}-1`, name, arguments: args }],
  };
}

it("closes a dangling fence so streaming markdown still renders as a block", () => {
  expect(stabilizeMarkdown("```ts\nconst a = 1")).toBe(
    "```ts\nconst a = 1\n```",
  );
  expect(stabilizeMarkdown("```ts\nconst a = 1\n```")).toBe(
    "```ts\nconst a = 1\n```",
  );
});

it("renders assistant markdown with a copy action and fenced copy", async () => {
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
  render(
    <Transcript
      messages={[assistant("## 标题\n\n- 一项\n\n```ts\nconst ok = true\n```")]}
      stream=""
      seconds={0}
      toolLabels={{}}
    />,
  );
  expect(screen.getByRole("heading", { name: "标题" })).toBeTruthy();
  expect(screen.getByText("一项")).toBeTruthy();
  expect(screen.getByText("ts")).toBeTruthy();
  const copies = screen.getAllByRole("button", { name: "复制" });
  expect(copies.length).toBeGreaterThan(1);
  fireEvent.click(copies[0]!);
  expect(navigator.clipboard.writeText).toHaveBeenCalled();
});

it("names a tool call the way its own definition does", () => {
  const { container } = render(
    <Transcript
      messages={[call("bgi.user.read", { path: "ScriptGroup/每日.json" })]}
      stream=""
      seconds={0}
      toolLabels={{ "bgi.user.read": "读取配置文件" }}
    />,
  );
  expect(container.querySelector(".activity-label")?.textContent).toBe(
    "读取配置文件",
  );
  expect(container.querySelector(".activity-subject")?.textContent).toBe(
    "ScriptGroup/每日.json",
  );
});

it("falls back to the tool name when a tool has no label of its own", () => {
  const { container } = render(
    <Transcript
      messages={[call("demo.echo", {})]}
      stream=""
      seconds={0}
      toolLabels={{}}
    />,
  );
  expect(container.querySelector(".activity-label")?.textContent).toBe(
    "demo.echo",
  );
  expect(container.querySelector(".activity-subject")).toBeNull();
});

it("renders streaming tokens as markdown instead of a clipped preview", () => {
  render(
    <Transcript
      messages={[]}
      stream={"**进行中**\n\n- 第一项"}
      phase="等待响应"
      seconds={1}
      toolLabels={{}}
    />,
  );
  expect(document.querySelector(".stream-live")).toBeNull();
  expect(
    document.querySelector(".assistant-message.is-streaming"),
  ).toBeTruthy();
  expect(screen.getByRole("button", { name: "复制" })).toBeTruthy();
});
