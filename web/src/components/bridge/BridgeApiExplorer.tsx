import { useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { readError } from "../../session";
import { Toast } from "../overlay/Toast";
import { Select } from "../controls/Select";
import { ChevronIcon, SearchIcon } from "../icons";
import type {
  BridgeCatalog,
  BridgeMethod,
  BridgeMethodDetail,
} from "../../ipc/types";
import "./BridgeApiExplorer.css";
import { useT, type Text } from "../../i18n";

const groups = (t: Text): Record<string, string> => ({
  lifecycle: t.bridge.statusDiag,
  settings: t.bridge.config,
  setting: t.bridge.configItems,
  command: t.bridge.commands,
  catalog: t.apiExplorer.catalog,
});
export function BridgeApiExplorer({ onBack }: { onBack(): void }) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState("");
  const [catalog, setCatalog] = useState<BridgeCatalog>();
  const [items, setItems] = useState<BridgeMethod[]>([]);
  const [selected, setSelected] = useState<BridgeMethodDetail>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const generation = useRef(0);
  const selectionGeneration = useRef(0);
  const root = useRef<HTMLDivElement>(null);
  const listScroll = useRef(0);
  useEffect(() => {
    const revision = ++generation.current;
    const timer = setTimeout(() => {
      setBusy(true);
      setError("");
      void api
        .bridgeCatalog(query, group)
        .then((result) => {
          if (revision !== generation.current) return;
          setCatalog(result);
          setItems(result.items ?? result.methods ?? []);
        })
        .catch((reason) => {
          if (revision === generation.current) setError(readError(reason));
        })
        .finally(() => {
          if (revision === generation.current) setBusy(false);
        });
    }, 200);
    return () => {
      clearTimeout(timer);
      generation.current++;
    };
  }, [query, group]);
  const inspect = async (methodId: string) => {
    const revision = ++selectionGeneration.current;
    const scroller = root.current?.closest<HTMLElement>(".settings-content");
    listScroll.current = scroller?.scrollTop ?? 0;
    setError("");
    try {
      const detail = await api.bridgeDescribe(methodId);
      if (revision === selectionGeneration.current) {
        setSelected(detail);
        requestAnimationFrame(() =>
          scroller?.scrollTo({ top: 0, behavior: "smooth" }),
        );
      }
    } catch (reason) {
      if (revision === selectionGeneration.current) setError(readError(reason));
    }
  };
  const more = async () => {
    const revision = generation.current;
    if (catalog?.nextOffset == null) return;
    setBusy(true);
    try {
      const result = await api.bridgeCatalog(query, group, catalog.nextOffset);
      if (revision !== generation.current) return;
      setCatalog(result);
      setItems((current) => [
        ...current,
        ...(result.items ?? result.methods ?? []),
      ]);
    } catch (reason) {
      if (revision === generation.current) setError(readError(reason));
    } finally {
      if (revision === generation.current) setBusy(false);
    }
  };
  const guide = selected?.guide;
  return (
    <div className="bridge-api-explorer" ref={root}>
      <div className="bridge-api-heading">
        <button
          type="button"
          className="page-back"
          onClick={() => {
            if (selected) {
              setSelected(undefined);
              selectionGeneration.current++;
              requestAnimationFrame(() =>
                root.current
                  ?.closest<HTMLElement>(".settings-content")
                  ?.scrollTo({ top: listScroll.current, behavior: "smooth" }),
              );
            } else onBack();
          }}
        >
          <ChevronIcon className="button-icon" />
          {selected ? t.bridge.methodCatalog : "BetterGI"}
        </button>
        <span className="muted">
          {catalog && Number.isFinite(catalog.total)
            ? t.bridge.methodsCount(catalog.total)
            : t.bridge.connectToView}
        </span>
      </div>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      {selected ? (
        <article className="bridge-api-detail">
          <header>
            <h2>{selected.displayName}</h2>
            <code>{selected.methodId}</code>
            <p>{guide?.purpose ?? selected.summary}</p>
            <span className="tag">{selected.callable ? t.bridge.available : t.bridge.unavailable}</span>
            {selected.unavailableReason && (
              <p className="muted">{selected.unavailableReason}</p>
            )}
          </header>
          {guide ? (
            <>
              <section>
                <h3>{t.apiExplorer.purpose}</h3>
                <ul>
                  {guide.whenToUse.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </section>
              <section>
                <h3>{t.apiExplorer.prerequisites}</h3>
                <ul>
                  {guide.preconditions.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </section>
            </>
          ) : (
            <p className="notice">{t.apiExplorer.noDocs}</p>
          )}
          <section>
            <h3>{t.apiExplorer.params}</h3>
            {Object.keys(selected.inputSchema?.properties ?? {}).length ? (
              <div className="bridge-api-table">
                <table>
                  <thead>
                    <tr>
                      <th>{t.apiExplorer.param}</th>
                      <th>{t.apiExplorer.type}</th>
                      <th>{t.apiExplorer.requiredCol}</th>
                      <th>{t.apiExplorer.effect}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {Object.entries(selected.inputSchema.properties ?? {}).map(
                      ([name, schema]) => (
                        <tr key={name}>
                          <td>
                            <code>{name}</code>
                          </td>
                          <td>{schema.type ?? "—"}</td>
                          <td>
                            {selected.inputSchema.required?.includes(name)
                              ? t.bridge.yes
                              : t.bridge.no}
                          </td>
                          <td>{schema.description ?? "—"}</td>
                        </tr>
                      ),
                    )}
                  </tbody>
                </table>
              </div>
            ) : (
              <p>{t.apiExplorer.noParams}</p>
            )}
            <details>
              <summary>{t.apiExplorer.paramsDetail}</summary>
              <pre>{JSON.stringify(selected.inputSchema, null, 2)}</pre>
            </details>
          </section>
          {guide && (
            <>
              <section>
                <h3>{t.apiExplorer.returns}</h3>
                <p>{guide.resultMeaning}</p>
                <p>{guide.verification}</p>
                <details>
                  <summary>{t.apiExplorer.returnShape}</summary>
                  <pre>{JSON.stringify(selected.outputSchema, null, 2)}</pre>
                </details>
              </section>
              <section>
                <h3>{t.apiExplorer.impact}</h3>
                <ul>
                  {guide.sideEffects.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
                <p>{guide.rollback}</p>
              </section>
              <section>
                <h3>{t.apiExplorer.examples}</h3>
                {guide.examples.map((example, index) => (
                  <pre key={index}>
                    {JSON.stringify(
                      { methodId: selected.methodId, arguments: example },
                      null,
                      2,
                    )}
                  </pre>
                ))}
              </section>
              <footer className="muted">
                {t.apiExplorer.source(guide.documentationSource)}
                {guide.sourceReference ? " · " + guide.sourceReference : ""}
              </footer>
            </>
          )}
          {selected.errors?.length ? (
            <details>
              <summary>{t.apiExplorer.errorCodes}</summary>
              <p>{selected.errors.join(" · ")}</p>
            </details>
          ) : null}
        </article>
      ) : (
        <>
          <div className="page-title">
            <h2>{t.bridge.methodCatalog}</h2>
          </div>
          <div className="list-toolbar">
            <label className="search-field is-compact">
              <SearchIcon />
              <input
                aria-label={t.bridge.searchMethods}
                placeholder={t.common.search}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <div className="bridge-api-group">
              <Select
                label={t.bridge.methodCategory}
                value={group}
                onChange={setGroup}
                options={[
                  { value: "", label: t.bridge.allCategories },
                  ...(catalog?.groups ?? []).map((entry) => ({
                    value: entry.id,
                    label: (groups(t)[entry.id] ?? entry.id) + " · " + entry.count,
                  })),
                ]}
              />
            </div>
          </div>
          <div className="bridge-api-list">
            {items.map((item) => (
              <button
                key={item.methodId}
                className="bridge-api-row"
                onClick={() => void inspect(item.methodId)}
              >
                <span>
                  <strong>{item.displayName ?? item.methodId}</strong>
                  <code>{item.methodId}</code>
                  <p>{item.summary}</p>
                  {item.whenToUse?.[0] && (
                    <small className="bridge-api-use">
                      {t.apiExplorer.applies(item.whenToUse[0])}
                    </small>
                  )}
                </span>
                <span className="tag">
                  {item.callable
                    ? item.effect === "readOnly"
                      ? t.bridge.readOnly
                      : t.bridge.mutating
                    : t.bridge.unavailable}
                </span>
              </button>
            ))}
          </div>
          {!busy && !items.length && (
            <p className="empty-note">{t.apiExplorer.noneFound}</p>
          )}
          {busy && (
            <p className="bridge-api-status" role="status">
              <span className="bridge-api-spinner" aria-hidden="true" />
              {t.common.loading}
            </p>
          )}
          {catalog?.nextOffset != null && (
            <button
              className="secondary-action bridge-api-more"
              disabled={busy}
              onClick={() => void more()}
            >
              {t.apiExplorer.loadMore}
            </button>
          )}
        </>
      )}
    </div>
  );
}
