import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("../assets/moon-character.png", () => ({ default: "mascot.png" }));
vi.mock("../api", () => ({
  api: {
    conversation: vi.fn(),
    events: vi.fn(),
    setConversationModel: vi.fn(),
    setPermission: vi.fn(),
  },
}));

import { ChatPage } from "./ChatPage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../types";

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
  fireEvent.change(screen.getByRole("textbox", { name: "消息" }), {
    target: { value: "帮我看看" },
  });
  expect(
    (screen.getByRole("button", { name: "发送" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
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
