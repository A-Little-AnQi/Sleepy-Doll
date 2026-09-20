import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { api } from "../../ipc/api";
import { BridgeApiExplorer } from "./BridgeApiExplorer";

vi.mock("../../ipc/api", () => ({
  api: { bridgeCatalog: vi.fn(), bridgeDescribe: vi.fn() },
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const entry = {
  methodId: "bgi.get_setting",
  displayName: "读取设置",
  group: "settings",
  summary: "读取当前配置值和可写约束",
  effect: "readOnly",
  callable: false,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "配置目录中的精确路径",
    },
  ],
};

it("shows summary and availability in rows, and the count exactly once", async () => {
  vi.mocked(api.bridgeCatalog).mockResolvedValue({ total: 1, items: [entry] });
  render(<BridgeApiExplorer onBack={vi.fn()} />);
  expect(await screen.findByText(entry.summary)).toBeTruthy();
  expect(screen.getByText("1 个接口")).toBeTruthy();
  expect(screen.queryByText(/配置目录中的精确路径/)).toBeNull();
  expect(screen.getByText("不可用")).toBeTruthy();
  expect(api.bridgeDescribe).not.toHaveBeenCalled();
});

it("loads every page and renders the guide from the bridge rather than a frontend copy", async () => {
  vi.mocked(api.bridgeCatalog)
    .mockResolvedValueOnce({ total: 2, items: [entry], nextOffset: 1 })
    .mockResolvedValueOnce({
      total: 2,
      items: [{ ...entry, methodId: "bgi.other", displayName: "第二项" }],
      nextOffset: null,
    });
  vi.mocked(api.bridgeDescribe).mockResolvedValue({
    ...entry,
    inputSchema: {
      type: "object",
      properties: { path: { type: "string", description: "精确路径" } },
      required: ["path"],
    },
    guide: {
      title: "读取设置",
      purpose: "来自桥的完整用途说明",
      whenToUse: ["准备查询时"],
      preconditions: ["宿主已连接"],
      sideEffects: ["不写配置"],
      resultMeaning: "返回当前值",
      verification: "按版本核验",
      rollback: "只读无需回退",
      examples: [{ path: "triggerInterval" }],
      documentationSource: "bridge-contract",
    },
  });
  render(<BridgeApiExplorer onBack={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "加载更多" }));
  expect(await screen.findByText("第二项")).toBeTruthy();
  expect(api.bridgeCatalog).toHaveBeenLastCalledWith("", "", 1);
  fireEvent.click(
    screen.getByRole("button", { name: /读取设置.*bgi.get_setting/ }),
  );
  expect(await screen.findByText("来自桥的完整用途说明")).toBeTruthy();
  expect(screen.getByText("只读无需回退")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "参数" })).toBeTruthy();
});

it("discards an older search response that arrives after a newer query", async () => {
  let resolveOld!: (value: { total: number; items: (typeof entry)[] }) => void;
  vi.mocked(api.bridgeCatalog)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    )
    .mockResolvedValueOnce({
      total: 1,
      items: [{ ...entry, summary: "新的搜索结果" }],
    });
  render(<BridgeApiExplorer onBack={vi.fn()} />);
  await waitFor(() => expect(api.bridgeCatalog).toHaveBeenCalledTimes(1));
  fireEvent.change(screen.getByRole("textbox", { name: "搜索接口" }), {
    target: { value: "新查询" },
  });
  expect(await screen.findByText("新的搜索结果")).toBeTruthy();
  resolveOld({ total: 1, items: [entry] });
  await waitFor(() => expect(screen.queryByText(entry.summary)).toBeNull());
});
