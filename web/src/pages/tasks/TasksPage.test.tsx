import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TasksPage } from "./TasksPage";
import { PREVIEW_PERMISSION, type Bootstrap, type TaskSummary } from "../../ipc/types";

vi.mock("../../ipc/api", () => ({
  api: {
    workflowList: vi.fn(),
    deleteWorkflow: vi.fn(),
  },
}));

import { api } from "../../ipc/api";

afterEach(cleanup);

const unavailable: TaskSummary = {
  id: "task-1",
  name: "巡夜",
  description: "到点执行",
  state: "unavailable",
  stateLabel: "需要连接工具",
  actionLabel: "连接工具",
  runnable: false,
  pinned: false,
  sourceConversationId: "chat-1",
  sourceTitleSnapshot: "夜巡",
  sourceDeleted: false,
  publishedRevision: 1,
  revision: 1,
  modelUsage: "none",
  zeroToken: true,
  nodeCount: 1,
  updatedAt: "2026-01-01T00:00:00Z",
  issue: "需要先连接工具才能运行。",
};

const bootstrap: Bootstrap = {
  configPath: "/tmp/config.json",
  models: [],
  skills: [],
  plugins: [],
  tools: [],
  conversations: [],
  tasks: [],
  strategies: [],
  workflows: [unavailable],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  permission: PREVIEW_PERMISSION,
  bridge: { enabled: false, connected: false, baseUrl: "http://127.0.0.1" },
};

it("keeps unavailable tasks on the provider layer instead of naming the host", () => {
  const onConnectTools = vi.fn();
  const onOpenConversation = vi.fn();
  render(
    <TasksPage
      bootstrap={bootstrap}
      reload={async () => undefined}
      onOpenConversation={onOpenConversation}
      onConnectTools={onConnectTools}
    />,
  );
  expect(screen.getByText("需要先连接工具才能运行。")).toBeTruthy();
  expect(screen.queryByText(/BetterGI|bgi\./)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "连接工具" }));
  expect(onConnectTools).toHaveBeenCalled();
  expect(onOpenConversation).not.toHaveBeenCalled();
});

it("asks in the product dialog before deleting a shortcut task", () => {
  render(
    <TasksPage
      bootstrap={bootstrap}
      reload={async () => undefined}
      onOpenConversation={() => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "巡夜 的更多操作" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "删除任务" }));
  expect(screen.getByRole("dialog", { name: "删除快捷任务" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(api.deleteWorkflow).not.toHaveBeenCalled();
});

it("deletes a shortcut task after the product dialog is confirmed", async () => {
  vi.mocked(api.deleteWorkflow).mockResolvedValue({
    deleted: true,
    activeRuns: [],
    historyKept: true,
  });
  vi.mocked(api.workflowList).mockResolvedValue([]);
  render(
    <TasksPage
      bootstrap={bootstrap}
      reload={async () => undefined}
      onOpenConversation={() => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "巡夜 的更多操作" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "删除任务" }));
  fireEvent.click(screen.getByRole("button", { name: "删除任务" }));
  await waitFor(() => expect(api.deleteWorkflow).toHaveBeenCalledWith("task-1"));
});
