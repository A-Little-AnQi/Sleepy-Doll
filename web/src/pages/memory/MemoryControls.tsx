import { useEffect, useSyncExternalStore, type ReactNode } from "react";
import { Button, SettingRow, Switch } from "@sleepy-doll/ui";
import { api } from "../../ipc/api";
import { useT } from "../../i18n";
import { readError } from "../../session";

// One persisted setting shared by both screens. An unknown value is disabled,
// rather than briefly displaying an incorrect enabled state while loading.
let state = { enabled: false, loading: true, pending: false, error: "" };
const listeners = new Set<() => void>();
let reading: Promise<void> | undefined;
let revision = 0;
function publish(next: Partial<typeof state>) {
  state = { ...state, ...next };
  listeners.forEach((listener) => listener());
}
function load() {
  const expected = revision;
  reading ??= api
    .memoryState()
    .then((result) => {
      if (expected === revision)
        publish({ enabled: result.enabled, error: "" });
    })
    .catch((reason: unknown) => {
      if (expected === revision) publish({ error: readError(reason) });
    })
    .finally(() => {
      publish({ loading: false });
      reading = undefined;
    });
}
async function setEnabled(enabled: boolean) {
  if (state.pending || state.loading) return;
  // A delayed page-load response must never undo a more recent user choice.
  revision += 1;
  publish({ pending: true, error: "" });
  try {
    const result = await api.setMemoryEnabled(enabled);
    publish({ enabled: result.enabled });
  } catch (reason) {
    publish({ error: readError(reason) });
  } finally {
    publish({ pending: false });
  }
}
export function useMemoryState() {
  const value = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => state,
  );
  useEffect(load, []);
  return value;
}

export function MemoryToggle({
  hint = false,
  children,
}: {
  hint?: boolean;
  children?: ReactNode;
}) {
  const t = useT();
  const value = useMemoryState();
  return (
    <div className="memory-toggle">
      <SettingRow
        label={t.settings.memoryEnabled}
        hint={hint ? t.settings.memoryToggleHint : undefined}
      >
        {children}
        <Switch
          checked={value.enabled}
          disabled={value.loading || value.pending}
          label={t.settings.memoryEnabled}
          onChange={(next) => void setEnabled(next)}
        />
      </SettingRow>
      {value.error && (
        <p role="alert" className="error-text">
          {value.error}
        </p>
      )}
    </div>
  );
}

export function MemorySettings({ onManage }: { onManage(): void }) {
  const t = useT();
  return (
    <section className="settings-group">
      <MemoryToggle>
        <Button variant="subtle" onClick={onManage}>
          {t.settings.memoryManage}
        </Button>
      </MemoryToggle>
    </section>
  );
}
