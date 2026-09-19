import { useEffect, useRef, useState } from "react";
import "./ExtensionsPage.css";
import { api } from "../../ipc/api";
import { Toast } from "../../components/overlay/Toast";
import { ConfirmDialog } from "../../components/overlay/ConfirmDialog";
import {
  CloseIcon,
  PluginIcon,
  SearchIcon,
  PlusIcon,
} from "../../components/icons";
import { readError } from "../../session";
import type { Bootstrap } from "../../ipc/types";
import { MotionSwitch } from "../../components/controls/MotionSwitch";
import { SlidingTabs } from "../../components/controls/SlidingTabs";
import { providerOfTool } from "../../ipc/providers";
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
  const dialog = useRef<HTMLDialogElement>(null);
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
          name: skill.name,
          description: skill.description,
          enabled: skill.enabled !== false,
          available: skill.available !== false,
          unavailableReason: skill.unavailableReason ?? "",
          meta:
            skill.source === "product"
              ? "随产品"
              : skill.source === "user"
                ? "本机"
                : skill.source,
          detail: skill.instructions ?? "",
          error: "",
          host: false,
        }))
      : bootstrap.plugins.map((plugin) => ({
          id: plugin.manifest.id,
          name: plugin.manifest.name,
          description: plugin.manifest.description ?? "",
          enabled: plugin.configuredEnabled ?? plugin.status === "enabled",
          available: plugin.status === "enabled",
          unavailableReason: plugin.error ?? "",
          meta: plugin.host ? "随产品" : plugin.manifest.version,
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
        }));
  const filtered = items.filter((item) =>
    (item.name + item.description).toLowerCase().includes(query.toLowerCase()),
  );
  const current = items.find((item) => item.id === selected);
  const toggle = (id: string, enabled: boolean) =>
    run(() =>
      currentTab === "skills"
        ? api.setSkillEnabled(id, enabled)
        : api.setPluginEnabled(id, enabled),
    );
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
        <h2>已安装</h2>
        <button
          className="secondary-action"
          onClick={() => {
            setInstall(true);
            setError("");
            dialog.current?.showModal();
          }}
        >
          <PlusIcon className="button-icon" />
          {currentTab === "skills" ? "导入技能" : "导入插件"}
        </button>
      </div>
      <div className="list-toolbar">
        <SlidingTabs
          ariaLabel="扩展类型"
          value={currentTab}
          onChange={(value) => {
            setTab(value);
            setQuery("");
            setError("");
          }}
          items={[
            {
              id: "skills",
              name: "技能",
              extra: (
                <span className="sd-tabs-count">{bootstrap.skills.length}</span>
              ),
            },
            {
              id: "plugins",
              name: "插件",
              extra: (
                <span className="sd-tabs-count">
                  {bootstrap.plugins.length}
                </span>
              ),
            },
          ]}
        />
        <label className="search-field">
          <SearchIcon />
          <input
            aria-label="搜索扩展"
            placeholder="搜索"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>
      {error && !dialog.current?.open && (
        <Toast message={error} onDismiss={() => setError("")} />
      )}
      <MotionSwitch viewKey={currentTab} kind="panel">
        {filtered.length ? (
          <div className="extension-list">
            {filtered.map((item) => (
              <div className="extension-row" key={item.id}>
                <div className="extension-glyph">
                  <PluginIcon />
                </div>
                <button
                  className="extension-summary"
                  onClick={() => {
                    setSelected(item.id);
                    setInstall(false);
                    setError("");
                    dialog.current?.showModal();
                  }}
                >
                  <strong>{item.name}</strong>
                  <span className="extension-meta">{item.meta}</span>
                  {item.description && <p>{item.description}</p>}
                  {item.error && <p>加载失败</p>}
                  {item.enabled &&
                    !item.available &&
                    item.unavailableReason && (
                      <p className="extension-flag">
                        当前未生效：{item.unavailableReason}
                      </p>
                    )}
                </button>
                <button
                  className={`switch ${item.enabled ? "on" : ""}`}
                  role="switch"
                  aria-label={item.name}
                  aria-checked={item.enabled}
                  disabled={busy}
                  onClick={() => void toggle(item.id, !item.enabled)}
                />
              </div>
            ))}
          </div>
        ) : (
          <div className="empty-state">
            <PluginIcon />
            <h3>
              {query
                ? "无匹配结果"
                : currentTab === "skills"
                  ? "暂无技能"
                  : "暂无插件"}
            </h3>
          </div>
        )}
      </MotionSwitch>
      <dialog
        ref={dialog}
        onClick={(event) => {
          if (event.target === dialog.current) {
            const r = dialog.current.getBoundingClientRect();
            if (
              event.clientX < r.left ||
              event.clientX > r.right ||
              event.clientY < r.top ||
              event.clientY > r.bottom
            )
              dialog.current.close();
          }
        }}
      >
        <div className="dialog-head">
          <h2>
            {install
              ? currentTab === "skills"
                ? "导入技能"
                : "导入插件"
              : current?.name}
          </h2>
          <button
            className="icon-button"
            aria-label="关闭"
            onClick={() => dialog.current?.close()}
          >
            <CloseIcon className="button-icon" />
          </button>
        </div>
        <div className="dialog-body">
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
                    dialog.current?.close();
                  }
                });
              }}
            >
              <label>
                <span>{currentTab === "skills" ? "技能目录" : "插件目录"}</span>
                <input
                  placeholder="文件夹的完整路径"
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                />
              </label>
              {currentTab === "skills" && (
                <p className="field-help">
                  目录里要有 SKILL.md。导入后出现在本机技能目录，可随时开关。
                </p>
              )}
              <div>
                <button
                  className="primary-action"
                  disabled={busy || !path.trim()}
                >
                  导入
                </button>
              </div>
            </form>
          ) : (
            current && (
              <>
                <p>{current.description}</p>
                {current.error && <p>{current.error}</p>}
                {currentTab === "skills" ? (
                  <section className="page-block">
                    <h3>指令</h3>
                    <pre>{current.detail || "无内容"}</pre>
                  </section>
                ) : current.host ? (
                  <p className="muted">随产品提供，不能移除。</p>
                ) : (
                  <section className="page-block">
                    <h3>提供的工具</h3>
                    <pre>{current.detail || "无内容"}</pre>
                  </section>
                )}
                {currentTab === "skills" &&
                  current.enabled &&
                  !current.available && (
                    <div className="detail-actions">
                      <button
                        className="secondary-action"
                        onClick={() => {
                          dialog.current?.close();
                          setTab("plugins");
                        }}
                      >
                        查看插件
                      </button>
                    </div>
                  )}
                {currentTab === "plugins" &&
                  current.host &&
                  current.enabled && (
                    <div className="detail-actions">
                      <button
                        className="secondary-action"
                        onClick={() => {
                          dialog.current?.close();
                          onOpenHost?.();
                        }}
                      >
                        打开设置
                      </button>
                    </div>
                  )}
                {currentTab === "plugins" && !current.host && (
                  <div className="detail-actions">
                    <button
                      className="secondary-action"
                      disabled={busy || current.enabled}
                      title={current.enabled ? "请先停用插件" : undefined}
                      onClick={() => {
                        dialog.current?.close();
                        setPendingRemove(current.id);
                      }}
                    >
                      移除插件
                    </button>
                  </div>
                )}
              </>
            )
          )}
        </div>
      </dialog>
      <ConfirmDialog
        open={pendingRemove != null}
        title="移除插件"
        confirmLabel="移除插件"
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
          移除「
          {items.find((item) => item.id === pendingRemove)?.name ?? "这个插件"}
          」。它提供的工具会从本机卸下。
        </p>
      </ConfirmDialog>
    </div>
  );
}
