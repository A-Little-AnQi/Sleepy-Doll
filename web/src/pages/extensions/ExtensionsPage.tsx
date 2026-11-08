import { ExtensionList, ExtensionRow } from "@sleepy-doll/ui";
import { SearchField, EmptyState, FormField } from "@sleepy-doll/ui";
import { Button, Dialog } from "@sleepy-doll/ui";
import { useEffect, useRef, useState } from "react";
import "./ExtensionsPage.css";
import { api } from "../../ipc/api";
import { Toast } from "@sleepy-doll/ui/overlay/Toast";
import { ConfirmDialog } from "@sleepy-doll/ui/overlay/ConfirmDialog";
import { PluginIcon, PlusIcon } from "@sleepy-doll/ui/icons";
import { readError } from "../../session";
import type { Bootstrap } from "../../ipc/types";
import { MotionSwitch } from "@sleepy-doll/ui/controls/MotionSwitch";
import { SlidingTabs } from "@sleepy-doll/ui/controls/SlidingTabs";
import { providerOfTool } from "../../ipc/providers";
import { useT } from "../../i18n";
import { skillLabel } from "../../components/chat/SkillMenu";
export function ExtensionsPage({
  bootstrap,
  reload,
  tab = "skills",
  onTab,
  onOpenHost,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  tab?: "skills" | "plugins";
  onTab?(tab: "skills" | "plugins"): void;
  onOpenHost?(): void;
}) {
  const t = useT();
  const [uncontrolled, setUncontrolled] = useState<"skills" | "plugins">(tab);
  const currentTab = onTab ? tab : uncontrolled;
  const setTab = (value: "skills" | "plugins") => {
    if (onTab) onTab(value);
    else setUncontrolled(value);
  };
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState("");
  const [install, setInstall] = useState(false);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [toggling, setToggling] = useState<Map<string, boolean>>(
    () => new Map(),
  );
  const pending = useRef(new Set<string>());
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError("");
    try {
      await action();
      await reload();
      return true;
    } catch (reason) {
      setError(readError(reason));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const items =
    currentTab === "skills"
      ? bootstrap.skills.map((skill) => ({
          id: skill.name,
          name: skillLabel(skill),
          description: skill.description,
          enabled: skill.enabled !== false,
          available: skill.available !== false,
          unavailableReason: skill.unavailableReason ?? "",
          meta:
            skill.source === "product"
              ? t.extensions.bundled
              : skill.source === "user"
                ? t.extensions.local
                : skill.source,
          detail: skill.instructions ?? "",
          error: "",
          host: false,
          bundled: false,
        }))
      : bootstrap.plugins.map((plugin) => ({
          id: plugin.manifest.id,
          name: plugin.manifest.name,
          description: plugin.manifest.description ?? "",
          enabled: plugin.configuredEnabled ?? plugin.status === "enabled",
          available: plugin.status === "enabled",
          unavailableReason: plugin.error ?? "",
          meta: plugin.host ? t.extensions.bundled : plugin.manifest.version,
          detail: plugin.host
            ? ""
            : bootstrap.tools
                .filter(
                  (tool) =>
                    providerOfTool(tool.name, tool.source) ===
                    plugin.manifest.id,
                )
                .map((tool) => tool.description || tool.name)
                .join("\n\n"),
          error: plugin.error ?? "",
          host: plugin.host === true,
          bundled: plugin.bundled === true || plugin.host === true,
        }));
  const filtered = items.filter((item) =>
    (item.name + item.id + item.description)
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const current = items.find((item) => item.id === selected);
  const toggle = async (id: string, enabled: boolean) => {
    const key = `${currentTab}:${id}`;
    if (pending.current.has(key)) return;
    pending.current.add(key);
    setToggling((current) => new Map(current).set(key, enabled));
    setError("");
    try {
      if (currentTab === "skills") await api.setSkillEnabled(id, enabled);
      else await api.setPluginEnabled(id, enabled);
      await reload();
    } catch (reason) {
      setError(readError(reason));
    } finally {
      pending.current.delete(key);
      setToggling((current) => {
        const next = new Map(current);
        next.delete(key);
        return next;
      });
    }
  };
  useEffect(() => {
    let cancelled = false;
    const scan = () => {
      void api
        .reloadExtensions()
        .then(() => (cancelled ? undefined : reload()))
        .catch((reason) => {
          if (!cancelled) setError(readError(reason));
        });
    };
    scan();
    const onVisible = () => {
      if (document.visibilityState === "visible") scan();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [reload]);
  return (
    <div className="page-sheet extensions-page">
      <div className="page-title">
        <h2>{t.extensions.installed}</h2>
        <Button
          variant="secondary"
          onClick={() => {
            setInstall(true);
            setError("");
            setOpen(true);
          }}
        >
          <PlusIcon className="button-icon" />
          {currentTab === "skills"
            ? t.extensions.importSkills
            : t.extensions.importPlugins}
        </Button>
      </div>
      <div className="list-toolbar page-list-toolbar">
        <SlidingTabs
          ariaLabel={t.extensions.kind}
          value={currentTab}
          onChange={(value) => {
            setTab(value);
            setQuery("");
            setError("");
          }}
          items={[
            {
              id: "skills",
              name: t.extensions.skills,
              extra: (
                <span className="sd-tabs-count">{bootstrap.skills.length}</span>
              ),
            },
            {
              id: "plugins",
              name: t.extensions.plugins,
              extra: (
                <span className="sd-tabs-count">
                  {bootstrap.plugins.length}
                </span>
              ),
            },
          ]}
        />
        <SearchField
          aria-label={t.extensions.search}
          placeholder={t.common.search}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      {error && !open && (
        <Toast message={error} onDismiss={() => setError("")} />
      )}
      <MotionSwitch viewKey={currentTab} kind="panel">
        {filtered.length ? (
          <ExtensionList>
            {filtered.map((item) => (
              <ExtensionRow
                key={item.id}
                name={item.name}
                meta={item.meta}
                description={item.description}
                error={item.error ? t.extensions.loadFailed : ""}
                flag={
                  item.enabled && !item.available && item.unavailableReason
                    ? "当前未生效：" + item.unavailableReason
                    : ""
                }
                enabled={
                  toggling.get(`${currentTab}:${item.id}`) ?? item.enabled
                }
                disabled={toggling.has(`${currentTab}:${item.id}`)}
                onOpen={() => {
                  setSelected(item.id);
                  setInstall(false);
                  setError("");
                  setOpen(true);
                }}
                onChange={(enabled) => void toggle(item.id, enabled)}
              />
            ))}
          </ExtensionList>
        ) : (
          <EmptyState
            icon={<PluginIcon />}
            title={
              <>
                {query
                  ? t.extensions.noMatch
                  : currentTab === "skills"
                    ? t.extensions.noSkills
                    : t.extensions.noPlugins}
              </>
            }
          ></EmptyState>
        )}
      </MotionSwitch>
      <Dialog
        open={open}
        compact
        className="extensions-dialog"
        initialFocus={install ? "field" : "dialog"}
        onClose={() => setOpen(false)}
        title={
          install
            ? currentTab === "skills"
              ? t.extensions.importSkills
              : t.extensions.importPlugins
            : (current?.name ?? "")
        }
      >
        {error && <Toast message={error} onDismiss={() => setError("")} />}
        {install ? (
          <form
            className="form-section"
            onSubmit={(event) => {
              event.preventDefault();
              void run(() =>
                currentTab === "skills"
                  ? api.installSkill(path)
                  : api.installPlugin(path),
              ).then((ok) => {
                if (ok) {
                  setPath("");
                  setOpen(false);
                }
              });
            }}
          >
            <FormField
              label={
                <>
                  {currentTab === "skills"
                    ? t.extensions.skillsDir
                    : t.extensions.pluginsDir}
                </>
              }
            >
              <input
                placeholder={t.extensions.dirHint}
                value={path}
                onChange={(event) => setPath(event.target.value)}
              />
            </FormField>
            {currentTab === "skills" && (
              <p className="field-help">
                所选目录须包含
                SKILL.md。导入后保存到本机技能目录，可在此启用或停用。
              </p>
            )}
            <div>
              <Button variant="primary" disabled={busy || !path.trim()}>
                {currentTab === "skills" ? "导入技能" : "导入插件"}
              </Button>
            </div>
          </form>
        ) : (
          current && (
            <>
              <p>{current.description}</p>
              {current.error && <p>{current.error}</p>}
              {currentTab === "skills" ? (
                <section className="page-block">
                  <h3>{t.extensions.commandsHeading}</h3>
                  <pre>{current.detail || t.extensions.nothing}</pre>
                </section>
              ) : current.host ? (
                <p className="muted">随产品提供，不能移除。</p>
              ) : (
                <section className="page-block">
                  <h3>{t.extensions.toolsHeading}</h3>
                  <pre>{current.detail || t.extensions.nothing}</pre>
                </section>
              )}
              {currentTab === "skills" &&
                current.enabled &&
                !current.available && (
                  <div className="detail-actions">
                    <Button
                      variant="secondary"
                      onClick={() => {
                        setOpen(false);
                        setTab("plugins");
                      }}
                    >
                      查看插件
                    </Button>
                  </div>
                )}
              {currentTab === "plugins" && current.host && current.enabled && (
                <div className="detail-actions">
                  <Button
                    variant="secondary"
                    onClick={() => {
                      setOpen(false);
                      onOpenHost?.();
                    }}
                  >
                    打开设置
                  </Button>
                </div>
              )}
              {currentTab === "plugins" && !current.bundled && (
                <div className="detail-actions">
                  <Button
                    variant="secondary"
                    disabled={busy || current.enabled}
                    title={
                      current.enabled ? t.extensions.disableFirst : undefined
                    }
                    onClick={() => {
                      setOpen(false);
                      setPendingRemove(current.id);
                    }}
                  >
                    移除插件
                  </Button>
                </div>
              )}
            </>
          )
        )}
      </Dialog>
      <ConfirmDialog
        open={pendingRemove != null}
        title={t.extensions.remove}
        confirmLabel={t.extensions.remove}
        busy={busy}
        onClose={() => setPendingRemove(null)}
        onConfirm={() => {
          const id = pendingRemove;
          if (!id) return;
          void run(() => api.removePlugin(id)).then((ok) => {
            if (ok) setPendingRemove(null);
          });
        }}
      >
        <p>
          {t.extensions.removeNote(
            items.find((item) => item.id === pendingRemove)?.name ?? "",
          )}
        </p>
      </ConfirmDialog>
    </div>
  );
}
