import { useSyncExternalStore } from "react";

/**
 * 一项存在本机、可能同时有好几处在编辑的偏好。
 *
 * 当前值只有 `localStorage` 这一份，写入之后通知所有订阅者。
 * `decode` 挡住手改过的值，`apply` 把值落到 DOM 上。
 */
export function preference<T extends string>(
  key: string,
  decode: (stored: string | null) => T,
  apply: (value: T) => void,
) {
  const listeners = new Set<() => void>();
  const read = () => decode(localStorage.getItem(key));
  const subscribe = (listener: () => void) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  };
  return {
    read,
    subscribe,
    /** 启动时恢复上次的选择：只落到 DOM 上，不写回，也不通知。 */
    restore: () => apply(read()),
    write(value: T) {
      localStorage.setItem(key, value);
      apply(value);
      listeners.forEach((listener) => listener());
    },
    use: () => useSyncExternalStore(subscribe, read),
  };
}
