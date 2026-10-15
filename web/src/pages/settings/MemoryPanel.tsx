import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import type { MemoryEntry, MemoryExperience } from "../../ipc/types";
import { useT } from "../../i18n";
import { readError } from "../../session";
import "./MemoryPanel.css";

type Row = { entry: MemoryEntry; compatible: boolean };
const lines = (value: string) =>
  value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

export function MemoryPanel() {
  const t = useT();
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
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="settings-group memory-panel">
      <h3>{t.settings.memoryHeading}</h3>
      <p className="memory-hint">{t.settings.memoryHint}</p>
      <button
        className="subtle-action"
        disabled={busy || loading}
        onClick={() =>
          void refresh().catch((reason: unknown) => setError(readError(reason)))
        }
      >
        {t.settings.memoryRefresh}
      </button>
      {error && (
        <p className="memory-error" role="alert">
          {error}
        </p>
      )}
      {loading ? (
        <p>{t.common.loading}</p>
      ) : (
        !rows.length && <p className="memory-hint">{t.settings.memoryEmpty}</p>
      )}
      {rows.map(({ entry, compatible }) => (
        <details key={entry.id} className="memory-entry">
          <summary>{entry.experience.intent}</summary>
          {!compatible && (
            <p className="memory-hint">{t.settings.memoryChanged}</p>
          )}
          <p>{entry.experience.summary}</p>
          <p className="memory-hint">
            {entry.experience.conditions.join("；")}
          </p>
          <ol>
            {entry.experience.steps.map((step, index) => (
              <li key={index}>
                <span>{step.action}</span>
                <code>
                  {step.tool}
                  {step.methodId ? ` → ${step.methodId}` : ""}
                </code>
              </li>
            ))}
          </ol>
          <p>{entry.experience.verification.join("；")}</p>
          {entry.experience.pitfalls.length > 0 && (
            <p className="memory-hint">
              {entry.experience.pitfalls.join("；")}
            </p>
          )}
          <div className="memory-actions">
            <button
              className="subtle-action"
              disabled={busy}
              onClick={() => {
                setEditing(entry);
                setDraft(structuredClone(entry.experience));
              }}
            >
              {t.settings.memoryEdit}
            </button>
            <button
              className="subtle-action"
              disabled={busy}
              onClick={() => void forget(entry)}
            >
              {t.settings.memoryDelete}
            </button>
          </div>
        </details>
      ))}
      {editing && draft && (
        <div className="memory-editor">
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
                          ? { ...value, methodId: event.target.value || null }
                          : value,
                      ),
                    )
                  }
                />
              </label>
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
          <div className="memory-editor-actions">
            <button
              className="subtle-action"
              disabled={busy}
              onClick={() => {
                setEditing(null);
                setDraft(null);
              }}
            >
              {t.common.cancel}
            </button>
            <button
              className="subtle-action"
              disabled={busy}
              onClick={() => void save()}
            >
              {t.common.save}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
