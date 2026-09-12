import type {
  Bootstrap,
  MessageInfo,
  RunEvent,
  SavedStrategy,
  TaskInfo,
  BridgeCatalog,
  BridgeMethodDetail,
  RecoveryRecord,
} from "./types";

declare global {
  interface Window {
    ipc?: { postMessage(message: string): void };
    __SLEEPY_DOLL_DESKTOP__?: boolean;
    __sleepyDollReceive?: (message: NativeMessage) => void;
  }
}

interface NativeMessage {
  kind: "response" | "event";
  id?: string;
  ok?: boolean;
  result?: unknown;
  error?: { message: string };
}

interface IpcEnvelope<T> {
  id: string;
  ok: boolean;
  result?: T;
  error?: { message: string };
}

const MOCK_BACKEND = "http://127.0.0.1:47124/ipc";
const pending = new Map<
  string,
  {
    resolve(value: unknown): void;
    reject(reason: unknown): void;
    timer: number;
  }
>();

window.__sleepyDollReceive = (message) => {
  if (message.kind !== "response" || !message.id) return;
  const entry = pending.get(message.id);
  if (!entry) return;
  window.clearTimeout(entry.timer);
  pending.delete(message.id);
  if (message.ok) entry.resolve(message.result);
  else entry.reject(new Error(message.error?.message ?? "原生请求失败"));
};

async function invokeMock<T>(
  id: string,
  method: string,
  params: Record<string, unknown>,
  timeoutMs: number,
) {
  let response: Response;
  try {
    response = await fetch(MOCK_BACKEND, {
      signal: AbortSignal.timeout(timeoutMs),
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id, method, params }),
    });
  } catch (error) {
    // A dropped connection is not the same as "never started": the event stream
    // holds a long poll open, so a restart or a sleeping machine lands here too.
    throw new Error(
      error instanceof DOMException &&
        ["TimeoutError", "AbortError"].includes(error.name)
        ? "请求超时。后台任务可能仍在运行。"
        : "无法连接本地服务。",
    );
  }
  const envelope = (await response.json()) as IpcEnvelope<T>;
  if (!response.ok || !envelope.ok) {
    throw new Error(
      envelope.error?.message ?? `Mock Backend 返回 HTTP ${response.status}`,
    );
  }
  return envelope.result as T;
}

function invoke<T>(
  method: string,
  params: Record<string, unknown> = {},
): Promise<T> {
  const id = crypto.randomUUID();
  const timeoutMs =
    method === "bridge.setEnabled"
      ? 100_000
      : method === "events.read"
        ? 30_000
        : 15_000;
  const ipc = window.ipc;
  if (!ipc) return invokeMock<T>(id, method, params, timeoutMs);
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      pending.delete(id);
      reject(new Error("请求超时。后台任务可能仍在运行，请刷新状态后重试。"));
    }, timeoutMs);
    pending.set(id, { resolve: (value) => resolve(value as T), reject, timer });
    try {
      ipc.postMessage(JSON.stringify({ id, method, params }));
    } catch (error) {
      window.clearTimeout(timer);
      pending.delete(id);
      reject(error);
    }
  });
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  submitTask: (
    prompt: string,
    conversationId?: string,
    clientKey: string = crypto.randomUUID(),
  ) =>
    invoke<TaskInfo>("task.submit", {
      prompt,
      clientKey,
      ...(conversationId ? { conversationId } : {}),
    }),
  task: (id: string) => invoke<TaskInfo>("task.get", { id }),
  tasks: () => invoke<TaskInfo[]>("task.list"),
  resume: (id: string) => invoke<TaskInfo>("run.resume", { id }),
  supplement: (
    id: string,
    content: string,
    clientKey: string = crypto.randomUUID(),
  ) => invoke("run.input", { id, content, clientKey }),
  approve: (id: string, approved: boolean) =>
    invoke("approval.respond", { id, approved }),
  events: (conversationId: string, after: number) =>
    invoke<{ events: RunEvent[] }>("events.read", {
      conversationId,
      after,
      waitMs: 20000,
    }),
  cancelTask: (id: string) => invoke<TaskInfo>("task.cancel", { id }),
  conversation: (id: string) =>
    invoke<{ id: string; messages: MessageInfo[] }>("conversation.get", { id }),
  useModel: (id: string) =>
    invoke<{ activeModel: string }>("model.use", { id }),
  saveModel: (model: {
    id: string;
    name: string;
    protocol: string;
    model: string;
    baseUrl: string;
    apiKey: string;
    timeoutMs?: number;
  }) => invoke<{ saved: boolean }>("model.save", { model }),
  setSkillEnabled: (name: string, enabled: boolean) =>
    invoke<{ enabled: boolean }>("skill.setEnabled", { name, enabled }),
  setPluginEnabled: (id: string, enabled: boolean) =>
    invoke<{ restartRequired: boolean }>("plugin.setEnabled", { id, enabled }),
  bridgeState: () => invoke<unknown>("bridge.state"),
  bridgeRecovery: () =>
    invoke<{ hostRunning: boolean; records: RecoveryRecord[] }>(
      "bridge.recoveryList",
    ),
  restoreBridgeConfig: (record: RecoveryRecord) =>
    invoke<{ restored: boolean; recoveryChangeId: string }>("bridge.restore", {
      changeId: record.changeId,
      recordVersion: record.recordVersion,
      currentVersion: record.currentVersion,
    }),
  bridgeCatalog: (query = "", group = "", offset = 0) =>
    invoke<BridgeCatalog>("bridge.catalog", { query, group, offset }),
  bridgeDescribe: (methodId: string) =>
    invoke<BridgeMethodDetail>("bridge.describe", { methodId }),
  setBridgeEnabled: (enabled: boolean) =>
    invoke<{ enabled: boolean; warning?: string }>("bridge.setEnabled", {
      enabled,
    }),
  installPlugin: (path: string) =>
    invoke<{ id: string }>("plugin.install", { path }),
  removePlugin: (id: string) => invoke("plugin.remove", { id }),
  reloadExtensions: () => invoke("extensions.reload"),
  extractStrategy: (runId: string, name: string) =>
    invoke<SavedStrategy>("strategy.extract", { runId, name }),
  extractWorkflow: (runId: string, name: string) =>
    invoke("workflow.extract", { runId, name }),
  runStrategy: (id: string) =>
    invoke<TaskInfo>("strategy.run", {
      id,
      clientKey: crypto.randomUUID(),
    }),
  runWorkflow: (id: string) =>
    invoke<TaskInfo>("workflow.run", {
      id,
      clientKey: crypto.randomUUID(),
    }),
  executeOperation: (id: string) => invoke("operation.execute", { id }),
  rollbackOperation: (id: string) => invoke("operation.rollback", { id }),
};
