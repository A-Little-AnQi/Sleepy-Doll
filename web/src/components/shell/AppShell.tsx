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
import type { Page } from "../../App";
import { api, framelessWindow } from "../../ipc/api";
import {
  isRunning,
  needsConfirmation,
  readError,
  taskLabels,
} from "../../session";
import type { Bootstrap, ConversationInfo } from "../../ipc/types";
import { hostPluginEnabled } from "../../ipc/providers";
import {
  BridgeIcon,
  EditIcon,
  FolderIcon,
  PanelIcon,
  PluginIcon,
  PlusIcon,
  SearchIcon,
  SidebarIcon,
  ToolIcon,
  TrashIcon,
  Wordmark,
} from "../icons";
import { SidebarAccount } from "./SidebarAccount";
import { TitleBar } from "./TitleBar";
import { InlineRename } from "../controls/InlineRename";
import { DisclosureChevron } from "../controls/DisclosureChevron";
import { Toast } from "../overlay/Toast";
import { ConfirmDialog } from "../overlay/ConfirmDialog";
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
  setMembership,
  splitConversations,
  orderConversations,
  writeLayout,
  type DropSlot,
  type GroupLayout,
} from "../../session/conversation-groups";
import "./app-shell.css";
import { useT } from "../../i18n";

const NAV = [
  { page: "tasks", labelKey: "tasks" as const, Icon: ToolIcon },
  { page: "extensions", labelKey: "extensions" as const, Icon: PluginIcon },
] as const;

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
  /** 输入框已有内容时，在侧栏放一条未落库的新会话。 */
  composingNewChat?: boolean;
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
  composingNewChat = false,
}: Props) {
  const t = useT();
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
  const pendingGroupRef = useRef<string | null>(null);
  const destinationRef = useRef<string | null>(
    conversationId ? (layout.membership[conversationId] ?? null) : null,
  );
  const [destinationGroupId, setDestinationGroupId] = useState<string | null>(
    destinationRef.current,
  );
  const [startedNew, setStartedNew] = useState(false);
  const [renamingGroupId, setRenamingGroupId] = useState<string | null>(null);
  const setDestination = (id: string | null) => {
    destinationRef.current = id;
    setDestinationGroupId(id);
  };
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
  const conversation = bootstrap.conversations.find(
    (entry) => entry.id === conversationId,
  );
  const bridgeLabel =
    bootstrap.bridge.enabled && bootstrap.bridge.connected
      ? t.nav.connected
      : t.nav.disconnected;

  const persistLayout = (next: GroupLayout) => {
    layoutRef.current = next;
    setLayout(next);
    writeLayout(next);
    if (bootstrap.conversationGroups === undefined) return;
    void api.saveConversationGroups(next).catch(() => undefined);
  };
  const startNew = (groupId: string | null = null) => {
    pendingGroupRef.current = groupId;
    setDestination(groupId);
    setStartedNew(true);
    if (groupId) {
      const current = layoutRef.current;
      const group = current.groups.find((item) => item.id === groupId);
      if (group?.collapsed) {
        persistLayout(setGroupCollapsed(current, groupId, false));
      }
    }
    onNew();
  };
  const openExisting = (id: string) => {
    pendingGroupRef.current = null;
    setStartedNew(false);
    setDestination(layoutRef.current.membership[id] ?? null);
    onConversation(id);
  };
  useEffect(() => {
    if (conversationId) setStartedNew(false);
  }, [conversationId]);
  useEffect(() => {
    if (!conversationId) return;
    if (pendingGroupRef.current) return;
    setDestination(layoutRef.current.membership[conversationId] ?? null);
  }, [conversationId]);
  useEffect(() => {
    const groupId = pendingGroupRef.current;
    if (!groupId || !conversationId) return;
    pendingGroupRef.current = null;
    const current = layoutRef.current;
    if (current.membership[conversationId] === groupId) return;
    persistLayout(setMembership(current, conversationId, groupId));
  }, [conversationId]);

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
    const current = layoutRef.current;
    if (ids.join("\0") === current.order.join("\0")) return;
    persistLayout({ ...current, order: ids });
  }, [ghost, orderedConversations]);
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
  const showDraft =
    page === "chat" &&
    !conversationId &&
    !query.trim() &&
    (composingNewChat || startedNew);

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
      if (!active && dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD)
        return;
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

  const detailsToggle = page === "chat";
  const detailsPane = detailsOpen && (page === "chat" || page === "tasks");
  const detailsDrawerOpen = detailsPane && !roomForDetails;
  // 宽窗口未折叠时侧栏占布局列，其余情况靠左缘悬停唤出悬浮抽屉。
  // 窄窗只允许一个模态抽屉；详情打开时侧栏不会同时留在遮罩下。
  const docked = wide && !collapsed;
  const overlayOpen = !docked && (peek || !collapsed) && !detailsDrawerOpen;
  const sidebarVisible = docked || overlayOpen;
  const showPeek = () => {
    window.clearTimeout(peekTimer.current);
    setPeek(true);
  };
  const hidePeek = (event?: { relatedTarget: EventTarget | null }) => {
    if (resizingRef.current || draggingRef.current) return;
    // 指针移出浏览器窗口时 relatedTarget 为空，此时不收起。
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
    if (detailsDrawerOpen) onToggleDetails();
    setCollapsed(false);
  };
  const foldSidebar = () => {
    window.clearTimeout(peekTimer.current);
    setPeek(false);
    setCollapsed(true);
  };
  useEffect(() => {
    if (detailsDrawerOpen && !collapsed) foldSidebar();
  }, [detailsDrawerOpen, collapsed]);
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
  const showHeader = page === "chat" || collapsed;
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
      aria-label={t.nav.sidebar}
      aria-hidden={!sidebarVisible}
      inert={!sidebarVisible}
      onPointerEnter={!docked ? showPeek : undefined}
      onPointerLeave={!docked ? hidePeek : undefined}
    >
      <div className="app-brand">
        <Wordmark />
        <button
          className="icon-button"
          title={wide ? t.nav.collapseSidebar : t.nav.closeSidebar}
          aria-label={wide ? t.nav.collapseSidebar : t.nav.closeSidebar}
          onClick={foldSidebar}
        >
          <SidebarIcon className="button-icon" />
        </button>
      </div>
      <button className="app-new" onClick={() => startNew(null)}>
        <PlusIcon className="button-icon" />
        <span>{t.nav.newChat}</span>
      </button>
      <nav className="app-nav" aria-label={t.nav.mainNav}>
        {NAV.map(({ page: target, labelKey, Icon }) => (
          <button
            key={target}
            className={`app-nav-item${page === target ? " is-active" : ""}`}
            aria-current={page === target ? "page" : undefined}
            onClick={() => onPage(target)}
          >
            <Icon className="app-nav-icon" />
            <span>{t.nav[labelKey]}</span>
            {target === "tasks" &&
              bootstrap.tasks.some((task) => isRunning(task)) && (
                <span className="app-nav-badge" title={t.nav.runningBadge}>
                  {bootstrap.tasks.filter((task) => isRunning(task)).length}
                </span>
              )}
          </button>
        ))}
      </nav>
      <div className="app-sidebar-tools">
        <div className="app-section-head">
          <span>{t.app.recent}</span>
          <button
            className="icon-button"
            title={t.nav.newGroup}
            aria-label={t.nav.newGroup}
            onClick={() => {
              const next = createGroup(layout);
              const created = next.groups[next.groups.length - 1];
              if (!created) return;
              persistLayout(next);
              setRenamingGroupId(created.id);
            }}
          >
            <FolderIcon className="button-icon" />
          </button>
        </div>
        <label className="search-field is-compact">
          <SearchIcon />
          <input
            aria-label={t.nav.searchConversations}
            placeholder={t.nav.searchPlaceholder}
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
              onConversation={openExisting}
              onAct={conversationAct}
              draft={showDraft && destinationGroupId === group.id}
              renaming={renamingGroupId === group.id}
              onRenaming={(open) => setRenamingGroupId(open ? group.id : null)}
              onNew={() => startNew(group.id)}
              onDelete={() => {
                if (destinationRef.current === group.id) setDestination(null);
                persistLayout(deleteGroup(layout, group.id));
              }}
            />
          ))}
          {showDraft && !destinationGroupId && <DraftRow />}
          {grouped.ungrouped.map((entry) => (
            <ConversationRow
              key={entry.id}
              entry={entry}
              current={page === "chat" && conversationId === entry.id}
              running={bootstrap.tasks.find(
                (task) => task.conversationId === entry.id && isRunning(task),
              )}
              onOpen={() => openExisting(entry.id)}
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
          {!conversations.length && !showDraft && query.trim() ? (
            <li className="app-conversation-empty">{t.nav.noMatch}</li>
          ) : null}
        </ul>
      </div>
      {hostPluginEnabled(bootstrap) && (
        <div className="app-sidebar-status">
          <button className="app-connection" onClick={() => onPage("bridge")}>
            <BridgeIcon className="app-nav-icon" />
            <span>BetterGI</span>
            <small>{bridgeLabel}</small>
          </button>
        </div>
      )}
      <div className="app-sidebar-foot">
        <SidebarAccount
          onHelp={() => onPage("help")}
          onSettings={() => onPage("settings")}
        />
      </div>
      <div
        className="app-sidebar-resize"
        role="separator"
        aria-orientation="vertical"
        aria-label={t.nav.resizeSidebar}
        aria-valuenow={sidebarWidth}
        aria-valuemin={SIDEBAR_MIN_WIDTH}
        aria-valuemax={SIDEBAR_MAX_WIDTH}
        onPointerDown={onResizePointerDown}
      />
    </aside>
  );

  return (
    <>
      {framelessWindow() && <TitleBar />}
      <div
        className="app-shell"
        data-collapsed={!docked}
        data-drawer={overlayOpen}
        data-resizing={resizing}
        data-details={detailsPane && roomForDetails}
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
        {!wide && !collapsed && !detailsDrawerOpen && (
          <button
            className="app-scrim"
            aria-label={t.nav.closeSidebar}
            onClick={foldSidebar}
          />
        )}
        <main className="app-main">
          {showHeader ? (
            <header className="app-header">
              <div className="app-header-title">
                {collapsed && (
                  <>
                    <button
                      className="icon-button"
                      title={t.nav.expandSidebar}
                      aria-label={t.nav.expandSidebar}
                      onClick={pinSidebar}
                    >
                      <SidebarIcon className="button-icon" />
                    </button>
                    <button
                      className="icon-button"
                      title={t.nav.newChat}
                      aria-label={t.nav.newChat}
                      onClick={() => startNew(null)}
                    >
                      <PlusIcon className="button-icon" />
                    </button>
                  </>
                )}
                {page === "chat" ? (
                  <h1>{conversation?.title ?? t.nav.newConversation}</h1>
                ) : null}
              </div>
              {detailsToggle ? (
                <div className="app-header-actions">
                  <button
                    className="icon-button"
                    aria-pressed={detailsOpen}
                    aria-label={detailsOpen ? t.nav.hideDetails : t.nav.showDetails}
                    title={detailsOpen ? t.nav.hideDetails : t.nav.showDetails}
                    onClick={() => {
                      if (!detailsOpen && !roomForDetails) foldSidebar();
                      onToggleDetails();
                    }}
                  >
                    <PanelIcon className="button-icon" />
                  </button>
                </div>
              ) : null}
            </header>
          ) : null}
          {error && <Toast message={error} onDismiss={() => setError("")} />}
          <div className="app-body">
            <div className="app-view">{children}</div>
            {detailsPane &&
              (roomForDetails ? (
                details
              ) : (
                <>
                  <button
                    className="details-scrim"
                    aria-label={t.nav.closeDetails}
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
    </>
  );
}

function DraftRow({ nested = false }: { nested?: boolean }) {
  const t = useT();
  return (
    <li className={`is-current${nested ? " is-nested" : ""}`}>
      <button type="button" aria-current="page">
        <span className="app-conversation-title">{t.nav.newConversation}</span>
      </button>
    </li>
  );
}

function GroupContextMenu({
  x,
  y,
  onRename,
  onDelete,
  onClose,
}: {
  x: number;
  y: number;
  onRename(): void;
  onDelete(): void;
  onClose(): void;
}) {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const node = root.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - rect.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - rect.height - 8)),
    });
  }, [x, y]);
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      if (
        event.target instanceof Node &&
        root.current?.contains(event.target)
      ) {
        return;
      }
      onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    document.addEventListener("scroll", onClose, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("scroll", onClose, true);
    };
  }, [onClose]);
  return createPortal(
    <div
      ref={root}
      className="app-context-menu"
      role="menu"
      aria-label={t.app.group}
      style={{ left: pos.left, top: pos.top }}
    >
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onClose();
          onRename();
        }}
      >
        重命名
      </button>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onClose();
          onDelete();
        }}
      >
        {t.app.deleteGroup}
      </button>
    </div>,
    document.body,
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
  draft = false,
  renaming = false,
  onRenaming,
  onNew,
  onDelete,
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
  draft?: boolean;
  renaming?: boolean;
  onRenaming(open: boolean): void;
  onNew(): void;
  onDelete(): void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(renaming);
  const [name, setName] = useState(group.name);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [asking, setAsking] = useState(false);
  useEffect(() => setName(group.name), [group.name]);
  useEffect(() => {
    if (renaming) setEditing(true);
  }, [renaming]);
  const stopEditing = (save: boolean) => {
    setEditing(false);
    onRenaming(false);
    if (save) persistLayout(renameGroup(layout, group.id, name));
  };
  const remove = () => {
    if (!items.length && !draft) {
      onDelete();
      return;
    }
    setAsking(true);
  };
  const showBody = items.length > 0 || draft;
  return (
    <li
      className={`app-group${draggingId === group.id ? " is-source" : ""}`}
      data-sd-slot="group"
      data-sd-id={group.id}
      style={{ viewTransitionName: vtName(group.id) } as CSSProperties}
    >
      {editing ? (
        <InlineRename
          label={t.app.groupName}
          value={name}
          onChange={setName}
          onSubmit={() => stopEditing(true)}
          onCancel={() => stopEditing(false)}
        />
      ) : (
        <div
          className="app-group-head"
          onPointerDown={(event) =>
            onDragArm(event, { kind: "group", id: group.id, label: group.name })
          }
          onContextMenu={(event) => {
            event.preventDefault();
            event.stopPropagation();
            setMenu({ x: event.clientX, y: event.clientY });
          }}
        >
          <button
            type="button"
            className="app-group-toggle"
            aria-expanded={!group.collapsed}
            aria-label={group.name}
            onClick={() =>
              persistLayout(
                setGroupCollapsed(layout, group.id, !group.collapsed),
              )
            }
          >
            <span className="app-group-disclose" aria-hidden="true">
              <span className="app-row-lead">
                <DisclosureChevron
                  expanded={!group.collapsed}
                  className="app-group-chevron"
                />
              </span>
            </span>
            <span className="app-conversation-title">{group.name}</span>
            <small aria-hidden="true">{items.length}</small>
          </button>
          <div className="app-conversation-actions">
            <button
              className="icon-button"
              aria-label={t.app.newChatInGroup}
              title={t.app.newChatInGroup}
              onClick={onNew}
            >
              <PlusIcon className="button-icon" />
            </button>
          </div>
        </div>
      )}
      {menu && (
        <GroupContextMenu
          x={menu.x}
          y={menu.y}
          onRename={() => {
            setEditing(true);
            onRenaming(true);
          }}
          onDelete={remove}
          onClose={() => setMenu(null)}
        />
      )}
      {showBody && (
        <div
          className={`app-group-chats${group.collapsed ? " is-collapsed" : ""}`}
          inert={group.collapsed}
        >
          <ul className="app-group-chats-inner">
            {draft && <DraftRow nested />}
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
      )}
      <ConfirmDialog
        open={asking}
        title={t.app.deleteGroup}
        confirmLabel={t.app.deleteGroup}
        onClose={() => setAsking(false)}
        onConfirm={() => {
          setAsking(false);
          onDelete();
        }}
      >
        <p>删除「{group.name}」后，里面的对话会回到未分组。</p>
      </ConfirmDialog>
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
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(entry.title);
  const [asking, setAsking] = useState<{
    title: string;
    tasks: number;
    keeps: string;
  } | null>(null);
  useEffect(() => setTitle(entry.title), [entry.title]);
  if (editing) {
    return (
      <li
        className={current ? "is-current" : undefined}
        style={{ viewTransitionName: vtName(entry.id) } as CSSProperties}
      >
        <InlineRename
          label={t.app.conversationName}
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
    <>
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
              title={taskLabels[running.state] ?? t.app.running}
              aria-label={taskLabels[running.state] ?? t.app.running}
            />
          )}
        </button>
        <div className="app-conversation-actions">
          <button
            className="icon-button"
            aria-label={t.app.rename}
            title={t.app.rename}
            onClick={() => setEditing(true)}
          >
            <EditIcon className="button-icon" />
          </button>
          <button
            className="icon-button"
            aria-label={t.app.deleteConversation}
            title={t.app.deleteConversation}
            onClick={() => {
              void (async () => {
                const first = await api.deleteConversation(entry.id, false);
                if (!first.requiresConfirmation) return;
                const preview = {
                  title: first.affects?.title ?? entry.title,
                  tasks: first.affects?.taskCount ?? 0,
                  keeps: first.keeps ?? "",
                };
                if (!needsConfirmation(permissionMode)) {
                  await api.deleteConversation(entry.id, true);
                  await onAct(async () => {});
                  return;
                }
                setAsking(preview);
              })().catch(() => undefined);
            }}
          >
            <TrashIcon className="button-icon" />
          </button>
        </div>
      </li>
      <ConfirmDialog
        open={asking != null}
        title={t.app.deleteConversation}
        confirmLabel={t.app.deleteConversation}
        onClose={() => setAsking(null)}
        onConfirm={() => {
          const preview = asking;
          setAsking(null);
          if (!preview) return;
          void onAct(() => api.deleteConversation(entry.id, true));
        }}
      >
        {asking ? (
          <>
            <p>
              会删除「{asking.title}」里的消息和运行记录
              {asking.tasks ? `，其中 ${asking.tasks} 个快捷任务会保留` : ""}。
            </p>
            {asking.keeps ? <p>{asking.keeps}</p> : null}
          </>
        ) : null}
      </ConfirmDialog>
    </>
  );
}
