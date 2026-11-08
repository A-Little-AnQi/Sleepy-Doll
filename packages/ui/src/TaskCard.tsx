import { IconButton, Button } from "./Button";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { createPortal } from "react-dom";

import { MoreIcon, PinIcon } from "./icons";
import "./task-card.css";
import { useT } from "./i18n";

export interface TaskCardData {
  id: string;
  name: string;
  description: string;
  state: string;
  stateLabel: string;
  actionLabel: string;
  runnable: boolean;
  pinned: boolean;
  zeroToken: boolean;
  sourceDeleted: boolean;
  shortcut?: { applicationName: string; targetName: string } | null;
  sourceConversationId?: string | null;
  issue?: string | null;
}
type CardRun = { state: string; error?: string | null };

export interface TaskActions<T extends TaskCardData = TaskCardData> {
  stop?(task: T): void;
  run(task: T): void;
  rename?(task: T): void;
  pin?(task: T, pinned: boolean): void;
  copy?(task: T): void;
  remove?(task: T): void;
  openSource?(task: T): void;
  askAi?(task: T): void;
  connect?(task: T): void;
  open?(task: T): void;
  schedule?(task: T): void;
}

/**
 * 快捷任务卡：名称与软件标签、描述、组项、一个主按钮。
 */
export function TaskCard<T extends TaskCardData>({
  task,
  busy,
  actions,
  showSource,
  showDescription = true,
  activeRun,
  running = false,
  applications,
  targets,
  issueText,
  runErrorText,
}: {
  task: T;
  busy?: boolean;
  actions: TaskActions<T>;
  showSource?: boolean;
  showDescription?: boolean;
  activeRun?: CardRun | undefined;
  running?: boolean;
  applications?: readonly string[];
  targets?: readonly string[];
  issueText?: string;
  runErrorText?: string;
}) {
  const t = useT();
  const [menu, setMenu] = useState(false);
  const [menuPoint, setMenuPoint] = useState<{ x: number; y: number } | null>(
    null,
  );
  const container = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const softwareNames =
    applications ?? (task.shortcut ? [task.shortcut.applicationName] : []);
  const targetNames =
    targets ?? (task.shortcut ? [task.shortcut.targetName] : []);
  useLayoutEffect(() => {
    const node = panel.current;
    const trigger = container.current?.querySelector("button");
    if (!menu || !node || !trigger) return;
    const box = trigger.getBoundingClientRect();
    node.style.left = `${Math.max(8, Math.min(menuPoint?.x ?? box.right - node.offsetWidth, innerWidth - node.offsetWidth - 8))}px`;
    node.style.top = `${Math.max(8, Math.min(menuPoint?.y ?? box.bottom + 4, innerHeight - node.offsetHeight - 8))}px`;
    node.style.transformOrigin = menuPoint ? "top left" : "top right";
  }, [menu, menuPoint]);
  useEffect(() => {
    if (!menu) return;
    const frame = requestAnimationFrame(() =>
      panel.current
        ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
        ?.focus({ preventScroll: true }),
    );
    const close = (event: PointerEvent) => {
      if (
        !container.current?.contains(event.target as Node) &&
        !panel.current?.contains(event.target as Node)
      )
        setMenu(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      setMenu(false);
      container.current
        ?.querySelector("button")
        ?.focus({ preventScroll: true });
    };
    const scroll = (event: Event) => {
      if (!panel.current?.contains(event.target as Node)) setMenu(false);
    };
    const resize = () => setMenu(false);
    document.addEventListener("pointerdown", close, true);
    document.addEventListener("keydown", escape, true);
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", resize);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", close, true);
      document.removeEventListener("keydown", escape, true);
      document.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", resize);
    };
  }, [menu]);
  const primary = task.runnable
    ? () => actions.run(task)
    : task.state === "unavailable"
      ? () => actions.connect?.(task)
      : () => actions.open?.(task);
  const openable = Boolean(actions.open);
  // 整卡点击打开详情：按钮/菜单等交互元素与正在选择的文本都不误触。
  const openFromCard = (event: ReactMouseEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element ? event.target : null;
    if (
      target?.closest(
        "button, a, input, textarea, select, label, .task-card-menu",
      )
    )
      return;
    if (window.getSelection()?.toString()) return;
    event.currentTarget
      .querySelector<HTMLButtonElement>(".task-card-name")
      ?.focus({ preventScroll: true });
    actions.open?.(task);
  };
  return (
    <article
      className="task-card list-item"
      data-state={task.state}
      data-running={running}
      data-openable={openable || undefined}
      data-list-surface={openable || undefined}
      data-context-menu-owner="task"
      onClick={openable ? openFromCard : undefined}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        setMenuPoint(
          event.clientX || event.clientY
            ? { x: event.clientX, y: event.clientY }
            : null,
        );
        setMenu(true);
      }}
    >
      <div className="task-card-head">
        <div className="task-card-heading">
          {actions.open ? (
            <button
              className="task-card-name list-item-trigger"
              onClick={() => actions.open?.(task)}
            >
              {task.name}
            </button>
          ) : (
            <span className="task-card-name">{task.name}</span>
          )}
          {softwareNames.filter(Boolean).map((name) => (
            <span className="task-card-tag" key={name}>
              {name}
            </span>
          ))}
        </div>
        {task.pinned && (
          <span className="task-card-pinned" title={t.taskCard.pinned}>
            <PinIcon />
          </span>
        )}
        <div className="task-card-menu" ref={container}>
          <IconButton
            aria-label={`${task.name} 的更多操作`}
            aria-expanded={menu}
            aria-haspopup="menu"
            onClick={() => {
              setMenuPoint(null);
              setMenu(!menu);
            }}
          >
            <MoreIcon className="button-icon" />
          </IconButton>
          {menu &&
            createPortal(
              <div
                className="task-menu task-card-action-menu"
                role="menu"
                ref={panel}
                aria-label={`${task.name} 的更多操作`}
                data-context-menu-owner="task"
                onClick={(event) => event.stopPropagation()}
                onKeyDown={(event) => {
                  if (
                    !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)
                  )
                    return;
                  event.preventDefault();
                  const items = Array.from(
                    event.currentTarget.querySelectorAll<HTMLButtonElement>(
                      "button:not(:disabled)",
                    ),
                  );
                  const current = items.indexOf(
                    document.activeElement as HTMLButtonElement,
                  );
                  const next =
                    event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? items.length - 1
                        : (current +
                            (event.key === "ArrowUp" ? -1 : 1) +
                            items.length) %
                          items.length;
                  items[next]?.focus();
                }}
              >
                {actions.schedule && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.schedule?.(task);
                    }}
                  >
                    安排时间
                  </button>
                )}
                {actions.rename && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.rename?.(task);
                    }}
                  >
                    重命名
                  </button>
                )}
                {actions.pin && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.pin?.(task, !task.pinned);
                    }}
                  >
                    {task.pinned ? t.taskCard.unpin : t.taskCard.pin}
                  </button>
                )}
                {actions.copy && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.copy?.(task);
                    }}
                  >
                    复制
                  </button>
                )}
                {actions.askAi && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.askAi?.(task);
                    }}
                  >
                    调整入口
                  </button>
                )}
                {showSource &&
                  actions.openSource &&
                  task.sourceConversationId && (
                    <button
                      role="menuitem"
                      onClick={() => {
                        setMenu(false);
                        actions.openSource?.(task);
                      }}
                    >
                      查看来源对话
                    </button>
                  )}
                {actions.remove && (
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      actions.remove?.(task);
                    }}
                  >
                    删除任务
                  </button>
                )}
              </div>,
              document.body,
            )}
        </div>
      </div>
      {showDescription && (
        <p className="task-card-description">
          {task.description || t.taskCard.noDescription}
        </p>
      )}
      {targetNames.length > 0 && (
        <div className="task-card-targets">
          {targetNames.filter(Boolean).map((name, index) => (
            <span key={`${index}:${name}`}>{name}</span>
          ))}
        </div>
      )}
      <div className="task-card-foot">
        {!task.shortcut && task.zeroToken && task.runnable && (
          <span className="task-card-note" title={t.taskCard.offlineNote}>
            {t.taskCard.offlineShort}
          </span>
        )}
        {task.sourceDeleted && showSource && (
          <span className="task-card-note">{t.taskCard.sourceDeleted}</span>
        )}
        <Button
          variant="primary"
          disabled={
            busy ||
            task.state === "deleted" ||
            activeRun?.state === "cancelling"
          }
          onClick={running ? () => actions.stop?.(task) : primary}
        >
          {busy
            ? "正在处理"
            : running
              ? activeRun?.state === "cancelling"
                ? "正在停止"
                : "停止"
              : task.actionLabel || t.taskCard.open}
        </Button>
      </div>
      {task.issue && (
        <p className="task-card-issue">{issueText ?? task.issue}</p>
      )}
      {!running && activeRun?.error && (
        <p className="task-card-issue">{runErrorText ?? activeRun.error}</p>
      )}
    </article>
  );
}
