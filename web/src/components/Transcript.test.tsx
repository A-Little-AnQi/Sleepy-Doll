import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Transcript, stabilizeMarkdown } from "./Transcript";
import type { MessageInfo } from "../types";

afterEach(cleanup);

function assistant(content: string): MessageInfo {
  return { role: "assistant", content };
}

it("closes a dangling fence so streaming markdown still renders as a block", () => {
  expect(stabilizeMarkdown("```ts\nconst a = 1")).toBe("```ts\nconst a = 1\n```");
  expect(stabilizeMarkdown("```ts\nconst a = 1\n```")).toBe("```ts\nconst a = 1\n```");
});

it("renders assistant markdown with a copy action and fenced copy", async () => {
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
  render(
    <Transcript
      messages={[
        assistant("## 标题\n\n- 一项\n\n```ts\nconst ok = true\n```"),
      ]}
      stream=""
      seconds={0}
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

it("renders streaming tokens as markdown instead of a clipped preview", () => {
  render(
    <Transcript
      messages={[]}
      stream={"**进行中**\n\n- 第一项"}
      phase="等待响应"
      seconds={1}
    />,
  );
  expect(document.querySelector(".stream-live")).toBeNull();
  expect(document.querySelector(".assistant-message.is-streaming")).toBeTruthy();
  expect(screen.getByRole("button", { name: "复制" })).toBeTruthy();
});
