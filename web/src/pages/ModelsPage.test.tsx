import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("../api", () => ({
  api: {
    deleteModel: vi.fn(),
    saveModel: vi.fn(),
    useModel: vi.fn(),
    listModels: vi.fn(),
  },
}));

import { api } from "../api";
import { ModelsPage } from "./ModelsPage";
import { PREVIEW_PERMISSION, type Bootstrap, type ModelInfo } from "../types";

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(cleanup);

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
  expect(screen.getByRole("button", { name: /主模型/ })).toBeTruthy();
  expect(screen.getByRole("button", { name: "添加" })).toBeTruthy();
});

it("shows a draft row when adding a model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  expect(screen.getByRole("heading", { name: "添加模型" })).toBeTruthy();
  expect(screen.getByRole("button", { name: /新模型/ }).textContent).toContain(
    "未保存",
  );
});

it("starts a new model empty instead of picking a provider", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  expect(screen.getByRole("combobox", { name: "服务商" }).textContent).toContain(
    "选择服务商",
  );
  expect((screen.getByLabelText("名称") as HTMLInputElement).value).toBe("");
  expect(screen.getByRole("combobox", { name: "请求协议" }).textContent).toContain(
    "选择协议",
  );
  expect((screen.getByLabelText("API 地址") as HTMLInputElement).value).toBe("");
});

it("keeps request protocol with the basic fields, not under advanced", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByRole("combobox", { name: "请求协议" })).toBeTruthy();
  const toggle = screen.getByRole("button", { name: "高级选项" });
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector(".model-advanced-body.is-collapsed")).toBeTruthy();
  fireEvent.click(toggle);
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelector(".model-advanced-body.is-collapsed")).toBeNull();
  expect(screen.getByText("连接")).toBeTruthy();
  expect(screen.getByText("窗口")).toBeTruthy();
});

it("lets the user pick a fetched model instead of typing an id", async () => {
  vi.mocked(api.listModels).mockResolvedValue({
    models: ["deepseek-chat", "deepseek-reasoner"],
  });
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  fireEvent.click(screen.getByRole("combobox", { name: "服务商" }));
  fireEvent.click(screen.getByRole("option", { name: "DeepSeek" }));
  fireEvent.change(screen.getByPlaceholderText("sk-…"), {
    target: { value: "sk-test" },
  });
  fireEvent.click(screen.getByRole("button", { name: "获取模型" }));
  await waitFor(() =>
    expect(api.listModels).toHaveBeenCalledWith(
      expect.objectContaining({
        protocol: "anthropic-messages",
        apiKey: "sk-test",
        modelsUrl: "https://api.deepseek.com/models",
      }),
    ),
  );
  expect(screen.getByRole("combobox", { name: "模型" })).toBeTruthy();
  expect(screen.getByText("已获取 2 个模型")).toBeTruthy();
});

it("labels the active configuration as the default model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByText("默认")).toBeTruthy();
  expect(screen.getByText("默认模型")).toBeTruthy();
  expect(screen.queryByText("当前模型")).toBeNull();
});

it("keeps the last remaining model deletable", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(
    (screen.getByRole("button", { name: "删除" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
});

it("exposes the context window for the selected model", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={async () => undefined} />);
  fireEvent.click(screen.getByRole("button", { name: "高级选项" }));
  expect(screen.getByLabelText("上下文长度")).toBeTruthy();
  expect(
    (screen.getByLabelText("上下文长度") as HTMLInputElement).value,
  ).toBe("200000");
});

it("asks in the product dialog before deleting a model", async () => {
  const extra: ModelInfo = {
    id: "other",
    name: "Gemini",
    protocol: "gemini",
    model: "test-model",
    baseUrl: "http://127.0.0.1/v1",
    active: false,
  };
  render(
    <ModelsPage
      bootstrap={{ ...bootstrap, models: [...bootstrap.models, extra] }}
      reload={async () => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Gemini/ }));
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  expect(screen.getByRole("dialog", { name: "删除模型" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "删除模型" })).toBeNull(),
  );
  expect(api.deleteModel).not.toHaveBeenCalled();
});

it("deletes a configured model after confirmation", async () => {
  const extra: ModelInfo = {
    id: "other",
    name: "Gemini",
    protocol: "gemini",
    model: "test-model",
    baseUrl: "http://127.0.0.1/v1",
    active: false,
  };
  vi.mocked(api.deleteModel).mockResolvedValue({
    deleted: true,
    activeModel: "primary",
  });
  render(
    <ModelsPage
      bootstrap={{ ...bootstrap, models: [...bootstrap.models, extra] }}
      reload={async () => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Gemini/ }));
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  fireEvent.click(screen.getByRole("button", { name: "删除模型" }));
  await waitFor(() => expect(api.deleteModel).toHaveBeenCalledWith("other"));
});

