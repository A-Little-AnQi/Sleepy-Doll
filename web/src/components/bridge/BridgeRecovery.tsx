import { useCallback, useEffect, useState } from "react";
import { api } from "../../ipc/api";
import { readError } from "../../session";
import { ConfirmDialog } from "../overlay/ConfirmDialog";
import { Toast } from "../overlay/Toast";
import type { RecoveryRecord } from "../../ipc/types";
import { ChevronIcon } from "../icons";
import "./BridgeRecovery.css";

function isBackup(record: RecoveryRecord): boolean {
  return Boolean(
    record.createdAt && record.recordVersion && record.currentVersion,
  );
}

function backupTitle(record: RecoveryRecord): string {
  const paths = record.paths.filter(Boolean);
  if (paths.length) {
    const shown = paths.slice(0, 2).join("、");
    return paths.length > 2 ? `${shown} 等 ${paths.length} 项` : shown;
  }
  if (record.operation === "offline-restore") return "恢复前的备份";
  if (record.state === "commandCheckpoint") return "操作前的备份";
  return "配置备份";
}

function backupWhen(iso?: string): string {
  if (!iso) return "";
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString();
}

export function BridgeRecovery({ onBack }: { onBack(): void }) {
  const [records, setRecords] = useState<RecoveryRecord[]>([]);
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<RecoveryRecord>();
  const refresh = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const result = await api.bridgeRecovery();
      setRecords(result.records.filter(isBackup));
      setRunning(result.hostRunning);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    const onVisible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [refresh]);
  const restore = async () => {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      const result = await api.restoreBridgeConfig(selected);
      if (result.restored) setNotice("配置已恢复。请重新启动 BetterGI。");
      setSelected(undefined);
      await refresh();
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="bridge-recovery">
      <div className="block-head">
        <button type="button" className="page-back" onClick={onBack}>
          <ChevronIcon className="button-icon" />
          BetterGI
        </button>
      </div>
      <div className="page-title">
        <h2>配置恢复</h2>
      </div>
      {running ? (
        <p className="notice">请先退出 BetterGI，再恢复配置。</p>
      ) : null}
      {notice ? <Toast message={notice} onDismiss={() => setNotice("")} /> : null}
      {error ? <Toast message={error} onDismiss={() => setError("")} /> : null}
      {records.length ? (
        <div className="recovery-list">
          {records.map((record) => (
            <article className="recovery-row" key={record.changeId}>
              <div>
                <strong>{backupTitle(record)}</strong>
                {backupWhen(record.createdAt) ? (
                  <small>{backupWhen(record.createdAt)}</small>
                ) : null}
                {!running && record.reason ? <p>{record.reason}</p> : null}
              </div>
              <button
                className="secondary-action"
                disabled={busy || !record.canRestore}
                onClick={() => {
                  setSelected(record);
                  setError("");
                }}
              >
                恢复
              </button>
            </article>
          ))}
        </div>
      ) : null}
      {!busy && !records.length ? (
        <p className="empty-note">
          还没有可恢复的备份。Sleepy Doll 修改 BetterGI 配置时会自动留下。
        </p>
      ) : null}
      <ConfirmDialog
        open={selected != null}
        title="恢复配置"
        confirmLabel="确认恢复"
        busy={busy}
        busyLabel="恢复中…"
        onClose={() => {
          if (!busy) setSelected(undefined);
        }}
        onConfirm={() => void restore()}
      >
        <p>
          把 BetterGI 的配置恢复到这次备份。当前配置会另存一份，便于再改回去。
        </p>
        {selected ? (
          <p className="muted">
            {[backupWhen(selected.createdAt), backupTitle(selected)]
              .filter(Boolean)
              .join(" · ")}
          </p>
        ) : null}
      </ConfirmDialog>
    </div>
  );
}
