import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { Page } from "../App";
import { api } from "../api";
import { DEMO_TITLE } from "../demo-id";
import { demoConversationId, startDemo } from "../demo-session";
import {
  isRunning,
  needsConfirmation,
  readError,
  taskLabels,
} from "../session";
import type { Bootstrap, ConversationInfo } from "../types";
import {
  ArchiveIcon,
  BrandIcon,
  ChatIcon,
  EditIcon,
  PanelIcon,
  PinIcon,
  PluginIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  SidebarIcon,
  ToolIcon,
  TrashIcon,
} from "./icons";
import { Select } from "./Select";
import { Toast } from "./Toast";
import "./app-shell.css";

const NAV = [
  { page: "chat", label: "对话", Icon: ChatIcon },
  { page: "tasks", label: "快捷任务", Icon: ToolIcon },
  { page: "extensions", label: "工具与扩展", Icon: PluginIcon },
] as const;

const SETTINGS_PAGES: Page[] = ["settings", "models", "bridge"];

interface Props {
  bootstrap: Bootstrap;
  page: Page;
  conversationId?: string | undefined;
  detailsOpen: boolean;
  details?: ReactNode | undefined;
  children?: ReactNode | undefined;
  onPage(page: Page): void;
  onNew(): void;
  onConversation(id: string): void;
  onModel(id: string | null): Promise<void>;
  pendingModel?: string | undefined;
  onToggleDetails(): void;
  reload(): Promise<void>;
}

