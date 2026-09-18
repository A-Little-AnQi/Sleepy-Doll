/** 安装窗口的原生接口。请求写法与产品主程序的窗口控制一致，回复和状态推送走
 * 初始化脚本装上的两个全局回调。 */

export interface SetupInfo {
  version: string;
  directory: string;
  defaultDirectory: string;
  installed: boolean;
  installedVersion: string | null;
  uninstallMode: boolean;
}

export interface SetupState {
  phase: "idle" | "running" | "done" | "failed";
  progress: number;
  message: string;
  error: string | null;
}

interface SetupReply {
  id: string;
  result?: unknown;
  error?: { message?: string } | null;
}

declare global {
  interface Window {
    __setupReceive?: (reply: SetupReply) => void;
    __setupState?: (state: SetupState) => void;
  }
}

/** 目录选择是模态对话框，用户可能停留很久；info 短一些，超时就用界面里的默认值。 */
const DIALOG_TIMEOUT = 600_000;
const INFO_TIMEOUT = 5_000;

const pending = new Map<
  string,
  {
    resolve(value: unknown): void;
    reject(reason: unknown): void;
    timer: number;
  }
>();

window.__setupReceive = (reply) => {
  const entry = pending.get(reply.id);
  if (!entry) return;
  window.clearTimeout(entry.timer);
  pending.delete(reply.id);
  if (reply.error)
    entry.reject(new Error(reply.error.message ?? "安装程序返回失败"));
  else entry.resolve(reply.result);
};

function invoke<T>(
  method: string,
  params: Record<string, unknown>,
  timeoutMs: number,
): Promise<T> {
  const ipc = window.ipc;
  if (!ipc) return Promise.reject(new Error("无法连接安装程序。"));
  const id = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      pending.delete(id);
      reject(new Error("安装程序没有响应。"));
    }, timeoutMs);
    pending.set(id, { resolve: (value) => resolve(value as T), reject, timer });
    ipc.postMessage(JSON.stringify({ id, method, params }));
  });
}

export const setupApi = {
  info: () => invoke<SetupInfo>("setup.info", {}, INFO_TIMEOUT),
  browse: () =>
    invoke<{ directory: string | null }>("setup.browse", {}, DIALOG_TIMEOUT),
  install: (directory: string, desktopShortcut: boolean) =>
    invoke("setup.install", { directory, desktopShortcut }, DIALOG_TIMEOUT),
  uninstall: (removeUserData: boolean) =>
    invoke("setup.uninstall", { removeUserData }, DIALOG_TIMEOUT),
};
