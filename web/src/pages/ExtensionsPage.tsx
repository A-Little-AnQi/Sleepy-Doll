import { useMemo, useState } from "react";

import { api } from "../api";
import type { Bootstrap } from "../types";

type ExtensionTab = "skills" | "plugins";

function SkillArt() {
  return (
    <svg viewBox="0 0 44 44" aria-hidden="true">
      <path d="M4 8c7-2 12 0 18 5v25c-6-5-11-7-18-5ZM40 8c-7-2-12 0-18 5v25c6-5 11-7 18-5Z" />
      <path d="M22 13v25M9 15c3 0 6 1 9 3M35 15c-3 0-6 1-9 3" />
    </svg>
  );
}

function PluginArt() {
  return (
    <svg viewBox="0 0 44 44" aria-hidden="true">
      <path d="M7 8h10a5 5 0 0 1 10 0h10v10a5 5 0 0 1 0 10v9H27a5 5 0 0 1-10 0H7v-9a5 5 0 0 1 0-10Z" />
    </svg>
  );
}

function SourceArt() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true">
      <path d="M13 10H8a4 4 0 0 0-4 4v4a4 4 0 0 0 4 4h5M19 10h5a4 4 0 0 1 4 4v4a4 4 0 0 1-4 4h-5M10 16h12" />
    </svg>
  );
}

function TagArt() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true">
      <path d="M4 5h11l13 13-10 10L5 15Z" />
      <circle cx="10" cy="11" r="2" />
    </svg>
  );
}

interface ExtensionsPageProps {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}

