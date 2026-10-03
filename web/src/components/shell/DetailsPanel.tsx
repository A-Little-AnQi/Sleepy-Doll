import { useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { isRunning, readError, taskLabels } from "../../session";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import { ChevronIcon, CloseIcon, HistoryIcon, SearchIcon } from "../icons";
import { TaskCard, type TaskActions } from "../tasks/TaskCard";
import { taskRun, taskError } from "../tasks/task-display";
import { ConfirmDialog } from "../overlay/ConfirmDialog";
import { Toast } from "../overlay/Toast";
import "./details-panel.css";

export function DetailsPanel({
  bootstrap,
  selectedTask,
  onSelectTask,
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
  const currentRun = selected ? taskRun(selected, bootstrap.tasks) : undefined;
  return (
    <aside
      ref={panel}
      className="details-panel shortcut-shelf"
      aria-label="快捷任务"
    >
      <header className="details-head">
        {selected ? (
          <button
            className="icon-button"
            aria-label="返回快捷任务"
            onClick={() => onSelectTask(undefined)}
          >
            <ChevronIcon className="button-icon shortcut-back-icon" />
          </button>
        ) : (
          <span className="details-heading-icon">
            <HistoryIcon />
          </span>
        )}
        <h2>{selected ? selected.name : "快捷任务"}</h2>
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
              activeRun={currentRun}
              busy={busy === selected.id}
              actions={actions}
            />
            <section className="shortcut-purpose">
              <h3>这项任务</h3>
              <p>
                {selected.description ||
                  `运行 ${selected.shortcut?.targetName}`}
              </p>
              <dl>
                <dt>应用</dt>
                <dd>{selected.shortcut?.applicationName}</dd>
                <dt>任务</dt>
                <dd>{selected.shortcut?.targetName}</dd>
              </dl>
              <p className="muted">
                点击运行即可执行，不会重新配置或重放对话。要调整绑定，在对话里让
                AI 修改这个入口。
              </p>
            </section>
            {currentRun && (
              <section className="shortcut-purpose">
                <h3>{isRunning(currentRun) ? "正在运行" : "最近一次运行"}</h3>
                <p>{taskLabels[currentRun.state] ?? currentRun.state}</p>
                {currentRun.error && (
                  <p className="shortcut-run-error">
                    {taskError(currentRun.error)}
                  </p>
                )}
                <small>{new Date(currentRun.createdAt).toLocaleString()}</small>
              </section>
            )}
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
