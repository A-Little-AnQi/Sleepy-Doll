import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("../../brand/mascot.webp", () => ({ default: "mascot.png" }));
vi.mock("../../ipc/api", () => ({
  api: {
    conversation: vi.fn(),
    events: vi.fn(),
    setConversationModel: vi.fn(),
    setPermission: vi.fn(),
  },
}));

import { ChatPage, RunPlanCard } from "./ChatPage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../../ipc/types";

beforeAll(() => {
  Element.prototype.scrollTo = vi.fn();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});

const bootstrap: Bootstrap = {
  configPath: "/tmp/config.json",
  models: [
    {
      id: "primary",
      name: "主模型",
      protocol: "anthropic-messages",
      model: "test-model",
      baseUrl: "http://127.0.0.1/v1",
      active: true,
      timeoutMs: 120000,
    },
    {
      id: "gemini",
      name: "Gemini",
      protocol: "gemini",
      model: "test-model",
      baseUrl: "http://127.0.0.1/v1",
      active: false,
    },
  ],
  skills: [],
  plugins: [],
  tools: [],
  conversations: [],
  tasks: [],
  strategies: [],
  workflows: [],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  permission: PREVIEW_PERMISSION,
  bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1" },
};

it("uses the shared animated disclosure for the in-conversation plan", () => {
  const save = vi.fn();
  const { container } = render(
    <RunPlanCard
      plan={{
        goal: "完成测试路线",
        steps: [
          {
            id: "step-1",
            title: "运行测试路线",
            tool: "bgi.route.run",
            outcome: "verifiedSucceeded",
          },
        ],
      }}
      save={{ disabled: false, onClick: save }}
    />,
  );
  const summary = screen.getByRole("button", { name: /执行计划/ });
  const motion = container.querySelector(".run-plan-motion") as HTMLElement;
  expect(summary.querySelector(".sd-chevron")).toBeTruthy();
  expect(motion.getAttribute("aria-hidden")).toBe("true");
  fireEvent.click(summary);
  expect(motion.getAttribute("aria-hidden")).toBe("false");
  fireEvent.click(screen.getByRole("button", { name: "保存为快捷任务" }));
  expect(save).toHaveBeenCalledOnce();
});

it("puts the model picker in the composer instead of following a default option", () => {
  render(
    <ChatPage
      bootstrap={bootstrap}
      onConversation={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByRole("combobox", { name: "模型" })).toBeTruthy();
  expect(screen.getByRole("combobox", { name: "模型" }).textContent).toContain(
    "主模型",
  );
  expect(screen.queryByText("跟随默认模型")).toBeNull();
  expect(screen.getByText("0 / 200k")).toBeTruthy();
});

it("blocks sending until a model is configured", () => {
  render(
    <ChatPage
      bootstrap={{ ...bootstrap, models: [] }}
      onConversation={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByText("先在设置里添加模型服务")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "使用说明" })).toBeNull();
  fireEvent.change(screen.getByRole("textbox", { name: "消息" }), {
    target: { value: "帮我看看" },
  });
  expect(
    (screen.getByRole("button", { name: "发送" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("opens the product guide from the welcome screen", () => {
  const onOpenHelp = vi.fn();
  render(
    <ChatPage
      bootstrap={bootstrap}
      onConversation={() => undefined}
      reload={async () => undefined}
      onOpenHelp={onOpenHelp}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "使用说明" }));
  expect(onOpenHelp).toHaveBeenCalled();
});

it("does not seed the welcome composer with host-specific prompts", () => {
  render(
    <ChatPage
      bootstrap={bootstrap}
      onConversation={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.queryByRole("button", { name: "查看游戏状态" })).toBeNull();
  expect(screen.queryByRole("button", { name: "查找可用路线" })).toBeNull();
});

it("tells the shell when a new-chat composer has text", () => {
  const onComposerDraft = vi.fn();
  const { unmount } = render(
    <ChatPage
      bootstrap={bootstrap}
      onConversation={() => undefined}
      reload={async () => undefined}
      onComposerDraft={onComposerDraft}
    />,
  );
  expect(onComposerDraft).toHaveBeenCalledWith(false);
  fireEvent.change(screen.getByRole("textbox", { name: "消息" }), {
    target: { value: "帮我看看" },
  });
  expect(onComposerDraft).toHaveBeenLastCalledWith(true);
  fireEvent.change(screen.getByRole("textbox", { name: "消息" }), {
    target: { value: "  " },
  });
  expect(onComposerDraft).toHaveBeenLastCalledWith(false);
  unmount();
  expect(onComposerDraft).toHaveBeenLastCalledWith(false);
});