export function ExtensionsPage({ bootstrap, reload }: ExtensionsPageProps) {
  const [tab, setTab] = useState<ExtensionTab>("skills");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(
    bootstrap.skills[0]?.name ?? bootstrap.plugins[0]?.manifest.id ?? "",
  );
  const [notice, setNotice] = useState("");
  const [installPath, setInstallPath] = useState("");

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

  const switchTab = (next: ExtensionTab) => {
    setTab(next);
    setQuery("");
    setSelectedId(
      next === "skills"
        ? (bootstrap.skills[0]?.name ?? "")
        : (bootstrap.plugins[0]?.manifest.id ?? ""),
    );
  };

  const setSkillEnabled = async (name: string, enabled: boolean) => {
    await api.setSkillEnabled(name, enabled);
    await reload();
    setNotice(
      enabled
        ? "Skill 已启用，新任务会加载它。"
        : "Skill 已停用，新任务不会加载它。",
    );
  };

  const setPluginEnabled = async (id: string, enabled: boolean) => {
    const result = await api.setPluginEnabled(id, enabled);
    await reload();
    setNotice(
      result.restartRequired
        ? "Plugin 配置已保存，重启 Sleepy Doll 后生效。"
        : "Plugin 状态已更新。",
    );
  };

  return (
    <section className="extensions-workspace">
      <aside className="extension-browser">
        <button
          className="runtime-action"
          onClick={() =>
            void api
              .reloadExtensions()
              .then(reload)
              .catch((e) => setNotice(String(e)))
          }
        >
          刷新能力库
        </button>
        <div className="segmented-control">
          <button
            className={tab === "skills" ? "active" : ""}
            onClick={() => switchTab("skills")}
          >
            Skills <span>{bootstrap.skills.length}</span>
          </button>
          <button
            className={tab === "plugins" ? "active" : ""}
            onClick={() => switchTab("plugins")}
          >
            Plugins <span>{bootstrap.plugins.length}</span>
          </button>
        </div>
        {tab === "plugins" ? (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void api
                .installPlugin(installPath)
                .then(async (result) => {
                  await reload();
                  setSelectedId(result.id);
                  setInstallPath("");
                  setNotice("插件已导入，可在此启用。");
                })
                .catch((e) => setNotice(String(e)));
            }}
          >
            <label className="extension-search">
              <input
                aria-label="本地插件文件夹"
                placeholder="本地插件文件夹路径"
                value={installPath}
                onChange={(e) => setInstallPath(e.target.value)}
              />
            </label>
            <button className="runtime-action" disabled={!installPath.trim()}>
              导入或更新插件
            </button>
          </form>
        ) : null}
        <label className="extension-search">
          <input
            aria-label="搜索扩展"
            placeholder="搜索名称、说明或标签"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <div className="extension-list">
          {tab === "skills"
            ? skills.map((skill) => (
                <button
                  key={skill.name}
                  className={selectedId === skill.name ? "active" : ""}
                  onClick={() => setSelectedId(skill.name)}
                >
                  <span className="list-icon">
                    <SkillArt />
                  </span>
                  <span>
                    <strong>{skill.name}</strong>
                    <small>{skill.description}</small>
                  </span>
                </button>
              ))
            : plugins.map((plugin) => (
                <button
                  key={plugin.manifest.id}
                  className={selectedId === plugin.manifest.id ? "active" : ""}
                  onClick={() => setSelectedId(plugin.manifest.id)}
                >
                  <span className="list-icon">
                    <PluginArt />
                  </span>
                  <span>
                    <strong>{plugin.manifest.name}</strong>
                    <small>v{plugin.manifest.version}</small>
                  </span>
                </button>
              ))}
        </div>
      </aside>

      <div className="extension-detail">
        {notice ? (
          <div className="notice extension-notice">{notice}</div>
        ) : null}
        {tab === "skills" && selectedSkill ? (
          <>
            <header className="detail-header">
              <span className="detail-icon">
                <SkillArt />
              </span>
              <div>
                <h2>{selectedSkill.name}</h2>
              </div>
              <button
                className={`switch ${selectedSkill.enabled === false ? "" : "on"}`}
                role="switch"
                aria-checked={selectedSkill.enabled !== false}
                aria-label={
                  selectedSkill.enabled !== false
                    ? "停用此 Skill"
                    : "启用此 Skill"
                }
                onClick={() =>
                  void setSkillEnabled(
                    selectedSkill.name,
                    selectedSkill.enabled === false,
                  )
                }
              >
                <span />
              </button>
            </header>
            <p className="detail-description">{selectedSkill.description}</p>
            <div className="detail-metadata">
              <div>
                <SourceArt />
                <span>来源</span>
                <strong>{selectedSkill.source}</strong>
              </div>
              <div>
                <TagArt />
                <span>标签</span>
                <strong>{selectedSkill.tags.join("、") || "无"}</strong>
              </div>
            </div>
            <section className="instruction-preview">
              <div>
                <SkillArt />
                <strong>指令预览</strong>
                <span>完整内容只在匹配任务时进入上下文</span>
              </div>
              <pre>
                {selectedSkill.instructions ??
                  "选择此 Skill 后可查看指令；内容不会全部注入每次对话。"}
              </pre>
            </section>
            <div className="behavior-settings">
              <h3>行为</h3>
              <div>
                <span>
                  <strong>自动匹配</strong>
                  <small>根据名称、说明和标签匹配用户请求</small>
                </span>
              </div>
              <div>
                <span>
                  <strong>显式调用</strong>
                  <small>用户可以使用 ${selectedSkill.name} 指定此 Skill</small>
                </span>
              </div>
            </div>
          </>
        ) : tab === "plugins" && selectedPlugin ? (
          <>
            <header className="detail-header">
              <span className="detail-icon">
                <PluginArt />
              </span>
              <div>
                <h2>{selectedPlugin.manifest.name}</h2>
              </div>
              <button
                className={`switch ${(selectedPlugin.configuredEnabled ?? selectedPlugin.status === "enabled") ? "on" : ""}`}
                role="switch"
                aria-label={
                  (selectedPlugin.configuredEnabled ??
                  selectedPlugin.status === "enabled")
                    ? "停用此插件"
                    : "启用此插件"
                }
                aria-checked={
                  selectedPlugin.configuredEnabled ??
                  selectedPlugin.status === "enabled"
                }
                onClick={() =>
                  void setPluginEnabled(
                    selectedPlugin.manifest.id,
                    !(
                      selectedPlugin.configuredEnabled ??
                      selectedPlugin.status === "enabled"
                    ),
                  )
                }
              >
                <span />
              </button>
            </header>
            <p className="detail-description">
              {selectedPlugin.manifest.description ?? "未提供说明"}
            </p>
            <div className="detail-metadata">
              <div>
                <SourceArt />
                <span>状态</span>
                <strong>{selectedPlugin.status}</strong>
              </div>
              <div>
                <TagArt />
                <span>版本</span>
                <strong>{selectedPlugin.manifest.version}</strong>
              </div>
            </div>
            <section className="plugin-capabilities">
              <h3>提供的能力</h3>
              <div>
                <PluginArt />
                <span>
                  <strong>工具与 MCP</strong>
                  <small>只在 Plugin 明确启用后注册和启动</small>
                </span>
              </div>
              <div>
                <SkillArt />
                <span>
                  <strong>附带 Skills</strong>
                  <small>与 Plugin 一起发现并遵守独立启用状态</small>
                </span>
              </div>
            </section>
            {selectedPlugin.error ? (
              <div className="inline-error">{selectedPlugin.error}</div>
            ) : null}
            <button
              className="runtime-action"
              disabled={
                selectedPlugin.configuredEnabled ??
                selectedPlugin.status === "enabled"
              }
              onClick={() =>
                void api
                  .removePlugin(selectedPlugin.manifest.id)
                  .then(async () => {
                    await reload();
                    setSelectedId("");
                    setNotice(
                      "插件已移入插件目录的 .retired 文件夹，可以恢复。",
                    );
                  })
                  .catch((e) => setNotice(String(e)))
              }
            >
              移出插件库
            </button>
          </>
        ) : (
          <div className="detail-empty">
            <PluginArt />
            <p>没有匹配的扩展</p>
          </div>
        )}
      </div>
    </section>
  );
}
