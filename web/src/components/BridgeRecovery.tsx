import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { readError } from "../session";
import { Toast } from "./Toast";
import type { RecoveryRecord } from "../types";
import { ChevronIcon, CloseIcon, RefreshIcon } from "./icons";
import "./BridgeRecovery.css";

export function BridgeRecovery({ onBack }: { onBack(): void }) {
  const [records, setRecords] = useState<RecoveryRecord[]>([]);
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<RecoveryRecord>();
  const dialog = useRef<HTMLDialogElement>(null);
  const refresh = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const result = await api.bridgeRecovery();
      setRecords(result.records);
      setRunning(result.hostRunning);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  const restore = async () => {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      const result = await api.restoreBridgeConfig(selected);
      if (result.restored)
        setNotice(
          "配置已恢复并核验，恢复前的文件也已另存。现在可以重新启动 BetterGI。",
        );
      dialog.current?.close();
      await refresh();
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  const labels: Record<string, string> = {
    committed: "已提交",
    rolledBack: "已回退",
    prepared: "提交待核对",
    recoveryRequired: "需要恢复",
    revertedAfterFailure: "失败后已复原",
    commandCheckpoint: "命令执行前备份",
    offlineRestorePrepared: "恢复待核对",
    offlineRestored: "离线恢复完成",
  };
  return (
    <div className="bridge-recovery">
      <div className="block-head">
        <button className="subtle-action" onClick={onBack}>
          <ChevronIcon className="button-icon" />
          BetterGI
        </button>
        <button
          className="secondary-action"
          disabled={busy}
          onClick={() => void refresh()}
        >
          <RefreshIcon className="button-icon" />
          刷新
        </button>
      </div>
      <div className="page-title">
        <h2>配置恢复记录</h2>
      </div>
      <p className="muted">
        显示最近 100
        条记录。离线恢复会恢复该记录之前的完整配置；普通按字段撤销使用接口
        bgi.rollback_settings。
      </p>
      {running && (
        <p className="notice">
          BetterGI 仍在运行。请完全退出宿主后，再恢复整个配置。
        </p>
      )}
      {notice && <Toast message={notice} onDismiss={() => setNotice("")} />}
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="recovery-list">
        {records.map((record) => (
          <article className="recovery-row" key={record.changeId}>
            <div>
              <strong>
                {labels[record.state ?? ""] ?? record.state ?? "记录不可用"}
              </strong>
              <p>
                {record.paths.join("、") || record.operation || record.changeId}
              </p>
              <small>
                {record.createdAt
                  ? new Date(record.createdAt).toLocaleString()
                  : ""}
              </small>
              {record.reason && <p>{record.reason}</p>}
            </div>
            <button
              className="secondary-action"
              disabled={busy || !record.canRestore}
              onClick={() => {
                setSelected(record);
                setError("");
                dialog.current?.showModal();
              }}
            >
              离线恢复
            </button>
          </article>
        ))}
      </div>
      {!busy && !records.length && (
        <p className="empty-note">尚无配置变更或命令前备份。</p>
      )}
      <dialog ref={dialog}>
        <div className="dialog-head">
          <h2>恢复整个配置</h2>
          <button
            className="icon-button"
            aria-label="关闭"
            disabled={busy}
            onClick={() => dialog.current?.close()}
          >
            <CloseIcon className="button-icon" />
          </button>
        </div>
        <div className="dialog-body">
          <p>
            将恢复这条记录之前的完整配置。当前文件会先另存一份；如果配置在确认后变化，恢复会被拒绝。
          </p>
          <pre>{selected?.configPath}</pre>
          <div className="detail-actions">
            <button
              className="primary-action"
              disabled={busy}
              onClick={() => void restore()}
            >
              {busy ? "恢复中…" : "确认恢复"}
            </button>
            <button
              className="secondary-action"
              disabled={busy}
              onClick={() => dialog.current?.close()}
            >
              取消
            </button>
          </div>
        </div>
      </dialog>
    </div>
  );
}
