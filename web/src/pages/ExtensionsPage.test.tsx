import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { api } from "../api";
import { ExtensionsPage } from "./ExtensionsPage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../types";

vi.mock("../api", () => ({
  api: {
    reloadExtensions: vi.fn(),
    setSkillEnabled: vi.fn(),
    setPluginEnabled: vi.fn(),
    installSkill: vi.fn(),
    installPlugin: vi.fn(),
    removePlugin: vi.fn(),
  },
}));

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const bootstrap: Bootstrap = {
  configPath: "/tmp/config.json",
  models: [],
  skills: [
    {
      name: "bgi-operator",
      description: "操作手册",
      source: "product",
      tags: [],
      enabled: true,
      available: false,
      unavailableReason: "需要先启用对应插件",
    },
  ],
  plugins: [
    {
      manifest: {
        id: "bgi",
        name: "BetterGI",
        version: "0.1.0",
        description: "游戏自动化宿主",
      },
      status: "enabled",
      configuredEnabled: true,
      host: true,
    },
  ],
  tools: [
    {
      name: "bgi.state.get",
      description: "读取一次宿主状态",
      source: "core:bgi",
    },
  ],
  conversations: [],
  tasks: [],
  strategies: [],
  workflows: [],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  permission: PREVIEW_PERMISSION,
  bridge: { enabled: false, connected: false, baseUrl: "http://127.0.0.1" },
};

it("rescans installed extensions without a refresh control", async () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  const reload = vi.fn(async () => undefined);
  render(<ExtensionsPage bootstrap={bootstrap} reload={reload} />);
  expect(screen.queryByRole("button", { name: /刷新/ })).toBeNull();
  expect(screen.getByRole("button", { name: "导入技能" })).toBeTruthy();
  await waitFor(() => {
    expect(api.reloadExtensions).toHaveBeenCalledTimes(1);
    expect(reload).toHaveBeenCalled();
  });
});

it("lists the host plugin as an introduction switch, not a connection control", async () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  const onOpenHost = vi.fn();
  render(
    <ExtensionsPage
      bootstrap={bootstrap}
      reload={async () => undefined}
      tab="plugins"
      onOpenHost={onOpenHost}
    />,
  );
  expect(screen.getByText("BetterGI")).toBeTruthy();
  expect(screen.getByText("随产品")).toBeTruthy();
  expect(screen.queryByText("bgi.state.get")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /游戏自动化宿主/ }));
  expect(screen.getByRole("button", { name: "打开设置" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "移除插件" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
  expect(onOpenHost).toHaveBeenCalled();
});

it("asks the user to enable a plugin when a skill is waiting on a provider", () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  render(<ExtensionsPage bootstrap={bootstrap} reload={async () => undefined} />);
  expect(screen.getByText("当前未生效：需要先启用对应插件")).toBeTruthy();
  expect(screen.queryByText(/BetterGI/)).toBeNull();
});

it("hides host settings until the plugin is introduced", async () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  render(
    <ExtensionsPage
      bootstrap={{
        ...bootstrap,
        plugins: [
          {
            ...bootstrap.plugins[0]!,
            status: "disabled",
            configuredEnabled: false,
          },
        ],
      }}
      reload={async () => undefined}
      tab="plugins"
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /游戏自动化宿主/ }));
  expect(screen.queryByRole("button", { name: "打开设置" })).toBeNull();
  expect(screen.getByText("随产品提供。开启后会出现在侧栏和设置里。")).toBeTruthy();
});

it("asks in the product dialog before removing a plugin", async () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  vi.mocked(api.removePlugin).mockResolvedValue(undefined);
  const reload = vi.fn(async () => undefined);
  render(
    <ExtensionsPage
      bootstrap={{
        ...bootstrap,
        plugins: [
          ...bootstrap.plugins,
          {
            manifest: {
              id: "pack",
              name: "路线包",
              version: "1",
              description: "用户插件",
            },
            status: "disabled",
            configuredEnabled: false,
          },
        ],
      }}
      reload={reload}
      tab="plugins"
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /用户插件/ }));
  fireEvent.click(screen.getByRole("button", { name: "移除插件" }));
  const dialog = screen.getByRole("dialog", { name: "移除插件" });
  fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "移除插件" })).toBeNull(),
  );
  expect(api.removePlugin).not.toHaveBeenCalled();
});

it("removes a plugin after the product dialog is confirmed", async () => {
  vi.mocked(api.reloadExtensions).mockResolvedValue(undefined);
  vi.mocked(api.removePlugin).mockResolvedValue(undefined);
  const reload = vi.fn(async () => undefined);
  render(
    <ExtensionsPage
      bootstrap={{
        ...bootstrap,
        plugins: [
          ...bootstrap.plugins,
          {
            manifest: {
              id: "pack",
              name: "路线包",
              version: "1",
              description: "用户插件",
            },
            status: "disabled",
            configuredEnabled: false,
          },
        ],
      }}
      reload={reload}
      tab="plugins"
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /用户插件/ }));
  fireEvent.click(screen.getByRole("button", { name: "移除插件" }));
  fireEvent.click(
    within(screen.getByRole("dialog", { name: "移除插件" })).getByRole(
      "button",
      { name: "移除插件" },
    ),
  );
  await waitFor(() => expect(api.removePlugin).toHaveBeenCalledWith("pack"));
});
