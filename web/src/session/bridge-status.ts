import type { Bootstrap } from "../ipc/types";

export function applyBridgeStatus(
  previous: Bootstrap | undefined,
  next: Bootstrap["bridge"],
): Bootstrap | undefined {
  if (!previous) return previous;
  const current = previous.bridge;
  if (
    current.enabled === next.enabled &&
    current.connected === next.connected &&
    current.baseUrl === next.baseUrl &&
    current.launchSilently === next.launchSilently &&
    current.stale === next.stale &&
    current.error === next.error
  )
    return previous;
  return { ...previous, bridge: next };
}

/** 被动同步状态；递归定时避免慢请求重叠，旧请求不能覆盖较新的手动刷新。 */
export function watchBridgeStatus({
  read,
  publish,
  generation,
  interval = 3000,
}: {
  read(): Promise<Bootstrap["bridge"]>;
  publish(value: Bootstrap["bridge"]): void;
  generation(): number;
  interval?: number;
}) {
  let stopped = false;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const refresh = async () => {
    if (stopped || pending || document.visibilityState === "hidden") return;
    if (timer !== undefined) clearTimeout(timer);
    pending = true;
    const started = generation();
    try {
      const value = await read();
      if (!stopped && started === generation()) publish(value);
    } catch {
      // IPC 暂时不可用不冒充 BGI 断开；下一轮重试，后台状态会携带真实连接错误。
    } finally {
      pending = false;
      if (!stopped) timer = setTimeout(() => void refresh(), interval);
    }
  };
  const visible = () => {
    if (document.visibilityState === "visible") void refresh();
  };
  window.addEventListener("focus", visible);
  document.addEventListener("visibilitychange", visible);
  void refresh();
  return () => {
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
    window.removeEventListener("focus", visible);
    document.removeEventListener("visibilitychange", visible);
  };
}