export function AppShell({
  bootstrap,
  page,
  conversationId,
  detailsOpen,
  details,
  children,
  onPage,
  onNew,
  onConversation,
  onModel,
  onToggleDetails,
  pendingModel,
  reload,
}: Props) {
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem("sleepy-doll-sidebar-collapsed") === "true",
  );
  const [query, setQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState("");
  const [wide, setWide] = useState(
    () => window.matchMedia("(min-width: 960px)").matches,
  );
  const [roomForDetails, setRoomForDetails] = useState(
    () => window.matchMedia("(min-width: 1180px)").matches,
  );

  useEffect(() => {
    localStorage.setItem("sleepy-doll-sidebar-collapsed", String(collapsed));
  }, [collapsed]);
  useEffect(() => {
    // 断点按 CSS 像素判定，窗口缩放改变的是 CSS 宽度。
    const sidebar = window.matchMedia("(min-width: 960px)");
    const details = window.matchMedia("(min-width: 1180px)");
    const sync = () => {
      setWide(sidebar.matches);
      setRoomForDetails(details.matches);
    };
    sync();
    sidebar.addEventListener("change", sync);
    details.addEventListener("change", sync);
    return () => {
      sidebar.removeEventListener("change", sync);
      details.removeEventListener("change", sync);
    };
  }, []);
  const settings = SETTINGS_PAGES.includes(page);
  const conversation = bootstrap.conversations.find(
    (entry) => entry.id === conversationId,
  );
  // 空字符串表示跟随默认模型。新对话还没有会话记录，就用用户这次挑的那一个，
  // 没挑过才回落到「跟随默认模型」。
  const conversationModel = conversationId
    ? (conversation?.modelId ?? "")
    : (pendingModel ?? "");
  const bridgeLabel = bootstrap.bridge.simulated
    ? "模拟连接"
    : bootstrap.bridge.enabled && bootstrap.bridge.connected
      ? "已连接"
      : "未连接";

  const conversations = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return bootstrap.conversations.filter(
      (entry) =>
        (showArchived || !entry.archived) &&
        (!needle || entry.title.toLowerCase().includes(needle)),
    );
  }, [bootstrap.conversations, query, showArchived]);

  // 窄窗口下侧栏默认收起，用覆盖抽屉打开；宽窗口下它是布局的一部分。
  const sidebarHidden = collapsed || !wide;
  // 详情栏讲的是「这个对话」和「这个任务」。设置、模型、工具页没有对应对象，
  // 在那里显示只会让人以为面板卡住了。
  const detailsAvailable = page === "chat" || page === "tasks";
  const conversationAct = async (action: () => Promise<unknown>) => {
    setError("");
    try {
      await action();
      await reload();
    } catch (reason) {
      setError(readError(reason));
    }
  };

  const aside = (
    <aside
      className="app-sidebar"
      aria-label="侧栏"
      aria-hidden={sidebarHidden}
      inert={sidebarHidden}
    >
      <div className="app-brand">
        <BrandIcon className="brand-mark" />
        <strong>Sleepy Doll</strong>
        <button
          className="icon-button"
          title={wide ? "收起侧栏" : "关闭侧栏"}
          aria-label={wide ? "收起侧栏" : "关闭侧栏"}
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
            {target === "chat" &&
              bootstrap.tasks.some((task) => isRunning(task)) && (
                <span className="app-nav-badge" title="有运行正在进行">
                  {bootstrap.tasks.filter((task) => isRunning(task)).length}
                </span>
              )}
          </button>
        ))}
      </nav>
      <div className="app-conversations">
        <div className="app-section-head">
          <span>最近对话</span>
          <button
            className="icon-button"
            aria-pressed={showArchived}
            title={showArchived ? "隐藏已归档" : "显示已归档"}
            aria-label={showArchived ? "隐藏已归档" : "显示已归档"}
            onClick={() => setShowArchived(!showArchived)}
          >
            <ArchiveIcon className="button-icon" />
          </button>
        </div>
        <label className="search-field is-compact">
          <SearchIcon />
          <input
            aria-label="搜索对话"
            placeholder="搜索对话"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <ul>
          {/* 开发专用：点击后由脚本自动演一遍真实会话，不连后端、不调模型。
              生产构建里 import.meta.env.DEV 为假，整段被摇掉。 */}
          {import.meta.env.DEV && (
            <li>
              <button
                title={DEMO_TITLE}
                className={
                  page === "chat" && conversationId === demoConversationId()
                    ? "is-current"
                    : undefined
                }
                aria-current={
                  page === "chat" && conversationId === demoConversationId()
                    ? "page"
                    : undefined
                }
                onClick={() => {
                  startDemo();
                  onConversation(demoConversationId());
                }}
              >
                <span>{DEMO_TITLE}</span>
              </button>
            </li>
          )}
          {conversations.map((entry) => (
            <ConversationRow
              key={entry.id}
              entry={entry}
              current={page === "chat" && conversationId === entry.id}
              running={bootstrap.tasks.find(
                (task) => task.conversationId === entry.id && isRunning(task),
              )}
              onOpen={() => onConversation(entry.id)}
              onAct={conversationAct}
              permissionMode={bootstrap.permission.mode}
            />
          ))}
          {!conversations.length && (
            <li className="app-conversation-empty">
              {query ? "没有找到匹配的对话" : "还没有对话"}
            </li>
          )}
        </ul>
      </div>
      <div className="app-sidebar-foot">
        <button className="app-connection" onClick={() => onPage("bridge")}>
          <PluginIcon className="app-nav-icon" />
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
  );

  return (
    <div
      className="app-shell"
      data-collapsed={sidebarHidden}
      data-drawer={!wide && !collapsed}
      data-details={detailsOpen && roomForDetails}
    >
      {aside}
      {!wide && !collapsed && (
        <button
          className="app-scrim"
          aria-label="关闭侧栏"
          onClick={() => setCollapsed(true)}
        />
      )}
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
                : page === "chat"
                  ? (conversation?.title ?? "新对话")
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
          <div className="app-header-actions">
            {page === "chat" && (
              <div className="app-model">
                <Select
                  value={conversationModel}
                  label="此对话使用的模型"
                  // 新对话也要能先挑好模型：把选择攒到发出第一条消息时一起生效。
                  disabled={switching || !bootstrap.models.length}
                  options={[
                    { value: "", label: "跟随默认模型" },
                    ...bootstrap.models.map((model) => ({
                      value: model.id,
                      label: model.name,
                      description: model.model,
                    })),
                  ]}
                  onChange={(id) => {
                    setSwitching(true);
                    setError("");
                    void onModel(id || null)
                      .catch((reason) => setError(readError(reason)))
                      .finally(() => setSwitching(false));
                  }}
                />
              </div>
            )}
            {detailsAvailable && (
              <button
                className="icon-button"
                aria-pressed={detailsOpen}
                aria-label={detailsOpen ? "隐藏详情" : "显示详情"}
                title={detailsOpen ? "隐藏详情" : "显示详情"}
                onClick={onToggleDetails}
              >
                <PanelIcon className="button-icon" />
              </button>
            )}
          </div>
        </header>
        {error && <Toast message={error} onDismiss={() => setError("")} />}
        <div className="app-body">
          <div
            className="app-view"
            data-motion="page"
            key={settings ? "settings" : page}
          >
            {children}
          </div>
          {detailsAvailable &&
            detailsOpen &&
            (roomForDetails ? (
              details
            ) : (
              <>
                <button
                  className="details-scrim"
                  aria-label="关闭详情"
                  onClick={onToggleDetails}
                />
                <div className="details-drawer">{details}</div>
              </>
            ))}
        </div>
      </main>
    </div>
  );
}

