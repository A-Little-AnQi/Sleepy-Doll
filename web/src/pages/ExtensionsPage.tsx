import { useMemo, useState } from "react";

import { api } from "../api";
import { AlertIcon, RefreshIcon } from "../components/icons";
import type { Bootstrap } from "../types";

type Tab = "skills" | "plugins";

const pluginStatus: Record<string, string> = {
  enabled: "运行中",
  disabled: "已停用",
  failed: "加载失败",
};

interface Props {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}

export function ExtensionsPage({ bootstrap, reload }: Props) {
  const [tab, setTab] = useState<Tab>("skills");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(
    bootstrap.skills[0]?.name ?? bootstrap.plugins[0]?.manifest.id ?? "",
  );
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [installPath, setInstallPath] = useState("");
  const [busy, setBusy] = useState(false);

  const skills = useMemo(
    () =>
      bootstrap.skills.filter((skill) =>
        `${skill.name} ${skill.description} ${skill.tags.join(" ")}`
          .toLowerCase()
          .includes(query.toLowerCase()),
      ),
    [bootstrap.skills, query],
  );
  const plugins = useMemo(
    () =>
      bootstrap.plugins.filter((plugin) =>
        `${plugin.manifest.name} ${plugin.manifest.description ?? ""}`
          .toLowerCase()
          .includes(query.toLowerCase()),
      ),
    [bootstrap.plugins, query],
  );
  const selectedSkill = bootstrap.skills.find(
    (skill) => skill.name === selectedId,
  );
  const selectedPlugin = bootstrap.plugins.find(
    (plugin) => plugin.manifest.id === selectedId,
  );

  const switchTab = (next: Tab) => {
    setTab(next);
    setQuery("");
    setNotice("");
    setError("");
    setSelectedId(
      next === "skills"
        ? (bootstrap.skills[0]?.name ?? "")
        : (bootstrap.plugins[0]?.manifest.id ?? ""),
    );
  };

  /** Every mutation reports both outcomes. Previously these calls had no catch,
   * so a failed toggle silently did nothing while the switch looked like it had
   * flipped. */
  const run = async (
    action: () => Promise<unknown>,
    success: string,
  ): Promise<void> => {
    setBusy(true);
    setNotice("");
    setError("");
    try {
      await action();
      await reload();
      setNotice(success);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const skillEnabled = selectedSkill?.enabled !== false;
  const pluginEnabled = Boolean(
    selectedPlugin &&
    (selectedPlugin.configuredEnabled ?? selectedPlugin.status === "enabled"),
  );

  return (
    <div className="page-sheet split-page">
      <aside className="picker">
        <div className="picker-head">
          <div className="segmented" role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={tab === "skills"}
              className={tab === "skills" ? "is-active" : ""}
              onClick={() => switchTab("skills")}
            >
              技能 <span>{bootstrap.skills.length}</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "plugins"}
              className={tab === "plugins" ? "is-active" : ""}
              onClick={() => switchTab("plugins")}
            >
              插件 <span>{bootstrap.plugins.length}</span>
            </button>
          </div>
        </div>

        {tab === "plugins" ? (
          <form
            className="install-form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(
                () => api.installPlugin(installPath),
                "已导入。打开它的开关即可使用。",
              ).then(() => setInstallPath(""));
            }}
          >
            <label>
              <span>从本地文件夹导入插件</span>
              <input
                placeholder="粘贴插件文件夹的完整路径"
                value={installPath}
                onChange={(event) => setInstallPath(event.target.value)}
              />
            </label>
            <button
              type="submit"
              className="runtime-action"
              disabled={!installPath.trim() || busy}
              title={installPath.trim() ? undefined : "先填写文件夹路径"}
            >
              导入
            </button>
          </form>
        ) : null}

        <label className="picker-search">
          <span className="sr-only">搜索</span>
          <input
            placeholder={tab === "skills" ? "搜索技能" : "搜索插件"}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>

        <div className="picker-list">
          {tab === "skills"
            ? skills.map((skill) => (
                <button
                  key={skill.name}
                  type="button"
                  className={`picker-item${selectedId === skill.name ? " is-active" : ""}`}
                  onClick={() => setSelectedId(skill.name)}
                >
                  <span>
                    <strong>{skill.name}</strong>
                    <small>{skill.description}</small>
                  </span>
                  {skill.enabled === false ? (
                    <span className="tag">已停用</span>
                  ) : null}
                </button>
              ))
            : plugins.map((plugin) => (
                <button
                  key={plugin.manifest.id}
                  type="button"
                  className={`picker-item${selectedId === plugin.manifest.id ? " is-active" : ""}`}
                  onClick={() => setSelectedId(plugin.manifest.id)}
                >
                  <span>
                    <strong>{plugin.manifest.name}</strong>
                    <small>版本 {plugin.manifest.version}</small>
                  </span>
                  {plugin.status === "failed" ? (
                    <span className="tag is-danger">
                      <AlertIcon className="tag-icon" />
                      加载失败
                    </span>
                  ) : null}
                </button>
              ))}
        </div>

        <button
          type="button"
          className="subtle-action"
          disabled={busy}
          onClick={() =>
            void run(() => api.reloadExtensions(), "已重新读取本地文件。")
          }
        >
          <RefreshIcon className="button-icon" />
          重新读取
        </button>
      </aside>

      <div className="detail-pane">
        {notice ? <p className="notice ok">{notice}</p> : null}
        {error ? (
          <div className="inline-error" role="alert">
            <strong>操作失败</strong>
            <span>{error}</span>
          </div>
        ) : null}

        {tab === "skills" && selectedSkill ? (
          <>
            <header className="detail-head">
              <div>
                <h2>{selectedSkill.name}</h2>
                <p>{selectedSkill.description}</p>
              </div>
              <div className="toggle-row">
                <span className="toggle-label">
                  {skillEnabled ? "已启用" : "已停用"}
                </span>
                <button
                  type="button"
                  className={`switch ${skillEnabled ? "on" : ""}`}
                  role="switch"
                  aria-checked={skillEnabled}
                  aria-label={skillEnabled ? "停用这个技能" : "启用这个技能"}
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () =>
                        api.setSkillEnabled(selectedSkill.name, !skillEnabled),
                      skillEnabled
                        ? "已停用。之后的对话不会再带上它。"
                        : "已启用。之后相关的对话会用到它。",
                    )
                  }
                >
                  <span />
                </button>
              </div>
            </header>

            <dl className="meta-list">
              <div>
                <dt>来自</dt>
                <dd>{selectedSkill.source}</dd>
              </div>
              <div>
                <dt>标签</dt>
                <dd>{selectedSkill.tags.join("、") || "无"}</dd>
              </div>
            </dl>

            <section className="page-block">
              <h3>什么时候会用到它</h3>
              <ul className="plain-list">
                <li>
                  你说的话和它的名称、说明、标签对得上时，Sleepy Doll
                  会自己想起来用它。
                </li>
                <li>
                  你也可以点名：在消息里写上 <code>${selectedSkill.name}</code>
                  ，就会强制用它。
                </li>
              </ul>
            </section>

            <section className="page-block">
              <h3>它给 Sleepy Doll 的说明</h3>
              <p className="muted">
                这段文字只在用到这个技能时才会发给模型，平时不占用对话。
              </p>
              <pre>
                {selectedSkill.instructions ?? "这个技能没有提供额外说明。"}
              </pre>
            </section>
          </>
        ) : tab === "plugins" && selectedPlugin ? (
          <>
            <header className="detail-head">
              <div>
                <h2>{selectedPlugin.manifest.name}</h2>
                <p>
                  {selectedPlugin.manifest.description ??
                    "这个插件没有写说明。"}
                </p>
              </div>
              <div className="toggle-row">
                <span className="toggle-label">
                  {pluginEnabled ? "已启用" : "已停用"}
                </span>
                <button
                  type="button"
                  className={`switch ${pluginEnabled ? "on" : ""}`}
                  role="switch"
                  aria-checked={pluginEnabled}
                  aria-label={pluginEnabled ? "停用这个插件" : "启用这个插件"}
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () =>
                        api.setPluginEnabled(
                          selectedPlugin.manifest.id,
                          !pluginEnabled,
                        ),
                      pluginEnabled
                        ? "已停用。它提供的能力会从列表里移除。"
                        : "已启用。它提供的能力现在可用了。",
                    )
                  }
                >
                  <span />
                </button>
              </div>
            </header>

            <dl className="meta-list">
              <div>
                <dt>状态</dt>
                <dd>
                  {pluginStatus[selectedPlugin.status] ?? selectedPlugin.status}
                </dd>
              </div>
              <div>
                <dt>版本</dt>
                <dd>{selectedPlugin.manifest.version}</dd>
              </div>
            </dl>

            {selectedPlugin.error ? (
              <div className="inline-error" role="alert">
                <strong>这个插件没能加载</strong>
                <span>{selectedPlugin.error}</span>
              </div>
            ) : null}

            <section className="page-block">
              <h3>它给 Sleepy Doll 增加了什么</h3>
              <ul className="plain-list">
                <li>可以调用的操作和工具。</li>
                <li>附带的操作说明（技能），它们各自还有独立的开关。</li>
              </ul>
              <p className="muted">
                只有打开上面的开关，这个插件才会真正运行。
              </p>
            </section>

            <footer className="detail-actions">
              <button
                type="button"
                className="runtime-action"
                disabled={pluginEnabled || busy}
                title={
                  pluginEnabled
                    ? "先停用它，才能从列表里移除"
                    : "把它移到回收文件夹，需要时可以再放回来"
                }
                onClick={() =>
                  void run(
                    () => api.removePlugin(selectedPlugin.manifest.id),
                    "已移到插件目录下的回收文件夹，需要时可以恢复。",
                  ).then(() => setSelectedId(""))
                }
              >
                从列表移除
              </button>
              {pluginEnabled ? (
                <p className="field-help">先停用它，才能从列表里移除。</p>
              ) : null}
            </footer>
          </>
        ) : (
          <p className="empty-note">
            {query
              ? `没有匹配「${query}」的${tab === "skills" ? "技能" : "插件"}。`
              : `还没有${tab === "skills" ? "技能" : "插件"}。`}
          </p>
        )}
      </div>
    </div>
  );
}
