import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent,
  type ReactNode,
  type CSSProperties,
} from "react";
import { createPortal, flushSync } from "react-dom";
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
  BrandIcon,
  ChatIcon,
  EditIcon,
  FolderIcon,
  PanelIcon,
  PluginIcon,
  PlusIcon,
  SearchIcon,
  SidebarIcon,
  ToolIcon,
  TrashIcon,
} from "./icons";
import { SidebarAccount } from "./SidebarAccount";
import { InlineRename } from "./InlineRename";
import { DisclosureChevron } from "./DisclosureChevron";
import { Toast } from "./Toast";
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTH_KEY,
  clampPreferredSidebarWidth,
  clampSidebarWidth,
} from "./sidebar-layout";
import {
  createGroup,
  deleteGroup,
  layoutAfterDrop,
  readLayout,
  renameGroup,
  setCollapsed as setGroupCollapsed,
  splitConversations,
  orderConversations,
  writeLayout,
  type DropSlot,
  type GroupLayout,
} from "../conversation-groups";
import "./app-shell.css";

const NAV = [
  { page: "chat", label: "对话", Icon: ChatIcon },
  { page: "tasks", label: "快捷任务", Icon: ToolIcon },
  { page: "extensions", label: "工具与扩展", Icon: PluginIcon },
] as const;

const SETTINGS_PAGES: Page[] = ["settings", "models", "bridge", "sponsor"];

const DRAG_THRESHOLD = 6;

type DragItem = { kind: "group" | "conversation"; id: string; label: string };

type DragSession = DragItem & {
  width: number;
  height: number;
  grabX: number;
  grabY: number;
};

function slotFromPoint(
  x: number,
  y: number,
  kind: DragItem["kind"],
  sourceId: string,
): DropSlot | null {
  const slots = document
    .elementsFromPoint(x, y)
    .filter(
      (node): node is HTMLElement =>
        node instanceof HTMLElement && Boolean(node.dataset.sdSlot),
    );
  const conversation = slots.find(
    (node) =>
      node.dataset.sdSlot === "conversation" && node.dataset.sdId !== sourceId,
  );
  if (conversation?.dataset.sdId) {
    const rect = conversation.getBoundingClientRect();
    return {
      target: "conversation",
      id: conversation.dataset.sdId,
      groupId: conversation.dataset.sdGroup || null,
      edge: y < rect.top + rect.height / 2 ? "before" : "after",
    };
  }
  const group = slots.find(
    (node) => node.dataset.sdSlot === "group" && node.dataset.sdId !== sourceId,
  );
  if (group?.dataset.sdId) {
    const rect = group.getBoundingClientRect();
    if (kind === "conversation") {
      return { target: "group", id: group.dataset.sdId, edge: "into" };
    }
    return {
      target: "group",
      id: group.dataset.sdId,
      edge: y < rect.top + rect.height / 2 ? "before" : "after",
    };
  }
  if (slots.some((node) => node.dataset.sdSlot === "ungrouped")) {
    return { target: "ungrouped" };
  }
  return null;
}

function vtName(id: string) {
  return `sd-${id.replace(/[^a-zA-Z0-9_-]/g, "")}`;
}

function idsKey(items: Array<{ id: string }>) {
  return items.map((item) => item.id).join("\0");
}

