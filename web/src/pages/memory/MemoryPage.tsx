import {
  Button,
  ConfirmDialog,
  Dialog,
  EmptyState,
  SearchField,
  SettingsScope,
} from "@sleepy-doll/ui";
import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import type { MemoryEntry, MemoryExperience } from "../../ipc/types";
import { useT } from "../../i18n";
import { readError } from "../../session";
import { HistoryIcon } from "@sleepy-doll/ui/icons";
import { MemoryToggle, useMemoryState } from "./MemoryControls";
import "./MemoryPage.css";

type Row = { entry: MemoryEntry; compatible: boolean };
const lines = (value: string) =>
  value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

export function MemoryPage() {
  const t = useT();
  const memoryState = useMemoryState();
  const [query, setQuery] = useState("");
  const [forgetting, setForgetting] = useState<MemoryEntry | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<MemoryEntry | null>(null);
  const [draft, setDraft] = useState<MemoryExperience | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = async () => setRows((await api.memories()).entries);
  useEffect(() => {
    let alive = true;
    api
      .memories()
      .then((result) => {
        if (alive) setRows(result.entries);
      })
      .catch((reason: unknown) => {
        if (alive) setError(readError(reason));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);
  const change = <K extends keyof MemoryExperience>(
    key: K,
    value: MemoryExperience[K],
  ) =>
    setDraft((previous) =>
      previous ? { ...previous, [key]: value } : previous,
    );
  const save = async () => {
    if (!editing || !draft || busy) return;
    setBusy(true);
    setError("");
    try {
      await api.memoryUpdate(editing.id, editing.revision, draft);
      await refresh();
      setEditing(null);
      setDraft(null);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  const forget = async (entry: MemoryEntry) => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await api.memoryDelete(entry.id, entry.revision);
      await refresh();
      setForgetting(null);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  const filteredRows = rows.filter(({ entry }) =>
    `${entry.experience.intent} ${entry.experience.summary} ${entry.experience.keywords.join(" ")}`
      .toLocaleLowerCase()
      .includes(query.toLocaleLowerCase().trim()),
  );
  return (
    <SettingsScope className="memory-page">
      <header className="memory-page-header">
        <div>
          <h1>{t.settings.memoryHeading}</h1>
          <p className="memory-hint">{t.settings.memoryDescription}</p>
        </div>
        <MemoryToggle compact />
      </header>
      <div className="memory-toolbar">
        <SearchField
          aria-label={t.settings.memorySearch}
          placeholder={t.settings.memorySearch}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className="memory-toolbar-actions">
          <span className="memory-hint">
            {t.settings.memoryCount(rows.length)}
          </span>
          <Button
            variant="subtle"
            disabled={busy || loading}
            onClick={() => {
              setLoading(true);
              setError("");
              void refresh()
                .catch((reason: unknown) => setError(readError(reason)))
                .finally(() => setLoading(false));
            }}
          >
            {t.settings.memoryRefresh}
          </Button>
        </div>
      </div>
      {error && (
        <p className="memory-error" role="alert">
          {error}
        </p>
      )}
      <div className="memory-content">
        {loading ? (
          <p className="memory-loading" role="status">
            {t.common.loading}
          </p>
        ) : rows.length === 0 && !error ? (
          <EmptyState
            icon={<HistoryIcon />}
            title={t.settings.memoryNone}
            description={
              memoryState.enabled
                ? t.settings.memoryEmpty
                : t.settings.memoryDisabled
            }
          />
        ) : filteredRows.length === 0 && !error ? (
          <EmptyState icon={<HistoryIcon />} title={t.settings.memoryNoMatch} />
        ) : null}
        <div className="memory-list">
          {filteredRows.map(({ entry, compatible }) => (
            <details key={entry.id} className="memory-entry">
              <summary>
                <span className="memory-entry-title">
                  {entry.experience.intent}
                </span>
                <span className="memory-entry-intro">
                  {entry.experience.summary}
                </span>
              </summary>
              <div className="memory-entry-body">
                <p className="memory-hint">
                  {entry.validationState === "verified"
                    ? t.settings.memoryVerified
                    : t.settings.memoryCandidate}
                </p>
                {!compatible && (
                  <p className="memory-hint">{t.settings.memoryChanged}</p>
                )}
                <p className="memory-hint">
                  {entry.experience.conditions.join("；")}
                </p>
                <ol>
                  {entry.experience.steps.map((step, index) => (
                    <li key={index}>
                      <span>{step.action}</span>
                    </li>
                  ))}
                </ol>
                <details className="memory-advanced">
                  <summary>{t.settings.memoryAdvanced}</summary>
                  {entry.experience.steps.map((step, index) => (
                    <code key={index}>
                      {step.tool}
                      {step.methodId ? ` → ${step.methodId}` : ""}
                    </code>
                  ))}
                </details>
                <p>{entry.experience.verification.join("；")}</p>
                {entry.experience.pitfalls.length > 0 && (
                  <p className="memory-hint">
                    {entry.experience.pitfalls.join("；")}
                  </p>
                )}
                <div className="memory-actions">
                  <Button
                    variant="subtle"
                    disabled={busy}
                    onClick={() => {
                      setError("");
                      setEditing(entry);
                      setDraft(structuredClone(entry.experience));
                    }}
                  >
                    {t.settings.memoryEdit}
                  </Button>
                  <Button
                    variant="subtle"
                    disabled={busy}
                    onClick={() => {
                      setError("");
                      setForgetting(entry);
                    }}
                  >
                    {t.settings.memoryDelete}
                  </Button>
                </div>
              </div>
            </details>
          ))}
        </div>
      </div>
      {editing && draft && (
        <Dialog
          open
          compact
          className="memory-editor-dialog"
          footer={
            <>
              <Button
                variant="subtle"
                disabled={busy}
                onClick={() => {
                  setEditing(null);
                  setDraft(null);
                }}
              >
                {t.common.cancel}
              </Button>
              <Button
                variant="primary"
                disabled={
                  busy ||
                  !draft.summary.trim() ||
                  Array.from(draft.summary).length > 240 ||
                  !draft.intent.trim()
                }
                onClick={() => void save()}
              >
                {t.common.save}
              </Button>
            </>
          }
          title={t.settings.memoryEdit}
          onClose={() => {
            if (!busy) {
              setEditing(null);
              setDraft(null);
            }
          }}
        >
          <SettingsScope className="memory-editor">
            {error && (
              <p role="alert" className="memory-error">
                {error}
              </p>
            )}
            <label>
              {t.settings.memoryIntent}
              <input
                value={draft.intent}
                onChange={(event) => change("intent", event.target.value)}
              />
            </label>
            <label>
              {t.settings.memorySummary}
              <textarea
                required
                maxLength={240}
                value={draft.summary}
                onChange={(event) => change("summary", event.target.value)}
              />
            </label>
            <label>
              {t.settings.memoryConditions}
              <textarea
                value={draft.conditions.join("\n")}
                onChange={(event) =>
                  change("conditions", lines(event.target.value))
                }
              />
            </label>
            <h4>{t.settings.memorySteps}</h4>
            {draft.steps.map((step, index) => (
              <div className="memory-step-editor" key={index}>
                <textarea
                  aria-label={t.settings.memorySteps}
                  value={step.action}
                  onChange={(event) =>
                    change(
                      "steps",
                      draft.steps.map((value, at) =>
                        at === index
                          ? { ...value, action: event.target.value }
                          : value,
                      ),
                    )
                  }
                />
                <details className="memory-advanced">
                  <summary>{t.settings.memoryAdvanced}</summary>
                  <label>
                    {t.settings.memoryTool}
                    <input
                      value={step.tool}
                      onChange={(event) =>
                        change(
                          "steps",
                          draft.steps.map((value, at) =>
                            at === index
                              ? { ...value, tool: event.target.value }
                              : value,
                          ),
                        )
                      }
                    />
                  </label>
                  <label>
                    {t.settings.memoryMethod}
                    <input
                      value={step.methodId ?? ""}
                      onChange={(event) =>
                        change(
                          "steps",
                          draft.steps.map((value, at) =>
                            at === index
                              ? {
                                  ...value,
                                  methodId: event.target.value || null,
                                }
                              : value,
                          ),
                        )
                      }
                    />
                  </label>

                  {Object.entries(step.inputRoles).map(([name, role]) => (
                    <label key={name}>
                      {`${t.settings.memoryRoles} · ${name}`}
                      <input
                        value={role}
                        onChange={(event) =>
                          change(
                            "steps",
                            draft.steps.map((value, at) =>
                              at === index
                                ? {
                                    ...value,
                                    inputRoles: {
                                      ...value.inputRoles,
                                      [name]: event.target.value,
                                    },
                                  }
                                : value,
                            ),
                          )
                        }
                      />
                    </label>
                  ))}
                </details>
              </div>
            ))}
            {(["verification", "pitfalls", "anchors"] as const).map((key) => (
              <label key={key}>
                {key === "verification"
                  ? t.settings.memoryVerification
                  : key === "pitfalls"
                    ? t.settings.memoryPitfalls
                    : t.settings.memoryAnchors}
                <textarea
                  value={draft[key].join("\n")}
                  onChange={(event) => change(key, lines(event.target.value))}
                />
              </label>
            ))}
          </SettingsScope>
        </Dialog>
      )}
      <ConfirmDialog
        open={forgetting !== null}
        title={t.settings.memoryForgetTitle}
        confirmLabel={t.settings.memoryDelete}
        busy={busy}
        onClose={() => setForgetting(null)}
        onConfirm={() => {
          if (forgetting) void forget(forgetting);
        }}
      >
        {error && (
          <p role="alert" className="memory-error">
            {error}
          </p>
        )}
        <p>{forgetting?.experience.intent}</p>
        <p>{t.settings.memoryForgetHint}</p>
      </ConfirmDialog>
    </SettingsScope>
  );
}
