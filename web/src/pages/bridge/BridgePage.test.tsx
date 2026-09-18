import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { api } from "../../ipc/api";
import { BridgePage } from "./BridgePage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../../ipc/types";

vi.mock("../../ipc/api", () => ({
  api: {
    setBridgeEnabled: vi.fn(),
    bridgeState: vi.fn(),
    bridgeCatalog: vi.fn(),
    bridgeRecovery: vi.fn(),
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function bootstrap(connected: boolean): Bootstrap {
  return {
    configPath: "/tmp/config.json",
    models: [],
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
    bridge: {
      enabled: true,
      connected,
      baseUrl: "http://127.0.0.1:26101",
    },
  };
}

it("does not open the catalog before BetterGI is connected", () => {
  render(
    <BridgePage bootstrap={bootstrap(false)} reload={async () => undefined} />,
  );
  expect(
    (screen.getByRole("button", { name: /接口目录/ }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: /接口目录/ }));
  expect(screen.queryByRole("button", { name: "BetterGI" })).toBeNull();
  expect(screen.getByRole("heading", { name: "BetterGI" })).toBeTruthy();
});

it("opens the catalog after BetterGI is connected", () => {
  vi.mocked(api.bridgeCatalog).mockResolvedValue({ total: 0, items: [] });
  render(
    <BridgePage bootstrap={bootstrap(true)} reload={async () => undefined} />,
  );
  expect(
    (screen.getByRole("button", { name: /接口目录/ }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: /接口目录/ }));
  expect(screen.getByRole("button", { name: "BetterGI" })).toBeTruthy();
});

it("connects without a refresh control or host-state dump", () => {
  render(
    <BridgePage bootstrap={bootstrap(true)} reload={async () => undefined} />,
  );
  expect(screen.queryByRole("button", { name: "刷新" })).toBeNull();
  expect(screen.queryByRole("button", { name: /重新连接/ })).toBeTruthy();
  expect(screen.queryByText("状态")).toBeNull();
  expect(api.bridgeState).not.toHaveBeenCalled();
});
