import {
  Button,
  ConfirmDialog,
  Dialog,
  EmptyState,
  SettingsScope,
} from "@sleepy-doll/ui";
import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import type { Bootstrap, MemoryEntry, MemoryExperience } from "../../ipc/types";
import { useT } from "../../i18n";
import { readError } from "../../session";
import { ChevronIcon, HistoryIcon } from "@sleepy-doll/ui/icons";
import { MemoryToggle, useMemoryState } from "./MemoryControls";
import { MarkdownText } from "../../components/chat/Transcript";
import { providerOfTool } from "../../ipc/providers";
import "./MemoryPage.css";

type Row = { entry: MemoryEntry; compatible: boolean };
const lines = (value: string) =>
  value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

const normalized = (value: string) =>
  value.replace(/\s+/g, "").replace(/[。.!！?？]+$/u, "");

function memoryTitle(
  entry: MemoryEntry,
  bootstrap?: Pick<Bootstrap, "plugins" | "tools">,
) {
  const experience = entry.experience;
  const title = experience.title?.trim();
  if (title && Array.from(title).length <= 24) return title;
  const intent = experience.intent.trim();
  const shortIntent =
    Array.from(intent).length <= 20 && !/[\n，,；;。]/u.test(intent);
  if (entry.userEdited && shortIntent) return intent;
  const namespaces = new Set(
    experience.steps.map((step) => step.tool.split(".")[0]?.toLowerCase()),
  );
  const software = experience.steps
    .map((step) => {
      const tool = bootstrap?.tools.find((tool) => tool.name === step.tool);
      const provider = tool
        ? providerOfTool(tool.name, tool.source)
        : undefined;
      return bootstrap?.plugins.find(
        (plugin) => plugin.manifest.id === provider,
      )?.manifest.name;
    })
    .find((name) => name && Array.from(name).length <= 12);
  const words = [
    ...new Set(experience.keywords.map((word) => word.trim())),
  ].filter(
    (word) =>
      Array.from(word).length >= 2 &&
      Array.from(word).length <= 10 &&
      !/[.:/\\{}\[\]，；;。]/u.test(word) &&
      !namespaces.has(word.toLowerCase()) &&
      word !== software,
  );
  if (words.length) {
    if (!software) return words.slice(0, 2).join(" · ");
    const word = words.find(
      (word) => Array.from(`${software} · ${word}`).length <= 24,
    );
    return word ? `${software} · ${word}` : `${software}操作`;
  }
  if (shortIntent) return intent;
  return software ? `${software}操作` : "软件操作经验";
}

