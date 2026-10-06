import { useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { isRunning, readError, taskLabels } from "../../session";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import { CloseIcon, HistoryIcon, SearchIcon } from "../icons";
import { TaskCard, type TaskActions } from "../tasks/TaskCard";
import { taskRun, taskError } from "../tasks/task-display";
import { taskPlan } from "../tasks/task-plan";
import { ConfirmDialog } from "../overlay/ConfirmDialog";
import { Toast } from "../overlay/Toast";
import "./details-panel.css";

export function DetailsPanel({
  bootstrap,
  selectedTask,
  onSelectTask,
  onOpenConversation,
  onConnectTools,
  reload,
  onClose,
}: {
  bootstrap: Bootstrap;
  selectedTask?: string | undefined;
  onSelectTask(id: string | undefined): void;
  onOpenConversation(id: string): void;
  onConnectTools?(): void;
  reload(): Promise<void>;
  onClose(): void;
}) {
  const panel = useRef<HTMLElement>(null);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [removing, setRemoving] = useState<TaskSummary | null>(null);
  const [renaming, setRenaming] = useState<TaskSummary | null>(null);
  const [newName, setNewName] = useState("");
  const tasks = bootstrap.workflows
    .filter(
      (task) =>
        task.shortcut && task.state !== "deleted" && task.state !== "archived",
    )
    .sort(
      (a, b) =>
        Number(b.pinned) - Number(a.pinned) || a.name.localeCompare(b.name),
    );
  const selected = bootstrap.workflows.find(
    (task) =>
      task.shortcut && task.state !== "deleted" && task.id === selectedTask,
  );
  const visible = tasks.filter((task) =>
    (task.name + task.description + task.shortcut?.applicationName)
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const key = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !element.querySelector('[aria-expanded="true"]')
      )
        onClose();
    };
    element.addEventListener("keydown", key);
    return () => element.removeEventListener("keydown", key);
  }, [onClose]);
  const act = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError("");
    try {
      await action();
      await reload();
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      setBusy("");
    }
  };
  const actions: TaskActions = {
    run: (task) =>
      void act(task.id, () =>
        api.runWorkflow(task.id, task.publishedRevision ?? undefined),
      ),
    stop: (task) => {
      const run = taskRun(task, bootstrap.tasks);
      if (run) void act(task.id, () => api.cancelTask(run.id));
    },
    open: (task) => onSelectTask(task.id),
    pin: (task, pinned) =>
      void act(task.id, () => api.pinWorkflow(task.id, pinned)),
    rename: (task) => {
      setRenaming(task);
      setNewName(task.name);
    },
    archive: (task, archived) =>
      void act(task.id, () => api.archiveWorkflow(task.id, archived)),
    remove: (task) => setRemoving(task),
    connect: () => onConnectTools?.(),
  };
  const selectedActions = { ...actions };
  delete selectedActions.open;
  const currentRun = selected ? taskRun(selected, bootstrap.tasks) : undefined;
  // 与该任务关联的运行记录：倒序取最近 5 条，含进行中的一次。
  const runs = selected
    ? bootstrap.tasks
        .filter(
          (run) =>
            run.source?.kind === "savedWorkflow" &&
            run.source.workflowId === selected.id,
        )
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
        .slice(0, 5)
    : [];
  return (
    <aside
      ref={panel}
      className="details-panel shortcut-shelf"
      aria-label="快捷任务"
    >
      <header className="details-head">
        <span className="details-heading-icon">
          <HistoryIcon />
        </span>
        <h2>{selected ? "任务详情" : "快捷任务"}</h2>
        {!selected && <span className="details-count">{tasks.length}</span>}
        <button
          className="icon-button"
          aria-label="关闭快捷任务侧栏"
          onClick={onClose}
        >
          <CloseIcon className="button-icon" />
        </button>
      </header>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="details-body">
        {selected ? (
          <>
            <TaskCard
              task={selected}
              activeRun={isRunning(currentRun) ? currentRun : undefined}
              busy={busy === selected.id}
              actions={selectedActions}
            />
            <section className="shortcut-purpose">
              <h3>执行内容</h3>
              <ol className="shortcut-plan">
                {taskPlan(selected.shortcut).map((line, index) => (
                  <li key={index}>{line}</li>
                ))}
              </ol>
            </section>
            {selected.sourceConversationId && !selected.sourceDeleted && (
              <section className="shortcut-purpose">
                <div className="shortcut-source-row">
                  <h3>来源对话</h3>
                  <small>
                    更新于 {new Date(selected.updatedAt).toLocaleString()}
                  </small>
                </div>
                <button
                  className="subtle-action shortcut-source"
                  onClick={() =>
                    onOpenConversation(selected.sourceConversationId!)
                  }
                >
                  {selected.sourceTitleSnapshot || "打开对话"}
                </button>
              </section>
            )}
            <section className="shortcut-purpose">
              <h3>运行记录</h3>
              {runs.length ? (
                <ul className="detail-runs shortcut-runs">
                  {runs.map((run) => (
                    <li key={run.id} data-running={isRunning(run)}>
                      <div className="detail-run-meta">
                        <span className="detail-run-state">
                          <i />
                          {taskLabels[run.state] ?? run.state}
                        </span>
                        <time>{new Date(run.createdAt).toLocaleString()}</time>
                      </div>
                      {run.error && (
                        <p className="shortcut-run-error">
                          {taskError(run.error)}
                        </p>
                      )}
                      {run.result && (
                        <p className="shortcut-run-result">{run.result}</p>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>尚未运行。</p>
              )}
            </section>
          </>
        ) : (
          <>
            {tasks.length > 0 && (
              <div className="shortcut-shelf-toolbar">
                <p>已保存的任务，随时点击运行。</p>
              </div>
            )}
            {tasks.length > 4 && (
              <label className="search-field">
                <SearchIcon />
                <input
                  aria-label="搜索快捷任务"
                  placeholder="搜索任务"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
              </label>
            )}
            {visible.length ? (
              <div className="shortcut-shelf-list">
                {visible.map((task) => (
                  <TaskCard
                    key={task.id}
                    task={task}
                    activeRun={taskRun(task, bootstrap.tasks)}
                    busy={busy === task.id}
                    actions={actions}
                  />
                ))}
              </div>
            ) : (
              <div className="details-empty">
                <span className="details-empty-icon">
                  <HistoryIcon />
                </span>
                <h3>{query ? "没有匹配的任务" : "还没有入口"}</h3>
                <p>
                  {query
                    ? "换个名称试试。"
                    : "在对话里输入「创建快捷任务」，保存后会出现在这里。"}
                </p>
              </div>
            )}
          </>
        )}
      </div>
      <ConfirmDialog
        open={removing != null}
        title="删除快捷任务"
        confirmLabel="删除"
        onClose={() => setRemoving(null)}
        onConfirm={() => {
          const task = removing;
          setRemoving(null);
          if (task) void act(task.id, () => api.deleteWorkflow(task.id));
        }}
      >
        <p>删除「{removing?.name}」的快捷入口？</p>
        <p>已经配置好的任务不会被删除，正在运行的任务也不会被停止。</p>
      </ConfirmDialog>
      <ConfirmDialog
        open={renaming != null}
        title="重命名快捷任务"
        confirmLabel="保存"
        onClose={() => setRenaming(null)}
        onConfirm={() => {
          const task = renaming;
          setRenaming(null);
          if (task && newName.trim())
            void act(task.id, () =>
              api.renameWorkflow(task.id, newName.trim()),
            );
        }}
      >
        <label className="shortcut-name-field">
          任务名称
          <input
            autoFocus
            value={newName}
            onChange={(event) => setNewName(event.target.value)}
            maxLength={120}
          />
        </label>
      </ConfirmDialog>
    </aside>
  );
}
