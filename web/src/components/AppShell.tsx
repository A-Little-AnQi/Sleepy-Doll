import { useEffect, useState, type ReactNode } from "react";
import type { Page } from "../App";
import type { Bootstrap } from "../types";
import {
  BrandIcon,
  BridgeIcon,
  ChatIcon,
  HistoryIcon,
  PluginIcon,
  PlusIcon,
  SettingsIcon,
  SidebarIcon,
} from "./icons";
import { Select } from "./Select";
import { isRunning, taskLabels } from "../session";
import "./app-shell.css";

const NAV = [
  { page: "chat", label: "对话", Icon: ChatIcon },
  { page: "library", label: "任务", Icon: HistoryIcon },
  { page: "extensions", label: "技能与插件", Icon: PluginIcon },
] as const;
interface Props {
  bootstrap: Bootstrap;
  page: Page;
  conversationId?: string | undefined;
  children?: ReactNode;
  onPage(page: Page): void;
  onNew(): void;
  onConversation(id: string): void;
  onModel(id: string): Promise<void>;
}
export function AppShell({
  bootstrap,
  page,
  conversationId,
  children,
  onPage,
  onNew,
  onConversation,
  onModel,
}: Props) {
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem("sleepy-doll-sidebar-collapsed") === "true",
  );
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    localStorage.setItem("sleepy-doll-sidebar-collapsed", String(collapsed));
  }, [collapsed]);
  const settings = ["settings", "models", "bridge"].includes(page);
  const active = bootstrap.models.find((model) => model.active);
  const bridgeLabel = bootstrap.bridge.simulated
    ? "模拟连接"
    : bootstrap.bridge.enabled && bootstrap.bridge.connected
      ? "已连接"
      : "未连接";
  return (
    <div className="app-shell" data-collapsed={collapsed}>
      <aside
        className="app-sidebar"
        aria-label="侧栏"
        aria-hidden={collapsed}
        inert={collapsed}
      >
        <div className="app-brand">
          <BrandIcon className="brand-mark" />
          <strong>Sleepy Doll</strong>
          <button
            className="icon-button"
            title="收起侧栏"
            aria-label="收起侧栏"
            onClick={() => setCollapsed(true)}
          >
            <SidebarIcon className="button-icon" />
          </button>
        </div>
        <button className="app-new" onClick={onNew}>
          <PlusIcon className="button-icon" />
          <span>新建对话</span>
        </button>
        <nav className="app-nav" aria-label="主导航">
          {NAV.map(({ page: target, label, Icon }) => (
            <button
              key={target}
              className={`app-nav-item${page === target ? " is-active" : ""}`}
              aria-current={page === target ? "page" : undefined}
              onClick={() => onPage(target)}
            >
              <Icon className="app-nav-icon" />
              <span>{label}</span>
            </button>
          ))}
        </nav>
        <div className="app-conversations">
          <div className="app-section-head">最近对话</div>
          <ul>
            {bootstrap.conversations.slice(0, 20).map((conversation) => (
              <li key={conversation.id}>
                <button
                  title={conversation.title}
                  className={
                    page === "chat" && conversationId === conversation.id
                      ? "is-current"
                      : undefined
                  }
                  aria-current={
                    page === "chat" && conversationId === conversation.id
                      ? "page"
                      : undefined
                  }
                  onClick={() => onConversation(conversation.id)}
                >
                  <span>{conversation.title}</span>
                  {bootstrap.tasks.some(
                    (task) =>
                      task.conversationId === conversation.id &&
                      isRunning(task),
                  ) && (
                    <span
                      className="conversation-status running"
                      aria-label={
                        taskLabels[
                          bootstrap.tasks.find(
                            (task) =>
                              task.conversationId === conversation.id &&
                              isRunning(task),
                          )?.state ?? ""
                        ] ?? "运行中"
                      }
                    />
                  )}
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div className="app-sidebar-foot">
          <button className="app-connection" onClick={() => onPage("bridge")}>
            <BridgeIcon className="app-nav-icon" />
            <span>BetterGI</span>
            <small>{bridgeLabel}</small>
          </button>
          <button
            className={`app-nav-item${settings ? " is-active" : ""}`}
            aria-current={settings ? "page" : undefined}
            onClick={() => onPage("settings")}
          >
            <SettingsIcon className="app-nav-icon" />
            <span>设置</span>
          </button>
        </div>
      </aside>
      <main className="app-main">
        <header className="app-header">
          <div className="app-header-title">
            {collapsed && (
              <>
                <button
                  className="icon-button"
                  title="展开侧栏"
                  aria-label="展开侧栏"
                  onClick={() => setCollapsed(false)}
                >
                  <SidebarIcon className="button-icon" />
                </button>
                <button
                  className="icon-button"
                  title="新建对话"
                  aria-label="新建对话"
                  onClick={onNew}
                >
                  <PlusIcon className="button-icon" />
                </button>
              </>
            )}
            <h1>
              {settings
                ? "设置"
                : NAV.find((item) => item.page === page)?.label}
            </h1>
            {bootstrap.preview && (
              <span
                className="tag"
                title="连接的是本地测试后端，不代表真实游戏状态"
              >
                模拟预览
              </span>
            )}
          </div>
          {page === "chat" && (
            <div className="app-model">
              <Select
                value={active?.id ?? ""}
                label="当前模型"
                disabled={switching}
                options={bootstrap.models.map((model) => ({
                  value: model.id,
                  label: model.name,
                  description: model.model,
                }))}
                onChange={(id) => {
                  setSwitching(true);
                  setError("");
                  void onModel(id)
                    .catch((reason) => setError(String(reason)))
                    .finally(() => setSwitching(false));
                }}
              />
            </div>
          )}
        </header>
        {error && (
          <p className="app-banner" role="alert">
            {error}
          </p>
        )}
        <div className="app-body">
          <div
            className="app-view"
            data-motion="page"
            key={settings ? "settings" : page}
          >
            {children}
          </div>
        </div>
      </main>
    </div>
  );
}