export function MemoryPage({
  bootstrap,
}: {
  bootstrap?: Pick<Bootstrap, "plugins" | "tools">;
}) {
  const t = useT();
  const memoryState = useMemoryState();
  const [forgetting, setForgetting] = useState<MemoryEntry | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [reviewing, setReviewing] = useState(false);
  const [editing, setEditing] = useState<MemoryEntry | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState<MemoryExperience | null>(null);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Row | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const refresh = async () => setRows((await api.memories()).entries);
  useEffect(() => {
    let alive = true;
    let reading = false;
    let revision: string | undefined;
    const load = async () => {
      if (reading || !alive) return;
      reading = true;
      try {
        const state = await api.memoryState();
        if (!alive) return;
        setReviewing(state.reviewing === true);
        if (revision == null || state.revision !== revision) {
          const result = await api.memories();
          if (alive) {
            setRows(result.entries);
            revision = state.revision;
          }
        }
      } catch (reason) {
        if (alive) setError(readError(reason));
      } finally {
        reading = false;
        if (alive) setLoading(false);
      }
    };
    const visible = () => {
      if (!document.hidden) void load();
    };
    void load();
    const timer = window.setInterval(visible, 2000);
    window.addEventListener("focus", visible);
    document.addEventListener("visibilitychange", visible);
    return () => {
      alive = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", visible);
      document.removeEventListener("visibilitychange", visible);
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
      setEditorOpen(false);
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
      setDetailOpen(false);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  const current =
    rows.find(({ entry }) => entry.id === selected?.entry.id) ?? selected;
  const overview = current
    ? [
        current.entry.experience.summary,
        current.entry.experience.intent,
      ].filter(
        (value, index, values) =>
          value.trim() &&
          normalized(value) !==
            normalized(memoryTitle(current.entry, bootstrap)) &&
          values.findIndex(
            (other) => normalized(other) === normalized(value),
          ) === index,
      )
    : [];
  return (
    <SettingsScope className="memory-page">
      <header className="page-title memory-page-header">
        <div>
          <h1>{t.settings.memoryHeading}</h1>
        </div>
        <MemoryToggle compact />
      </header>
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
            title={
              reviewing ? t.settings.memoryReviewing : t.settings.memoryNone
            }
            description={
              reviewing
                ? t.settings.memoryReviewingHint
                : memoryState.enabled
                  ? t.settings.memoryEmpty
                  : t.settings.memoryDisabled
            }
          />
        ) : null}
        {reviewing && rows.length > 0 && (
          <p className="memory-hint memory-reviewing" role="status">
            {t.settings.memoryReviewing}
          </p>
        )}
        <div className="memory-list">
          {rows.map(({ entry, compatible }) => {
            const title = memoryTitle(entry, bootstrap);
            const intro = entry.experience.summary.trim();
            return (
              <div key={entry.id} className="memory-entry list-item">
                <button
                  type="button"
                  className="memory-entry-trigger list-item-trigger"
                  aria-label={title}
                  aria-haspopup="dialog"
                  onClick={() => {
                    setError("");
                    setSelected({ entry, compatible });
                    setDetailOpen(true);
                  }}
                >
                  <span className="memory-entry-copy">
                    <span className="memory-entry-title">{title}</span>
                    {intro && normalized(intro) !== normalized(title) && (
                      <span className="memory-entry-intro">{intro}</span>
                    )}
                  </span>
                  <ChevronIcon className="memory-entry-chevron" />
                </button>
              </div>
            );
          })}
        </div>
      </div>
      <Dialog
        open={detailOpen && current !== null}
        compact
        className="memory-detail-dialog"
        title={current ? memoryTitle(current.entry, bootstrap) : ""}
        initialFocus="dialog"
        onClose={() => setDetailOpen(false)}
        footer={
          current && (
            <>
              <Button
                variant="subtle"
                className="memory-detail-forget"
                disabled={busy}
                onClick={() => {
                  setError("");
                  setForgetting(current.entry);
                }}
              >
                {t.settings.memoryDelete}
              </Button>
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  setError("");
                  setEditing(current.entry);
                  setDraft({
                    ...structuredClone(current.entry.experience),
                    title: memoryTitle(current.entry, bootstrap),
                  });
                  setEditorOpen(true);
                }}
              >
                {t.settings.memoryEdit}
              </Button>
            </>
          )
        }
      >
        {current && (
          <SettingsScope className="memory-detail-content">
            <p className="memory-detail-status">
              {[
                current.entry.validationState === "verified"
                  ? t.settings.memoryVerified
                  : t.settings.memoryCandidate,
                !current.compatible ? t.settings.memoryChanged : "",
              ]
                .filter(Boolean)
                .join(" · ")}
            </p>
            {overview.length > 0 && (
              <section className="memory-detail-section">
                <h3>{t.settings.memoryOverview}</h3>
                {overview.map((value, index) => (
                  <MarkdownText key={index} text={value} />
                ))}
              </section>
            )}
            {current.entry.experience.conditions.length > 0 && (
              <section className="memory-detail-section">
                <h3>{t.settings.memoryConditionsHeading}</h3>
                <ul>
                  {current.entry.experience.conditions.map((value, index) => (
                    <li key={index}>
                      <MarkdownText text={value} />
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {current.entry.experience.steps.length > 0 && (
              <section className="memory-detail-section">
                <h3>{t.settings.memorySteps}</h3>
                <ol>
                  {current.entry.experience.steps.map((step, index) => (
                    <li key={index}>
                      <MarkdownText text={step.action} />
                    </li>
                  ))}
                </ol>
              </section>
            )}
            {current.entry.experience.verification.length > 0 && (
              <section className="memory-detail-section">
                <h3>{t.settings.memoryVerificationHeading}</h3>
                <ul>
                  {current.entry.experience.verification.map((value, index) => (
                    <li key={index}>
                      <MarkdownText text={value} />
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {current.entry.experience.pitfalls.length > 0 && (
              <section className="memory-detail-section">
                <h3>{t.settings.memoryPitfallsHeading}</h3>
                <ul>
                  {current.entry.experience.pitfalls.map((value, index) => (
                    <li key={index}>
                      <MarkdownText text={value} />
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {current.entry.experience.steps.length > 0 && (
              <details className="memory-advanced memory-detail-advanced">
                <summary>{t.settings.memoryAdvanced}</summary>
                <ol>
                  {current.entry.experience.steps.map((step, index) => (
                    <li key={index}>
                      <code>
                        {step.tool}
                        {step.methodId ? ` → ${step.methodId}` : ""}
                      </code>
                      {Object.entries(step.inputRoles).length > 0 && (
                        <dl className="memory-role-list">
                          {Object.entries(step.inputRoles).map(
                            ([name, role]) => (
                              <div key={name}>
                                <dt>
                                  <code>{name}</code>
                                </dt>
                                <dd>{role}</dd>
                              </div>
                            ),
                          )}
                        </dl>
                      )}
                    </li>
                  ))}
                </ol>
              </details>
            )}
          </SettingsScope>
        )}
      </Dialog>
      {editing && draft && (
        <Dialog
          open={editorOpen}
          compact
          className="memory-editor-dialog"
          footer={
            <>
              <Button
                variant="subtle"
                disabled={busy}
                onClick={() => {
                  setEditorOpen(false);
                }}
              >
                {t.common.cancel}
              </Button>
              <Button
                variant="primary"
                disabled={
                  busy ||
                  !draft.title?.trim() ||
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
              setEditorOpen(false);
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
              {t.settings.memoryTitle}
              <input
                maxLength={24}
                value={draft.title ?? ""}
                onChange={(event) => change("title", event.target.value)}
              />
            </label>
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
