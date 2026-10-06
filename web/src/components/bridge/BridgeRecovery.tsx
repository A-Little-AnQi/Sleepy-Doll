import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { readError } from "../../session";
import { Dialog } from "../overlay/Dialog";
import { Toast } from "../overlay/Toast";
import { SlidingTabs } from "../controls/SlidingTabs";
import { Checkbox } from "../controls/Checkbox";
import { ChevronIcon, SearchIcon } from "../icons";
import type { RecoveryRecord, RecoveryPreview } from "../../ipc/types";
import "./BridgeRecovery.css";

const when = (value?: string) =>
  value ? new Date(value).toLocaleString() : "时间未知";
const fields = (record: RecoveryRecord) =>
  record.fields ?? record.paths.map((path) => ({ path, label: path }));
const title = (record: RecoveryRecord) =>
  record.kind === "unavailable"
    ? "无法读取的备份"
    : record.paths.length
      ? fields(record)
          .slice(0, 2)
          .map((field) => field.label)
          .join("、") +
        (record.paths.length > 2 ? ` 等 ${record.paths.length} 项` : "")
      : record.operation === "offline-restore"
        ? "恢复前的完整备份"
        : "完整配置备份";
function valueText(value: unknown) {
  return value === null || value === undefined
    ? "未设置"
    : value === ""
      ? "（空字符串）"
      : typeof value === "boolean"
        ? value
          ? "开启"
          : "关闭"
        : typeof value === "object"
          ? JSON.stringify(value, null, 2)
          : String(value);
}
export function BridgeRecovery({ onBack }: { onBack(): void }) {
  const [records, setRecords] = useState<RecoveryRecord[]>([]);
  const [runningTargets, setRunningTargets] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [tab, setTab] = useState<"changes" | "backups">("changes");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<RecoveryRecord>();
  const [mode, setMode] = useState<"fields" | "full">("fields");
  const [paths, setPaths] = useState<string[]>([]);
  const [preview, setPreview] = useState<RecoveryPreview>();
  const [previewing, setPreviewing] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const [generation, setGeneration] = useState(0);
  const sequence = useRef(0);
  const listSequence = useRef(0);
  const [undo, setUndo] = useState<string>();
  const refresh = useCallback(async () => {
    const current = ++listSequence.current;
    setLoading(true);
    setError("");
    try {
      const result = await api.bridgeRecovery();
      if (current !== listSequence.current) return;
      setRecords(result.records);
      setRunningTargets(result.runningTargets ?? []);
    } catch (reason) {
      if (current === listSequence.current) setError(readError(reason));
    } finally {
      if (current === listSequence.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    const visible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      listSequence.current++;
      document.removeEventListener("visibilitychange", visible);
    };
  }, [refresh]);
  const targets = JSON.stringify([
    ...new Set(
      records
        .map((record) => record.hostExecutable)
        .filter((path): path is string => Boolean(path)),
    ),
  ]);
  useEffect(() => {
    if (targets === "[]") return;
    let disposed = false;
    let timer = 0;
    const poll = async () => {
      if (document.visibilityState === "visible") {
        try {
          const status = await api.bridgeRecoveryStatus(JSON.parse(targets));
          if (!disposed)
            setRunningTargets((previous) =>
              JSON.stringify(previous) === JSON.stringify(status.runningTargets)
                ? previous
                : status.runningTargets,
            );
        } catch {
          /* The explicit refresh exposes errors; transient status reads preserve the last state. */
        }
      }
      if (!disposed) timer = window.setTimeout(poll, 3000);
    };
    timer = window.setTimeout(poll, 3000);
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [targets]);
  const selectedRunning = Boolean(
    selected?.hostExecutable &&
    runningTargets.includes(selected.hostExecutable),
  );
  useEffect(() => {
    if (!selected) return;
    const current = ++sequence.current;
    setPreview(undefined);
    setPreviewError("");
    if (mode === "fields" && !paths.length) {
      setPreviewing(false);
      return;
    }
    setPreviewing(true);
    const timer = window.setTimeout(() => {
      void api
        .previewBridgeRecovery(selected, mode, paths)
        .then((value) => {
          if (current === sequence.current) setPreview(value);
        })
        .catch((reason) => {
          if (current === sequence.current) setPreviewError(readError(reason));
        })
        .finally(() => {
          if (current === sequence.current) setPreviewing(false);
        });
    }, 150);
    return () => {
      clearTimeout(timer);
      sequence.current++;
    };
  }, [selected, mode, paths, generation, selectedRunning]);
  const open = (record: RecoveryRecord) => {
    setSelected(record);
    setMode(record.paths.length ? "fields" : "full");
    setPaths([]);
    setPreview(undefined);
    setPreviewError("");
  };
  const close = () => {
    if (restoring) return;
    sequence.current++;
    setSelected(undefined);
    setPreview(undefined);
  };
  const matches = Boolean(
    preview &&
    preview.mode === mode &&
    JSON.stringify([...preview.paths].sort()) ===
      JSON.stringify([...paths].sort()),
  );
  const restore = async () => {
    if (!preview || !matches || !preview.canApply || previewing || restoring)
      return;
    setRestoring(true);
    setPreviewError("");
    try {
      const result = await api.restoreBridgeConfig(preview);
      if (!result.restored) throw new Error("恢复结果尚未确认，请刷新后核对。");
      setNotice(
        result.online
          ? "所选设置已恢复并生效，其他设置保留。"
          : "配置已恢复，重新启动 BetterGI 后生效。",
      );
      setUndo(result.recoveryChangeId);
      setSelected(undefined);
      setPreview(undefined);
      await refresh();
    } catch (reason) {
      setPreviewError(readError(reason));
      setPreview(undefined);
    } finally {
      setRestoring(false);
    }
  };
  const seen = new Set<string>();
  const backups = records
    .filter((record) => !record.paths.length)
    .filter((record) => {
      const key = `${record.configPath ?? ""}:${record.snapshotDigest ?? record.changeId}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  const visible = (
    tab === "changes"
      ? records.filter((record) => record.paths.length)
      : backups
  ).filter((record) =>
    (title(record) + when(record.createdAt))
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const undoRecord = records.find(
    (record) => record.changeId === undo && record.canPreview !== false,
  );
  return (
    <div className="bridge-recovery">
      <div className="block-head">
        <button type="button" className="page-back" onClick={onBack}>
          <ChevronIcon className="button-icon" />
          BetterGI
        </button>
      </div>
      <div className="page-title">
        <div>
          <h2>配置恢复</h2>
        </div>
        <button
          className="secondary-action"
          disabled={loading || restoring}
          onClick={() => void refresh()}
        >
          {loading ? "正在读取" : "刷新记录"}
        </button>
      </div>
      {notice && (
        <div className="recovery-success" role="status">
          <span>{notice}</span>
          {undoRecord && (
            <button className="subtle-action" onClick={() => open(undoRecord)}>
              查看并撤销本次恢复
            </button>
          )}
          <button className="subtle-action" onClick={() => setNotice("")}>
            关闭提示
          </button>
        </div>
      )}
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="list-toolbar">
        <SlidingTabs
          ariaLabel="配置恢复范围"
          value={tab}
          onChange={setTab}
          items={[
            {
              id: "changes",
              name: "设置变更",
              extra: (
                <span>
                  {records.filter((record) => record.paths.length).length}
                </span>
              ),
            },
            {
              id: "backups",
              name: "完整备份",
              extra: <span>{backups.length}</span>,
            },
          ]}
        />
        <label className="search-field">
          <SearchIcon />
          <input
            aria-label="搜索恢复记录"
            placeholder="搜索设置名称或日期"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>
      {visible.length ? (
        <div className="recovery-list">
          {visible.map((record) => (
            <article className="recovery-row" key={record.changeId}>
              <div>
                <strong>{title(record)}</strong>
                <small>
                  {when(record.createdAt)} ·{" "}
                  {record.operation === "setting-restore"
                    ? "恢复前的设置"
                    : record.paths.length
                      ? `${record.paths.length} 项设置修改`
                      : "完整配置快照"}
                </small>
                {record.configPath && (
                  <small
                    className="recovery-location"
                    title={record.configPath}
                  >
                    {record.configPath}
                  </small>
                )}
                {record.reason && <p>{record.reason}</p>}
              </div>
              <button
                className="secondary-action"
                disabled={record.canPreview === false || restoring}
                onClick={() => open(record)}
              >
                查看与恢复
              </button>
            </article>
          ))}
        </div>
      ) : (
        !loading && (
          <p className="empty-note">
            {query
              ? "没有匹配的记录。"
              : tab === "changes"
                ? "还没有设置变更记录。操作前的快照可在“完整备份”中查看。"
                : "还没有完整备份。"}
          </p>
        )
      )}
      <Dialog
        open={Boolean(selected)}
        onClose={close}
        title={mode === "fields" ? "恢复所选设置" : "恢复完整备份"}
        subtitle={
          selected ? `${when(selected.createdAt)} · ${title(selected)}` : ""
        }
        className="recovery-preview-dialog"
        footer={
          <>
            <span className="recovery-preview-status">
              {previewing
                ? "正在核对…"
                : mode === "fields" && paths.length >= 20
                  ? "单次最多 20 项"
                  : ""}
            </span>
            <button
              className="subtle-action"
              disabled={restoring}
              onClick={close}
            >
              取消
            </button>
            <button
              className="primary-action"
              disabled={
                restoring || previewing || !matches || !preview?.canApply
              }
              onClick={() => void restore()}
            >
              {restoring
                ? "恢复中…"
                : mode === "full"
                  ? "恢复完整配置"
                  : `恢复 ${paths.length} 项`}
            </button>
          </>
        }
      >
        {selected?.paths.length ? (
          <div className="recovery-mode">
            <button
              className="subtle-action"
              aria-pressed={mode === "fields"}
              disabled={restoring}
              onClick={() => setMode("fields")}
            >
              所选设置
            </button>
            <button
              className="subtle-action"
              aria-pressed={mode === "full"}
              disabled={restoring}
              onClick={() => setMode("full")}
            >
              完整配置
            </button>
          </div>
        ) : null}
        {selected?.configPath && (
          <p className="recovery-target">恢复到：{selected.configPath}</p>
        )}
        {mode === "full" && (
          <p className="recovery-full-warning">
            将替换整份配置，不恢复脚本、路线或配置组。恢复前请退出 BetterGI。
          </p>
        )}
        {previewError && (
          <p className="recovery-error" role="alert">
            {previewError}
          </p>
        )}
        {preview?.reason && (
          <p className="recovery-blocker" role="status">
            {preview.reason}
          </p>
        )}
        {selected && (
          <div className="recovery-differences">
            {(mode === "fields"
              ? fields(selected)
              : (preview?.differences ?? [])
            ).map((field) => {
              const row = preview?.differences.find(
                (item) => item.path === field.path,
              );
              const checked = paths.includes(field.path);
              return (
                <section className="recovery-difference" key={field.path}>
                  <div className="recovery-difference-title">
                    {mode === "fields" ? (
                      <Checkbox
                        checked={checked}
                        disabled={restoring || (!checked && paths.length >= 20)}
                        onChange={(next) =>
                          setPaths((current) =>
                            next
                              ? [...current, field.path]
                              : current.filter((path) => path !== field.path),
                          )
                        }
                        label={field.label}
                      />
                    ) : (
                      <strong>{field.label}</strong>
                    )}
                    {row && (mode === "full" || checked) && (
                      <span>
                        {!row.changed
                          ? "无变化"
                          : row.related
                            ? "联动设置"
                            : row.laterChanged
                              ? "将覆盖后续修改"
                              : "将恢复"}
                      </span>
                    )}
                  </div>
                  {row && (mode === "full" || checked) && (
                    <div className="recovery-values">
                      <div>
                        <small>当前值</small>
                        <pre>{valueText(row.current)}</pre>
                      </div>
                      <div>
                        <small>恢复为</small>
                        <pre>{valueText(row.restore)}</pre>
                      </div>
                    </div>
                  )}
                </section>
              );
            })}
          </div>
        )}
        {selected && (mode === "full" || paths.length > 0) && (
          <button
            className="subtle-action recovery-repreview"
            disabled={previewing || restoring}
            onClick={() => setGeneration((value) => value + 1)}
          >
            刷新预览
          </button>
        )}
      </Dialog>
    </div>
  );
}
