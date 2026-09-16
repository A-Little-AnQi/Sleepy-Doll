import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("../api", () => ({
  api: {
    deleteModel: vi.fn(),
    saveModel: vi.fn(),
    useModel: vi.fn(),
  },
}));

import { api } from "../api";
import { ModelsPage } from "./ModelsPage";
import { PREVIEW_PERMISSION, type Bootstrap, type ModelInfo } from "../types";

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

it("lists configured models instead of hiding them in a picker", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByRole("navigation", { name: "已配置的模型" })).toBeTruthy();
  expect(screen.getByRole("button", { name: /Mock Anthropic/ })).toBeTruthy();
  expect(screen.getByRole("button", { name: "添加" })).toBeTruthy();
});

it("shows a draft row when adding a model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  expect(screen.getByRole("heading", { name: "添加模型" })).toBeTruthy();
  expect(screen.getByText("尚未保存")).toBeTruthy();
});

it("labels the active configuration as the default model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByText("默认")).toBeTruthy();
  expect(screen.getByText("默认模型")).toBeTruthy();
  expect(screen.queryByText("当前模型")).toBeNull();
});

it("keeps the last remaining model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(
    (screen.getByRole("button", { name: "删除" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("exposes the context window for the selected model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByLabelText("上下文窗口（token）")).toBeTruthy();
  expect(
    (screen.getByLabelText("上下文窗口（token）") as HTMLInputElement).value,
  ).toBe("200000");
});

it("deletes a configured model after confirmation", async () => {
  const extra: ModelInfo = {
    id: "other",
    name: "Mock Gemini",
    protocol: "gemini",
    model: "mock-model",
    baseUrl: "http://127.0.0.1/v1",
    active: false,
  };
  vi.mocked(api.deleteModel).mockResolvedValue({
    deleted: true,
    activeModel: "mock",
  });
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(
    <ModelsPage
      bootstrap={{ ...bootstrap, models: [...bootstrap.models, extra] }}
      reload={async () => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Mock Gemini/ }));
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  await waitFor(() => expect(api.deleteModel).toHaveBeenCalledWith("other"));
});

