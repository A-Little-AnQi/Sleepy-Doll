import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { api } from "../api";
import { BridgeRecovery } from "./BridgeRecovery";

vi.mock("../api", () => ({
  api: { bridgeRecovery: vi.fn(), restoreBridgeConfig: vi.fn() },
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const backup = {
  changeId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  recordVersion: "v1",
  currentVersion: "c1",
  state: "committed",
  createdAt: "2026-09-01T12:00:00.000Z",
  operation: "commit",
  configPath: "D:\\\\BetterGI\\\\User\\\\config.json",
  paths: ["autoPickEnabled", "triggerInterval"],
  canRestore: true,
};

it("lists restorable backups and hides unreadable records", async () => {
  vi.mocked(api.bridgeRecovery).mockResolvedValue({
    hostRunning: false,
    records: [
      backup,
      {
        changeId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        paths: [],
        canRestore: false,
        reason: "记录无法校验。",
      },
    ],
  });
  render(<BridgeRecovery onBack={vi.fn()} />);
  expect(await screen.findByText("autoPickEnabled、triggerInterval")).toBeTruthy();
  expect(screen.queryByText("记录无法校验。")).toBeNull();
  expect(screen.queryByText("已提交")).toBeNull();
  expect(screen.queryByText("记录不可用")).toBeNull();
});

it("asks to quit BetterGI instead of showing every row as unusable", async () => {
  vi.mocked(api.bridgeRecovery).mockResolvedValue({
    hostRunning: true,
    records: [{ ...backup, canRestore: false, reason: "请先完全退出 BetterGI，再恢复配置。" }],
  });
  render(<BridgeRecovery onBack={vi.fn()} />);
  expect(await screen.findByText("请先退出 BetterGI，再恢复配置。")).toBeTruthy();
  expect(
    (screen.getByRole("button", { name: "恢复" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(screen.queryByText("请先完全退出 BetterGI，再恢复配置。")).toBeNull();
});

it("confirms restore in the product dialog", async () => {
  vi.mocked(api.bridgeRecovery).mockResolvedValue({
    hostRunning: false,
    records: [backup],
  });
  vi.mocked(api.restoreBridgeConfig).mockResolvedValue({
    restored: true,
    recoveryChangeId: "cccccccccccccccccccccccccccccccc",
  });
  render(<BridgeRecovery onBack={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "恢复" }));
  expect(screen.getByRole("dialog", { name: "恢复配置" })).toBeTruthy();
  expect(screen.queryByText(/D:\\\\BetterGI/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "确认恢复" }));
  await waitFor(() => {
    expect(api.restoreBridgeConfig).toHaveBeenCalledWith(backup);
  });
  expect(await screen.findByText("配置已恢复。请重新启动 BetterGI。")).toBeTruthy();
});
