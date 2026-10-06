import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { readError } from "../../session";
import { Dialog } from "../overlay/Dialog";
import { Toast } from "../overlay/Toast";
import { SlidingTabs } from "../controls/SlidingTabs";
import { Checkbox } from "../controls/Checkbox";
import { ChevronIcon, SearchIcon } from "../icons";
import type { RecoveryRecord, RecoveryPreview } from "../../ipc/types";
import { useT, type Text } from "../../i18n";
import { useLocale, type LocaleId } from "../../appearance/locale";
import "./BridgeRecovery.css";

// helper 不用 hook：文案与日期 locale 由调用方显式传入。
const when = (value: string | undefined, t: Text, locale: LocaleId) =>
  value
    ? new Date(value).toLocaleString(locale === "en" ? "en-US" : "zh-CN")
    : t.bridgeRecovery.whenUnknown;
const fields = (record: RecoveryRecord) =>
  record.fields ?? record.paths.map((path) => ({ path, label: path }));
const title = (record: RecoveryRecord, t: Text, locale: LocaleId) =>
  record.kind === "unavailable"
    ? t.bridgeRecovery.unreadableBackup
    : record.paths.length
      ? fields(record)
          .slice(0, 2)
          .map((field) => field.label)
          .join(locale === "en" ? ", " : "、") +
        (record.paths.length > 2
          ? ` ${t.bridge.backupItemsCount(
              // 英语短语是“+N more”，N 是被省略的字段数；中文“等 N 项”用总数。
              locale === "en" ? record.paths.length - 2 : record.paths.length,
            )}`
          : "")
      : record.operation === "offline-restore"
        ? t.bridgeRecovery.preRestoreFullBackup
        : t.bridgeRecovery.fullConfigBackup;
function valueText(value: unknown, t: Text) {
  const r = t.bridgeRecovery;
  return value === null || value === undefined
    ? r.notSet
    : value === ""
      ? r.emptyString
      : typeof value === "boolean"
        ? value
          ? r.on
          : r.off
        : typeof value === "object"
          ? JSON.stringify(value, null, 2)
          : String(value);
}
export function BridgeRecovery({ onBack }: { onBack(): void }) {
  const t = useT();
  const locale = useLocale();
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
      if (!result.restored)
        throw new Error(t.bridgeRecovery.restoreUnconfirmed);
      setNotice(
        result.online
          ? t.bridgeRecovery.restoredOnline
          : t.bridgeRecovery.restoredOffline,
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
    (title(record, t, locale) + when(record.createdAt, t, locale))
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
          <h2>{t.bridge.recoveryHeading}</h2>
        </div>
        <button
          className="secondary-action"
          disabled={loading || restoring}
          onClick={() => void refresh()}
        >
          {loading
            ? t.bridgeRecovery.loadingShort
            : t.bridgeRecovery.refreshRecords}
        </button>
      </div>
      {notice && (
        <div className="recovery-success" role="status">
          <span>{notice}</span>
          {undoRecord && (
            <button className="subtle-action" onClick={() => open(undoRecord)}>
              {t.bridgeRecovery.viewUndoRestore}
            </button>
          )}
          <button className="subtle-action" onClick={() => setNotice("")}>
            {t.bridgeRecovery.dismissNotice}
          </button>
        </div>
      )}
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="list-toolbar">
        <SlidingTabs
          ariaLabel={t.bridgeRecovery.scopeTabs}
          value={tab}
          onChange={setTab}
          items={[
            {
              id: "changes",
              name: t.bridgeRecovery.tabChanges,
              extra: (
                <span>
                  {records.filter((record) => record.paths.length).length}
                </span>
              ),
            },
            {
              id: "backups",
              name: t.bridgeRecovery.tabBackups,
              extra: <span>{backups.length}</span>,
            },
          ]}
        />
        <label className="search-field">
          <SearchIcon />
          <input
            aria-label={t.bridgeRecovery.searchRecords}
            placeholder={t.bridgeRecovery.searchPlaceholder}
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
                <strong>{title(record, t, locale)}</strong>
                <small>
                  {when(record.createdAt, t, locale)} ·{" "}
                  {record.operation === "setting-restore"
                    ? t.bridgeRecovery.preRestoreSetting
                    : record.paths.length
                      ? t.bridgeRecovery.changesCount(record.paths.length)
                      : t.bridgeRecovery.fullSnapshot}
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
                {t.bridgeRecovery.viewAndRestore}
              </button>
            </article>
          ))}
        </div>
      ) : (
        !loading && (
          <p className="empty-note">
            {query
              ? t.bridgeRecovery.noMatch
              : tab === "changes"
                ? t.bridgeRecovery.emptyChanges
                : t.bridgeRecovery.emptyBackups}
          </p>
        )
      )}
      <Dialog
        open={Boolean(selected)}
        onClose={close}
        title={
          mode === "fields"
            ? t.bridgeRecovery.restoreSelectedTitle
            : t.bridgeRecovery.restoreFullTitle
        }
        subtitle={
          selected
            ? `${when(selected.createdAt, t, locale)} · ${title(selected, t, locale)}`
            : ""
        }
        className="recovery-preview-dialog"
        footer={
          <>
            <span className="recovery-preview-status">
              {previewing
                ? t.bridgeRecovery.verifying
                : mode === "fields" && paths.length >= 20
                  ? t.bridgeRecovery.maxPerRestore
                  : ""}
            </span>
            <button
              className="subtle-action"
              disabled={restoring}
              onClick={close}
            >
              {t.common.cancel}
            </button>
            <button
              className="primary-action"
              disabled={
                restoring || previewing || !matches || !preview?.canApply
              }
              onClick={() => void restore()}
            >
              {restoring
                ? t.bridge.restoring
                : mode === "full"
                  ? t.bridgeRecovery.restoreFullAction
                  : t.bridgeRecovery.restoreCount(paths.length)}
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
              {t.bridgeRecovery.modeFields}
            </button>
            <button
              className="subtle-action"
              aria-pressed={mode === "full"}
              disabled={restoring}
              onClick={() => setMode("full")}
            >
              {t.bridgeRecovery.modeFull}
            </button>
          </div>
        ) : null}
        {selected?.configPath && (
          <p className="recovery-target">
            {t.bridgeRecovery.restoreTo(selected.configPath)}
          </p>
        )}
        {mode === "full" && (
          <p className="recovery-full-warning">
            {t.bridgeRecovery.fullWarning}
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
                          ? t.bridgeRecovery.noChange
                          : row.related
                            ? t.bridgeRecovery.relatedChange
                            : row.laterChanged
                              ? t.bridgeRecovery.willOverwriteLater
                              : t.bridgeRecovery.willRestore}
                      </span>
                    )}
                  </div>
                  {row && (mode === "full" || checked) && (
                    <div className="recovery-values">
                      <div>
                        <small>{t.bridgeRecovery.currentValue}</small>
                        <pre>{valueText(row.current, t)}</pre>
                      </div>
                      <div>
                        <small>{t.bridgeRecovery.restoreValue}</small>
                        <pre>{valueText(row.restore, t)}</pre>
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
            {t.bridgeRecovery.refreshPreview}
          </button>
        )}
      </Dialog>
    </div>
  );
}