function ConversationRow({
  entry,
  current,
  running,
  permissionMode,
  onOpen,
  onAct,
}: {
  entry: ConversationInfo;
  current: boolean;
  running?: { state: string } | undefined;
  permissionMode: string;
  onOpen(): void;
  onAct(action: () => Promise<unknown>): void;
}) {
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(entry.title);
  useEffect(() => setTitle(entry.title), [entry.title]);
  if (editing) {
    return (
      <li className={current ? "is-current" : undefined}>
        <form
          className="app-conversation-rename"
          onSubmit={(event) => {
            event.preventDefault();
            setEditing(false);
            void onAct(() => api.renameConversation(entry.id, title));
          }}
        >
          <input
            aria-label="会话名称"
            value={title}
            autoFocus
            onChange={(event) => setTitle(event.target.value)}
            onBlur={() => setEditing(false)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setEditing(false);
            }}
          />
        </form>
      </li>
    );
  }
  return (
    <li className={current ? "is-current" : undefined}>
      <button
        title={entry.title}
        aria-current={current ? "page" : undefined}
        onClick={onOpen}
      >
        {entry.pinned && <PinIcon className="app-conversation-pin" />}
        <span className="app-conversation-title">{entry.title}</span>
        {entry.archived && <em className="app-conversation-flag">已归档</em>}
        {running && (
          <span
            className="conversation-status running"
            title={taskLabels[running.state] ?? "运行中"}
            aria-label={taskLabels[running.state] ?? "运行中"}
          />
        )}
      </button>
      <div className="app-conversation-actions">
        <button
          className="icon-button"
          aria-label={entry.pinned ? "取消置顶" : "置顶"}
          title={entry.pinned ? "取消置顶" : "置顶"}
          onClick={() =>
            void onAct(() => api.pinConversation(entry.id, !entry.pinned))
          }
        >
          <PinIcon className="button-icon" />
        </button>
        <button
          className="icon-button"
          aria-label="重命名"
          title="重命名"
          onClick={() => setEditing(true)}
        >
          <EditIcon className="button-icon" />
        </button>
        <button
          className="icon-button"
          aria-label={entry.archived ? "取消归档" : "归档"}
          title={entry.archived ? "取消归档" : "归档"}
          onClick={() =>
            void onAct(() => api.archiveConversation(entry.id, !entry.archived))
          }
        >
          <ArchiveIcon className="button-icon" />
        </button>
        <button
          className="icon-button"
          aria-label="删除对话"
          title="删除对话"
          onClick={() => {
            void (async () => {
              const first = await api.deleteConversation(entry.id, false);
              if (!first.requiresConfirmation) return;
              const kept = first.keeps ?? "";
              const tasks = first.affects?.taskCount ?? 0;
              // 「完全控制」下不再拦第二遍，后端已按同一级别放行。
              const confirmed =
                !needsConfirmation(permissionMode) ||
                window.confirm(
                  `删除对话「${first.affects?.title ?? entry.title}」？\n` +
                    `会删除其中的消息和运行记录${
                      tasks ? `，其中 ${tasks} 个快捷任务会保留` : ""
                    }。\n${kept}`,
                );
              if (confirmed) await api.deleteConversation(entry.id, true);
              await onAct(async () => {});
            })().catch(() => undefined);
          }}
        >
          <TrashIcon className="button-icon" />
        </button>
      </div>
    </li>
  );
}
