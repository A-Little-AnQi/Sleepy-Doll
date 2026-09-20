import { useEffect, useRef, useState } from "react";
import type { TaskSummary } from "../../ipc/types";
import { MoreIcon, PinIcon } from "../icons";
import "./task-card.css";
import { useT } from "../../i18n";

export interface TaskActions {
  run(task: TaskSummary): void;
  rename?(task: TaskSummary): void;
  pin?(task: TaskSummary, pinned: boolean): void;
  archive?(task: TaskSummary, archived: boolean): void;
  copy?(task: TaskSummary): void;
  remove?(task: TaskSummary): void;
  openSource?(task: TaskSummary): void;
  askAi?(task: TaskSummary): void;
  connect?(task: TaskSummary): void;
  open?(task: TaskSummary): void;
}

/**
 * 快捷任务卡：名称、描述、就绪状态、一个主按钮。
 */
export function TaskCard({
  task,
  busy,
  actions,
  showSource,
}: {
  task: TaskSummary;
  busy?: boolean;
  actions: TaskActions;
  showSource?: boolean;
}) {
  const t = useT();
  const [menu, setMenu] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const close = (event: MouseEvent) => {
      if (!container.current?.contains(event.target as Node)) setMenu(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [menu]);
  const primary = task.runnable
    ? () => actions.run(task)
    : task.state === "archived"
      ? () => actions.archive?.(task, false)
      : task.state === "unavailable"
        ? () => actions.connect?.(task)
        : () => actions.open?.(task);
  return (
    <article className="task-card" data-state={task.state}>
      <div className="task-card-head">
        <button className="task-card-name" onClick={() => actions.open?.(task)}>
          {task.name}
        </button>
        {task.pinned && (
          <span className="task-card-pinned" title={t.taskCard.pinned}>
            <PinIcon />
          </span>
        )}
        <div className="task-card-menu" ref={container}>
          <button
            className="icon-button"
            aria-label={`${task.name} 的更多操作`}
            aria-expanded={menu}
            aria-haspopup="menu"
            onClick={() => setMenu(!menu)}
          >
            <MoreIcon className="button-icon" />
          </button>
          {menu && (
            <div className="task-menu" role="menu">
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
                  让 AI 修改
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
              {actions.archive && (
                <button
                  role="menuitem"
                  onClick={() => {
                    setMenu(false);
                    actions.archive?.(task, task.state !== "archived");
                  }}
                >
                  {task.state === "archived" ? t.taskCard.restore : t.taskCard.archive}
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
            </div>
          )}
        </div>
      </div>
      <p className="task-card-description">
        {task.description || t.taskCard.noDescription}
      </p>
      <div className="task-card-foot">
        <span className="task-card-state" data-state={task.state}>
          {task.stateLabel}
        </span>
        {task.zeroToken && task.runnable && (
          <span className="task-card-note" title={t.taskCard.offlineNote}>
            {t.taskCard.offlineShort}
          </span>
        )}
        {task.sourceDeleted && showSource && (
          <span className="task-card-note">{t.taskCard.sourceDeleted}</span>
        )}
        <button
          className="primary-action"
          disabled={busy || task.state === "deleted"}
          onClick={primary}
        >
          {busy ? t.app.running : task.actionLabel || t.taskCard.open}
        </button>
      </div>
      {task.issue && task.state !== "archived" && (
        <p className="task-card-issue">{task.issue}</p>
      )}
    </article>
  );
}
