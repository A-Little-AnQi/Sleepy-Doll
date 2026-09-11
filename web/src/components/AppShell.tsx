import { useEffect, useState, type ReactNode } from "react";

import type { Page } from "../App";
import type { Bootstrap } from "../types";
import {
  BridgeIcon,
  ChatIcon,
  ChevronIcon,
  HistoryIcon,
  ModelIcon,
  PluginIcon,
  PlusIcon,
  SettingsIcon,
} from "./icons";
import "./app-shell.css";

/** Sidebar entries. Every one carries a visible label — the previous shell drew
 * five unlabelled glyphs onto an illustration, which a new user cannot decode. */
const NAV = [
  { page: "chat", label: "对话", Icon: ChatIcon },
  { page: "library", label: "运行记录", Icon: HistoryIcon },
  { page: "bridge", label: "BetterGI 连接", Icon: BridgeIcon },
  { page: "models", label: "模型", Icon: ModelIcon },
  { page: "extensions", label: "技能与插件", Icon: PluginIcon },
  { page: "settings", label: "设置", Icon: SettingsIcon },
] as const satisfies readonly { page: Page; label: string; Icon: unknown }[];

/** One factual line per page, so a first-time user knows what they are looking
 * at. Deliberately short: this explains, it does not decorate. */
const SUBTITLE: Record<Page, string> = {
  chat: "说出你想让 Sleepy Doll 做的事。需要操作游戏时，它会先征求你的同意。",
  library: "已经验证成功过的流程可以在这里直接重跑，不用再问一次模型。",
  bridge:
    "Sleepy Doll 通过 BetterGI 读取游戏画面、执行操作。没连上时下面的按钮不可用。",
  models: "选择由哪一个 AI 服务来理解你的指令，以及它怎么连接。",
  extensions: "技能是给 AI 的操作说明；插件为它增加新的能力。",
  settings: "界面偏好。",
};

const COLLAPSE_KEY = "sleepy-doll-sidebar-collapsed";

interface ShellProps {
  bootstrap: Bootstrap;
  page: Page;
  children?: ReactNode;
  onPage(page: Page): void;
  onNew(): void;
  onConversation(id: string): void;
  onModel(id: string): Promise<void>;
}

export function AppShell({
  bootstrap,
  page,
  children,
  onPage,
  onNew,
  onConversation,
  onModel,
}: ShellProps) {
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(COLLAPSE_KEY) === "true",
  );
  const [switching, setSwitching] = useState(false);
  const [modelError, setModelError] = useState("");

  useEffect(() => {
    localStorage.setItem(COLLAPSE_KEY, String(collapsed));
  }, [collapsed]);

  const active = bootstrap.models.find((model) => model.active);
  const bridge = bootstrap.bridge;
  const bridgeLabel = !bridge.enabled
    ? "未启用"
    : bridge.connected
      ? "已连接"
      : "未连接";
  const bridgeTone = !bridge.enabled
    ? "is-off"
    : bridge.connected
      ? "is-ok"
      : "is-bad";

  const switchModel = async (id: string) => {
    setSwitching(true);
    setModelError("");
    try {
      await onModel(id);
    } catch (reason) {
      setModelError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSwitching(false);
    }
  };

  return (
    <div className="app-shell" data-collapsed={collapsed}>
      <aside className="app-sidebar">
        <div className="app-brand">
          <span className="app-brand-art" aria-hidden="true" />
          <span className="app-brand-text">
            <strong>Sleepy Doll</strong>
            <small>BetterGI 助手</small>
          </span>
        </div>

        <nav className="app-nav" aria-label="主导航">
          {NAV.map(({ page: target, label, Icon }) => (
            <button
              key={target}
              type="button"
              className={`app-nav-item${page === target ? " is-active" : ""}`}
              aria-current={page === target ? "page" : undefined}
              title={collapsed ? label : undefined}
              onClick={() => onPage(target)}
            >
              <Icon className="app-nav-icon" />
              <span>{label}</span>
            </button>
          ))}
        </nav>

        <div className="app-conversations">
          <div className="app-section-head">
            <span>最近对话</span>
            <button type="button" className="app-new" onClick={onNew}>
              <PlusIcon className="app-new-icon" />
              <span>新建</span>
            </button>
          </div>
          {bootstrap.conversations.length === 0 ? (
            <p className="app-conversations-empty">还没有对话。</p>
          ) : (
            <ul>
              {bootstrap.conversations.slice(0, 8).map((conversation) => (
                <li key={conversation.id}>
                  <button
                    type="button"
                    title={conversation.title}
                    onClick={() => onConversation(conversation.id)}
                  >
                    {conversation.title}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="app-sidebar-foot">
          <button
            type="button"
            className="app-collapse"
            aria-label={collapsed ? "展开侧栏" : "折叠侧栏"}
            aria-expanded={!collapsed}
            onClick={() => setCollapsed((value) => !value)}
          >
            <ChevronIcon className="app-collapse-icon" />
            <span>{collapsed ? "展开侧栏" : "折叠侧栏"}</span>
          </button>
        </div>
      </aside>

      <main className="app-main">
        <header className="app-header">
          <div className="app-header-title">
            <h1>{NAV.find((item) => item.page === page)?.label}</h1>
            <p>{SUBTITLE[page]}</p>
          </div>

          <div className="app-header-status">
            <label className="app-model">
              <span>当前模型</span>
              <select
                value={active?.id ?? ""}
                disabled={switching}
                onChange={(event) => void switchModel(event.target.value)}
              >
                {bootstrap.models.map((model) => (
                  <option key={model.id} value={model.id}>
                    {model.name}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className={`app-bridge ${bridgeTone}`}
              onClick={() => onPage("bridge")}
              title="查看 BetterGI 连接"
            >
              <BridgeIcon className="app-bridge-icon" />
              <span>BetterGI {bridgeLabel}</span>
            </button>
          </div>
        </header>

        {modelError && (
          <p className="app-banner" role="alert">
            切换模型失败：{modelError}
          </p>
        )}

        <div className="app-body">{children}</div>
      </main>
    </div>
  );
}
