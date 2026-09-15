import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

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
afterEach(cleanup);

const bootstrap: Bootstrap = {
  configPath: "/tmp/config.json",
  models: [
    {
      id: "mock",
      name: "Mock Anthropic",
      protocol: "anthropic-messages",
      model: "mock-model",
      baseUrl: "http://127.0.0.1/v1",
      active: true,
      timeoutMs: 120000,
    },
    {
      id: "gemini",
      name: "Mock Gemini",
      protocol: "gemini",
      model: "mock-model",
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
    "Mock Anthropic",
  );
  expect(screen.queryByText("跟随默认模型")).toBeNull();
  expect(screen.getByText("0 / 200k")).toBeTruthy();
});
