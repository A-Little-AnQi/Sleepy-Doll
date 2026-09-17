import type {
  Bootstrap,
  BridgeCatalog,
  BridgeMethodDetail,
  ConversationInfo,
  MessageInfo,
  RecoveryRecord,
  RunEvent,
  SavedStrategy,
  TaskInfo,
  PermissionState,
  TaskSummary,
  TaskValidation,
  WorkflowDetail,
} from "./types";
import type { GroupLayout } from "./conversation-groups";
import { withPermission } from "./types";

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

export function eventsReadParams(
  conversationId: string,
  after: number,
  nativeIpc = typeof window !== "undefined" && Boolean(window.ipc),
) {
  return nativeIpc
    ? { conversationId, after, waitMs: 20_000 }
    : { conversationId, after };
}

export function devIpcUrl(dev = import.meta.env.DEV) {
  return dev ? "/ipc" : "http://127.0.0.1:47124/ipc";
}

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

async function invokeHttp<T>(
  id: string,
  method: string,
  params: Record<string, unknown>,
  timeoutMs: number,
) {
  let response: Response;
  try {
    response = await fetch(devIpcUrl(), {
      signal: AbortSignal.timeout(timeoutMs),
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id, method, params }),
    });
  } catch (error) {
    throw new Error(
      error instanceof DOMException &&
        ["TimeoutError", "AbortError"].includes(error.name)
        ? "请求超时。后台任务可能仍在运行。"
        : "无法连接本地服务。",
    );
  }
  const text = await response.text();
  if (!text) {
    throw new Error(
      response.ok
        ? "无法连接本地服务。"
        : `本地服务返回 HTTP ${response.status}`,
    );
  }
  const envelope = JSON.parse(text) as IpcEnvelope<T>;
  if (!response.ok || !envelope.ok) {
    throw new Error(
      envelope.error?.message ?? `本地服务返回 HTTP ${response.status}`,
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
      : method === "events.read" || method === "model.list"
        ? 30_000
        : 15_000;
  const ipc = window.ipc;
  if (!ipc) return invokeHttp<T>(id, method, params, timeoutMs);
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
  bootstrap: () => invoke<Bootstrap>("bootstrap").then(withPermission),
  submitTask: (
    prompt: string,
    conversationId?: string,
    clientKey: string = crypto.randomUUID(),
    modelId?: string | null,
  ) =>
    invoke<TaskInfo>("task.submit", {
      prompt,
      clientKey,
      ...(conversationId ? { conversationId } : {}),
      // 新对话里先挑好的模型随第一条消息一起生效；留空表示跟随默认模型。
      ...(modelId ? { modelId } : {}),
    }),
  task: (id: string) => invoke<TaskInfo>("task.get", { id }),
  tasks: () => invoke<TaskInfo[]>("task.list"),
  supplement: (
    id: string,
    content: string,
    clientKey: string = crypto.randomUUID(),
  ) => invoke("run.input", { id, content, clientKey }),
  approve: (id: string, approved: boolean) =>
    invoke("approval.respond", { id, approved }),
  events: (conversationId: string, after: number) =>
    invoke<{ events: RunEvent[]; snapshotRequired?: boolean }>(
      "events.read",
      eventsReadParams(conversationId, after, Boolean(window.ipc)),
    ),
  cancelTask: (id: string) => invoke<TaskInfo>("task.cancel", { id }),
  conversation: (id: string) =>
    invoke<{ id: string; messages: MessageInfo[] }>("conversation.get", { id }),
  useModel: (id: string) =>
    invoke<{ activeModel: string }>("model.use", { id }),
  deleteModel: (id: string) =>
    invoke<{ deleted: boolean; activeModel: string }>("model.delete", { id }),
  permission: () => invoke<PermissionState>("permission.get"),
  setPermission: (mode: string) =>
    invoke<{ mode: string; label: string }>("permission.set", { mode }),
  saveModel: (model: {
    id: string;
    name: string;
    protocol: string;
    model: string;
    baseUrl: string;
    apiKey: string;
    timeoutMs?: number;
    contextWindow?: number;
    maxOutputTokens?: number;
    auth?: string;
    promptCache?: boolean;
  }) => invoke<{ saved: boolean }>("model.save", { model }),
  listModels: (probe: {
    id?: string;
    protocol: string;
    baseUrl: string;
    apiKey: string;
    auth?: string;
    modelsUrl?: string;
  }) => invoke<{ models: string[] }>("model.list", probe),
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
  installSkill: (path: string) =>
    invoke<{ name: string }>("skill.install", { path }),
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
  executeOperation: (id: string) => invoke("operation.execute", { id }),
  rollbackOperation: (id: string) => invoke("operation.rollback", { id }),

  configRead: () =>
    invoke<{ path: string; content: string }>("config.read"),
  configWrite: (content: string) =>
    invoke<{ saved: boolean }>("config.write", { content }),
  conversations: (search = "", includeArchived = false) =>
    invoke<ConversationInfo[]>("conversation.list", {
      search,
      includeArchived,
      limit: 200,
    }),
  renameConversation: (id: string, title: string) =>
    invoke("conversation.rename", { id, title }),
  saveConversationGroups: (layout: GroupLayout) =>
    invoke("conversation.groups.save", {
      groups: layout.groups,
      membership: layout.membership,
    }),
  pinConversation: (id: string, pinned: boolean) =>
    invoke("conversation.setPinned", { id, pinned }),
  archiveConversation: (id: string, archived: boolean) =>
    invoke("conversation.setArchived", { id, archived }),
  setConversationModel: (id: string, modelId: string) =>
    invoke("conversation.setModel", { id, modelId }),
  deleteConversation: (id: string, confirmed = false) =>
    invoke<{
      requiresConfirmation?: boolean;
      affects?: { title: string; taskCount: number };
      keeps?: string;
      deleted?: boolean;
      messages?: number;
      runs?: number;
      tasksKept?: number;
    }>("conversation.delete", { id, confirmed }),

  workflowList: (conversationId?: string) =>
    invoke<TaskSummary[]>("workflow.list", {
      ...(conversationId ? { conversationId } : {}),
    }),
  workflowGet: (id: string) => invoke<WorkflowDetail>("workflow.get", { id }),
  workflowValidate: (id: string, draftRevision?: number) =>
    invoke<{
      validation: TaskValidation;
      publishable: boolean;
      zeroToken: boolean;
      modelUsage: string;
    }>("workflow.validate", {
      id,
      ...(draftRevision ? { draftRevision } : {}),
    }),
  workflowPublish: (
    id: string,
    draftRevision: number,
    expectedPublishedRevision?: number,
  ) =>
    invoke<{ publishedRevision: number; zeroToken: boolean }>(
      "workflow.publish",
      {
        id,
        draftRevision,
        ...(expectedPublishedRevision ? { expectedPublishedRevision } : {}),
      },
    ),
  runWorkflow: (id: string, expectedPublishedRevision?: number) =>
    invoke<TaskInfo>("workflow.run", {
      id,
      clientKey: crypto.randomUUID(),
      ...(expectedPublishedRevision ? { expectedPublishedRevision } : {}),
    }),
  renameWorkflow: (id: string, name: string) =>
    invoke("workflow.rename", { id, name }),
  pinWorkflow: (id: string, pinned: boolean) =>
    invoke("workflow.pin", { id, pinned }),
  archiveWorkflow: (id: string, archived: boolean) =>
    invoke(archived ? "workflow.archive" : "workflow.restore", { id }),
  deleteWorkflow: (id: string) =>
    invoke<{ deleted: boolean; activeRuns: string[]; historyKept: boolean }>(
      "workflow.delete",
      { id },
    ),
  copyWorkflow: (id: string) => invoke<{ id: string }>("workflow.copy", { id }),
};
