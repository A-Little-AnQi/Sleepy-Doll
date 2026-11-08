import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import { Toast } from "./Toast";

type Action = { label: string; onAction(): void };
type Notice = { id: number; owner: symbol; text: string; action?: Action };
let notice: Notice | null = null;
let sequence = 0;
const listeners = new Set<() => void>();
const update = (next: Notice | null) => {
  notice = next;
  listeners.forEach((listener) => listener());
};
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
const read = () => notice;

/** 页面持有消息归属；离开后撤掉带页面操作的提示。 */
export function useMessage() {
  const owner = useRef(Symbol("message"));
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (notice?.owner === owner.current) update(null);
    };
  }, []);
  return useMemo(
    () => ({
      error(text: string, action?: Action) {
        if (!alive.current) return;
        update({
          id: ++sequence,
          owner: owner.current,
          text,
          ...(action ? { action } : {}),
        });
      },
      dismiss() {
        if (notice?.owner === owner.current) update(null);
      },
    }),
    [],
  );
}

export function MessageHost({ scopeKey }: { scopeKey?: string }) {
  const current = useSyncExternalStore(subscribe, read, read);
  useLayoutEffect(() => {
    if (notice) update(null);
  }, [scopeKey]);
  if (!current) return null;
  return (
    <Toast
      key={current.id}
      message={current.text}
      {...(current.action ? { action: current.action } : {})}
      onDismiss={() => {
        if (notice?.id === current.id) update(null);
      }}
    />
  );
}