function runLayout(apply: () => void) {
  if (typeof document.startViewTransition !== "function") {
    apply();
    return;
  }
  document.startViewTransition(apply);
}

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
  onToggleDetails,
  reload,
}: Props) {
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem("sleepy-doll-sidebar-collapsed") === "true",
  );
  const [preferredWidth, setPreferredWidth] = useState(() =>
    clampPreferredSidebarWidth(
      Number(localStorage.getItem(SIDEBAR_WIDTH_KEY)) || SIDEBAR_DEFAULT_WIDTH,
    ),
  );
  const [viewport, setViewport] = useState(() => window.innerWidth);
  const sidebarWidth = clampSidebarWidth(preferredWidth, viewport);
  const [peek, setPeek] = useState(false);
  const [resizing, setResizing] = useState(false);
  const widthRef = useRef(preferredWidth);
  const resizingRef = useRef(false);
  const peekTimer = useRef(0);
  const [query, setQuery] = useState("");
  const [layout, setLayout] = useState<GroupLayout>(() => {
    const local = readLayout();
    const remote = bootstrap.conversationGroups;
    if (!remote) return local;
    if (!remote.groups.length && local.groups.length) return local;
    return {
      ...remote,
      order: remote.order?.length ? remote.order : local.order,
    };
  });
  const layoutRef = useRef(layout);
  layoutRef.current = layout;
  const draggingRef = useRef(false);
  const draftRef = useRef<GroupLayout | null>(null);
  const [draft, setDraft] = useState<GroupLayout | null>(null);
  const [ghost, setGhost] = useState<
    (DragSession & { x: number; y: number }) | null
  >(null);
  const runningConversations = useRef<Set<string>>(new Set());
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
    widthRef.current = preferredWidth;
  }, [preferredWidth]);
  useEffect(() => {
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  useEffect(() => () => window.clearTimeout(peekTimer.current), []);
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
  const bridgeLabel = bootstrap.bridge.simulated
    ? "模拟连接"
    : bootstrap.bridge.enabled && bootstrap.bridge.connected
      ? "已连接"
      : "未连接";

  const persistLayout = (next: GroupLayout) => {
    setLayout(next);
    writeLayout(next);
    if (bootstrap.conversationGroups === undefined) return;
    void api.saveConversationGroups(next).catch(() => undefined);
  };

  const shownLayout = draft ?? layout;
  const orderedConversations = useMemo(() => {
    const runningIds = new Set(
      bootstrap.tasks
        .filter((task) => isRunning(task))
        .map((task) => task.conversationId),
    );
    const ordered = orderConversations(
      bootstrap.conversations,
      layout.order,
      runningIds,
      runningConversations.current,
    );
    runningConversations.current = runningIds;
    return ordered;
  }, [bootstrap.conversations, bootstrap.tasks, layout.order]);
  useEffect(() => {
    if (ghost) return;
    const ids = orderedConversations.map((entry) => entry.id);
    if (ids.join("\0") === layout.order.join("\0")) return;
    persistLayout({ ...layout, order: ids });
  }, [ghost, layout, orderedConversations]);
  const [shownConversations, setShownConversations] =
    useState(orderedConversations);
  useLayoutEffect(() => {
    if (idsKey(orderedConversations) === idsKey(shownConversations)) {
      setShownConversations(orderedConversations);
      return;
    }
    if (!shownConversations.length || ghost || draggingRef.current) {
      setShownConversations(orderedConversations);
      return;
    }
    runLayout(() => {
      flushSync(() => setShownConversations(orderedConversations));
    });
  }, [ghost, orderedConversations, shownConversations]);
  const conversations = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return shownConversations.filter(
      (entry) => !needle || entry.title.toLowerCase().includes(needle),
    );
  }, [shownConversations, query]);
  const grouped = useMemo(
    () => splitConversations(conversations, shownLayout),
    [conversations, shownLayout],
  );

  const armDrag = (event: PointerEvent<HTMLElement>, item: DragItem) => {
    if (event.button !== 0) return;
    if (
      event.target instanceof Element &&
      event.target.closest(
        ".app-conversation-actions, .sd-inline-rename, input",
      )
    ) {
      return;
    }
    const origin = event.currentTarget.getBoundingClientRect();
    const session: DragSession = {
      ...item,
      width: origin.width,
      height: origin.height,
      grabX: event.clientX - origin.left,
      grabY: event.clientY - origin.top,
    };
    const startX = event.clientX;
    const startY = event.clientY;
    let active = false;
    const preview = (clientX: number, clientY: number) => {
      const slot = slotFromPoint(clientX, clientY, session.kind, session.id);
      const next = slot
        ? layoutAfterDrop(layoutRef.current, session, slot)
        : (draftRef.current ?? layoutRef.current);
      draftRef.current = next;
      setDraft(next);
      setGhost({ ...session, x: clientX, y: clientY });
    };
    const move = (next: globalThis.PointerEvent) => {
      const dx = next.clientX - startX;
      const dy = next.clientY - startY;
      if (!active && dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD) return;
      if (!active) {
        active = true;
        draggingRef.current = true;
        document.documentElement.dataset.sdDragging = session.kind;
        try {
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          /* jsdom 和部分嵌入预览没有 capture。 */
        }
      }
      next.preventDefault();
      preview(next.clientX, next.clientY);
    };
    const stop = (next: globalThis.PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      delete document.documentElement.dataset.sdDragging;
      if (!active) {
        draggingRef.current = false;
        return;
      }
      next.preventDefault();
      const dropped = draftRef.current ?? layoutRef.current;
      draftRef.current = null;
      draggingRef.current = false;
      setDraft(null);
      setGhost(null);
      persistLayout(dropped);
      const block = (click: MouseEvent) => {
        click.preventDefault();
        click.stopPropagation();
      };
      window.addEventListener("click", block, { capture: true, once: true });
      window.setTimeout(() => {
        window.removeEventListener("click", block, { capture: true });
      }, 50);
    };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };

  // 宽窗口未折叠时侧栏占布局列；否则不占列，靠左缘悬停唤出悬浮抽屉。
  const docked = wide && !collapsed;
  const overlayOpen = !docked && (peek || !collapsed);
  const sidebarVisible = docked || overlayOpen;
  const showPeek = () => {
    window.clearTimeout(peekTimer.current);
    setPeek(true);
  };
  const hidePeek = (event?: { relatedTarget: EventTarget | null }) => {
    if (resizingRef.current || draggingRef.current) return;
    // 指针移出浏览器窗口时 relatedTarget 为空。这时收起，回窗会撞上左缘热区再播一遍入场。
    if (event && event.relatedTarget == null) return;
    if (
      event?.relatedTarget instanceof Element &&
      (event.relatedTarget.closest(".app-sidebar") ||
        event.relatedTarget.closest(".app-sidebar-peek"))
    ) {
      return;
    }
    window.clearTimeout(peekTimer.current);
    peekTimer.current = window.setTimeout(() => setPeek(false), 220);
  };
  useEffect(() => {
    if (docked || !peek) return;
    const onMove = (event: globalThis.PointerEvent) => {
      if (resizingRef.current || draggingRef.current) return;
      const x = event.clientX;
      const y = event.clientY;
      if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) {
        return;
      }
      if (x > sidebarWidth) {
        window.clearTimeout(peekTimer.current);
        setPeek(false);
      }
    };
    window.addEventListener("pointermove", onMove);
    return () => window.removeEventListener("pointermove", onMove);
  }, [docked, peek, sidebarWidth]);
  const pinSidebar = () => {
    window.clearTimeout(peekTimer.current);
    setPeek(false);
    setCollapsed(false);
  };
  const foldSidebar = () => {
    window.clearTimeout(peekTimer.current);
    setPeek(false);
    setCollapsed(true);
  };
  const onResizePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const origin = event.clientX;
    const start = widthRef.current;
    const handle = event.currentTarget;
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      /* jsdom 和部分嵌入预览没有 capture，改听 window。 */
    }
    resizingRef.current = true;
    setResizing(true);
    document.documentElement.dataset.sidebarResizing = "true";
    const move = (next: globalThis.PointerEvent) => {
      const width = clampPreferredSidebarWidth(start + next.clientX - origin);
      widthRef.current = width;
      setPreferredWidth(width);
    };
    const stop = () => {
      resizingRef.current = false;
      setResizing(false);
      delete document.documentElement.dataset.sidebarResizing;
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(widthRef.current));
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };
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
      aria-hidden={!sidebarVisible}
      inert={!sidebarVisible}
      onPointerEnter={!docked ? showPeek : undefined}
      onPointerLeave={!docked ? hidePeek : undefined}
    >
      <div className="app-brand">
        <BrandIcon className="brand-mark" />
        <strong>Sleepy Doll</strong>
        <button
          className="icon-button"
          title={wide ? "收起侧栏" : "关闭侧栏"}
          aria-label={wide ? "收起侧栏" : "关闭侧栏"}
          onClick={foldSidebar}
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
      <div className="app-sidebar-tools">
        <div className="app-section-head">
          <span>最近对话</span>
          <button
            className="icon-button"
            title="新建分组"
            aria-label="新建分组"
            onClick={() => persistLayout(createGroup(layout))}
          >
            <FolderIcon className="button-icon" />
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
      </div>
      <div className="app-conversations">
        <ul
          data-sd-list
          onPointerDown={(event) => {
            if (!ghost) return;
            event.preventDefault();
          }}
        >
          {/* 开发专用：点击后由脚本自动演一遍真实会话，不连后端、不调模型。
              生产构建里 import.meta.env.DEV 为假，整段被摇掉。 */}
          {import.meta.env.DEV && (
            <li
              className={
                page === "chat" && conversationId === demoConversationId()
                  ? "is-current"
                  : undefined
              }
            >
              <button
                title={DEMO_TITLE}
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
                <span className="app-conversation-title">{DEMO_TITLE}</span>
              </button>
            </li>
          )}
          {grouped.groups.map(({ group, items }) => (
            <GroupRow
              key={group.id}
              group={group}
              items={items}
              page={page}
              conversationId={conversationId}
              bootstrap={bootstrap}
              layout={shownLayout}
              persistLayout={persistLayout}
              draggingId={ghost?.id}
              onDragArm={armDrag}
              onConversation={onConversation}
              onAct={conversationAct}
            />
          ))}
          {grouped.ungrouped.map((entry) => (
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
              draggingId={ghost?.id}
              onDragArm={armDrag}
            />
          ))}
          {ghost?.kind === "conversation" && (
            <li
              data-sd-slot="ungrouped"
              className="app-drop-end"
              aria-hidden="true"
            />
          )}
          {!conversations.length && (
            <li className="app-conversation-empty">
              {query ? "没有找到匹配的对话" : "还没有对话"}
            </li>
          )}
        </ul>
      </div>
      <div className="app-sidebar-status">
        <button className="app-connection" onClick={() => onPage("bridge")}>
          <PluginIcon className="app-nav-icon" />
          <span>BetterGI</span>
          <small>{bridgeLabel}</small>
        </button>
      </div>
      <div className="app-sidebar-foot">
        <SidebarAccount onSettings={() => onPage("settings")} />
      </div>
      <div
        className="app-sidebar-resize"
        role="separator"
        aria-orientation="vertical"
        aria-label="调整侧栏宽度"
        aria-valuenow={sidebarWidth}
        aria-valuemin={SIDEBAR_MIN_WIDTH}
        aria-valuemax={SIDEBAR_MAX_WIDTH}
        onPointerDown={onResizePointerDown}
      />
    </aside>
  );

  return (
    <div
      className="app-shell"
      data-collapsed={!docked}
      data-drawer={overlayOpen}
      data-resizing={resizing}
      data-details={detailsOpen && roomForDetails}
      style={
        {
          "--sidebar-user-width": `${sidebarWidth}px`,
          "--sidebar-width": `${docked ? sidebarWidth : 0}px`,
        } as CSSProperties
      }
    >
      {!docked && (
        <div
          className="app-sidebar-peek"
          onPointerEnter={showPeek}
          onPointerDown={showPeek}
          onPointerLeave={hidePeek}
        />
      )}
      {aside}
      {!wide && !collapsed && (
        <button
          className="app-scrim"
          aria-label="关闭侧栏"
          onClick={foldSidebar}
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
                  onClick={pinSidebar}
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
          <div className="app-view">{children}</div>
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
      {ghost
        ? createPortal(
            <div
              className="app-drag-ghost"
              style={{
                left: ghost.x - ghost.grabX,
                top: ghost.y - ghost.grabY,
                width: ghost.width,
                height: ghost.height,
              }}
            >
              {ghost.label}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}

function GroupRow({
  group,
  items,
  page,
  conversationId,
  bootstrap,
  layout,
  persistLayout,
  draggingId,
  onDragArm,
  onConversation,
  onAct,
}: {
  group: { id: string; name: string; collapsed: boolean };
  items: ConversationInfo[];
  page: Page;
  conversationId?: string | undefined;
  bootstrap: Bootstrap;
  layout: GroupLayout;
  persistLayout(next: GroupLayout): void;
  draggingId?: string | undefined;
  onDragArm(event: PointerEvent<HTMLElement>, item: DragItem): void;
  onConversation(id: string): void;
  onAct(action: () => Promise<unknown>): void;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(group.name);
  useEffect(() => setName(group.name), [group.name]);
  return (
    <li
      className={`app-group${draggingId === group.id ? " is-source" : ""}`}
      data-sd-slot="group"
      data-sd-id={group.id}
      style={{ viewTransitionName: vtName(group.id) } as CSSProperties}
    >
      {editing ? (
        <InlineRename
          label="分组名称"
          value={name}
          onChange={setName}
          onSubmit={() => {
            setEditing(false);
            persistLayout(renameGroup(layout, group.id, name));
          }}
          onCancel={() => setEditing(false)}
        />
      ) : (
        <div
          className="app-group-head"
          onPointerDown={(event) =>
            onDragArm(event, { kind: "group", id: group.id, label: group.name })
          }
        >
          <button
            type="button"
            className="app-group-toggle"
            aria-expanded={!group.collapsed}
            aria-label={group.name}
            onClick={() =>
              persistLayout(setGroupCollapsed(layout, group.id, !group.collapsed))
            }
          >
            <span className="app-row-lead" aria-hidden="true">
              <DisclosureChevron
                expanded={!group.collapsed}
                className="app-group-chevron"
              />
            </span>
            <span className="app-conversation-title">{group.name}</span>
            <small>{items.length}</small>
          </button>
          <div className="app-conversation-actions">
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
              aria-label="删除分组"
              title="删除分组"
              onClick={() => {
                if (
                  items.length &&
                  !window.confirm(
                    `删除分组「${group.name}」？里面的对话会回到未分组。`,
                  )
                ) {
                  return;
                }
                persistLayout(deleteGroup(layout, group.id));
              }}
            >
              <TrashIcon className="button-icon" />
            </button>
          </div>
        </div>
      )}
      <div
        className={`app-group-chats${group.collapsed ? " is-collapsed" : ""}`}
        inert={group.collapsed}
      >
        <ul className="app-group-chats-inner">
          {items.map((entry) => (
            <ConversationRow
              key={entry.id}
              entry={entry}
              current={page === "chat" && conversationId === entry.id}
              running={bootstrap.tasks.find(
                (task) => task.conversationId === entry.id && isRunning(task),
              )}
              onOpen={() => onConversation(entry.id)}
              onAct={onAct}
              permissionMode={bootstrap.permission.mode}
              nested
              groupId={group.id}
              draggingId={draggingId}
              onDragArm={onDragArm}
            />
          ))}
        </ul>
      </div>
    </li>
  );
}

function ConversationRow({
  entry,
  current,
  running,
  permissionMode,
  onOpen,
  onAct,
  nested = false,
  groupId = null,
  draggingId,
  onDragArm,
}: {
  entry: ConversationInfo;
  current: boolean;
  running?: { state: string } | undefined;
  permissionMode: string;
  onOpen(): void;
  onAct(action: () => Promise<unknown>): void;
  nested?: boolean;
  groupId?: string | null;
  draggingId?: string | undefined;
  onDragArm(event: PointerEvent<HTMLElement>, item: DragItem): void;
}) {
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(entry.title);
  useEffect(() => setTitle(entry.title), [entry.title]);
  if (editing) {
    return (
      <li
        className={current ? "is-current" : undefined}
        style={{ viewTransitionName: vtName(entry.id) } as CSSProperties}
      >
        <InlineRename
          label="会话名称"
          value={title}
          onChange={setTitle}
          onSubmit={() => {
            setEditing(false);
            void onAct(() => api.renameConversation(entry.id, title));
          }}
          onCancel={() => setEditing(false)}
        />
      </li>
    );
  }
  return (
    <li
      className={`${current ? "is-current" : ""}${nested ? " is-nested" : ""}${
        draggingId === entry.id ? " is-source" : ""
      }`.trim()}
      data-sd-slot="conversation"
      data-sd-id={entry.id}
      data-sd-group={groupId ?? ""}
      style={{ viewTransitionName: vtName(entry.id) } as CSSProperties}
      onPointerDown={(event) =>
        onDragArm(event, {
          kind: "conversation",
          id: entry.id,
          label: entry.title,
        })
      }
    >
      <button
        title={entry.title}
        aria-current={current ? "page" : undefined}
        onClick={onOpen}
      >
        <span className="app-conversation-title">{entry.title}</span>
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
          aria-label="重命名"
          title="重命名"
          onClick={() => setEditing(true)}
        >
          <EditIcon className="button-icon" />
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
