import { useRef, useState } from "react";
import "./ExtensionsPage.css";
import { api } from "../api";
import { Toast } from "../components/Toast";
import {
  CloseIcon,
  PluginIcon,
  RefreshIcon,
  SearchIcon,
  PlusIcon,
} from "../components/icons";
import { readError } from "../session";
import type { Bootstrap } from "../types";
export function ExtensionsPage({
  bootstrap,
  reload,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}) {
  const [tab, setTab] = useState<"skills" | "plugins">("skills");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState("");
  const [install, setInstall] = useState(false);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
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
    tab === "skills"
      ? bootstrap.skills.map((skill) => ({
          id: skill.name,
          name: skill.name,
          description: skill.description,
          enabled: skill.enabled !== false,
          available: skill.available !== false,
          unavailableReason: skill.unavailableReason ?? "",
          meta: skill.source,
          detail: skill.instructions ?? "",
          error: "",
        }))
      : bootstrap.plugins.map((plugin) => ({
          id: plugin.manifest.id,
          name: plugin.manifest.name,
          description: plugin.manifest.description ?? "",
          enabled: plugin.configuredEnabled ?? plugin.status === "enabled",
          available: plugin.status === "enabled",
          unavailableReason: plugin.error ?? "",
          meta: plugin.manifest.version,
          detail: bootstrap.tools
            .filter((tool) => tool.source.includes(plugin.manifest.id))
            .map((tool) => tool.name + "\n" + tool.description)
            .join("\n\n"),
          error: plugin.error ?? "",
        }));
  const filtered = items.filter((item) =>
    (item.name + item.description).toLowerCase().includes(query.toLowerCase()),
  );
  const current = items.find((item) => item.id === selected);
  const toggle = (id: string, enabled: boolean) =>
    run(() =>
      tab === "skills"
        ? api.setSkillEnabled(id, enabled)
        : api.setPluginEnabled(id, enabled),
    );
  return (
    <div className="page-sheet extensions-page">
      <div className="page-title">
        <h2>已安装</h2>
        <div className="detail-actions">
          <button
            className="icon-button"
            aria-label="刷新扩展"
            title="刷新"
            disabled={busy}
            onClick={() => void run(api.reloadExtensions)}
          >
            <RefreshIcon className="button-icon" />
          </button>
          {tab === "plugins" && (
            <button
              className="secondary-action"
              onClick={() => {
                setInstall(true);
                setError("");
                dialog.current?.showModal();
              }}
            >
              <PlusIcon className="button-icon" />
              导入插件
            </button>
          )}
        </div>
      </div>
      <div className="list-toolbar">
        <div className="segmented" role="tablist" aria-label="扩展类型">
          {(["skills", "plugins"] as const).map((value) => (
            <button
              key={value}
              role="tab"
              aria-selected={tab === value}
              className={tab === value ? "is-active" : ""}
              onClick={() => {
                setTab(value);
                setQuery("");
                setError("");
              }}
            >
              {value === "skills" ? "技能" : "插件"}
              <span>{bootstrap[value].length}</span>
            </button>
          ))}
        </div>
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
                {/* 开着但依赖不在线时，说清楚现在没生效，而不是让开关骗人。 */}
                {item.enabled && !item.available && item.unavailableReason && (
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
            {query ? "无匹配结果" : tab === "skills" ? "暂无技能" : "暂无插件"}
          </h3>
        </div>
      )}
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
          <h2>{install ? "导入插件" : current?.name}</h2>
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
                void run(() => api.installPlugin(path)).then((ok) => {
                  if (ok) {
                    setPath("");
                    dialog.current?.close();
                  }
                });
              }}
            >
              <label>
                <span>插件目录</span>
                <input
                  placeholder="文件夹的完整路径"
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                />
              </label>
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
                <section className="page-block">
                  <h3>{tab === "skills" ? "指令" : "提供的工具"}</h3>
                  <pre>{current.detail || "无内容"}</pre>
                </section>
                {tab === "plugins" && (
                  <div className="detail-actions">
                    <button
                      className="secondary-action"
                      disabled={busy || current.enabled}
                      title={current.enabled ? "请先停用插件" : undefined}
                      onClick={() =>
                        void run(() => api.removePlugin(current.id)).then(
                          (ok) => {
                            if (ok) dialog.current?.close();
                          },
                        )
                      }
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
    </div>
  );
}
