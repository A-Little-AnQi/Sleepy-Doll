import { useMemo, useState } from "react";
import { api } from "../../ipc/api";
import { HistoryIcon, PlusIcon, SearchIcon } from "../../components/icons";
import { TaskCard, type TaskActions } from "../../components/tasks/TaskCard";
import { ShortcutConfigurator } from "../../components/tasks/ShortcutConfigurator";
import { taskRun, taskError } from "../../components/tasks/task-display";
import { isRunning, readError, taskLabels } from "../../session";
import { Toast } from "../../components/overlay/Toast";
import { ConfirmDialog } from "../../components/overlay/ConfirmDialog";
import { SlidingTabs } from "../../components/controls/SlidingTabs";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import "./TasksPage.css";

export function TasksPage({
  bootstrap,
  reload,
  onOpenTask,
  onConnectTools,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onOpenConversation(id: string): void;
  onOpenTask?: (task: TaskSummary) => void;
  onConnectTools?: () => void;
}) {
  const [tab, setTab] = useState<"tasks" | "runs">("tasks");
  const [query, setQuery] = useState("");
  const [archived, setArchived] = useState(false);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [configuring, setConfiguring] = useState(false);
  const [target, setTarget] = useState<TaskSummary>();
  const [removing, setRemoving] = useState<TaskSummary | null>(null);
  const [renaming, setRenaming] = useState<TaskSummary | null>(null);
  const [newName, setNewName] = useState("");
  const tasks = bootstrap.workflows.filter((task) => task.state !== "deleted");
  const needle = query.trim().toLowerCase();
  const visible = useMemo(
    () =>
      tasks
        .filter(
          (task) =>
            task.shortcut &&
            (archived
              ? task.state === "archived"
              : task.state !== "archived") &&
            `${task.name} ${task.description} ${task.shortcut.applicationName} ${task.shortcut.targetName}`
              .toLowerCase()
              .includes(needle),
        )
        .sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) || a.name.localeCompare(b.name),
        ),
    [bootstrap.workflows, archived, needle],
  );
  const legacy = tasks.filter((task) => !task.shortcut);
  const runs = bootstrap.tasks.filter(
    (run) =>
      run.source?.kind === "savedWorkflow" &&
      run.prompt.toLowerCase().includes(needle),
  );
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
  const configure = (task?: TaskSummary) => {
    setTarget(task);
    setConfiguring(true);
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
    rename: (task) => {
      setRenaming(task);
      setNewName(task.name);
    },
    pin: (task, pinned) =>
      void act(task.id, () => api.pinWorkflow(task.id, pinned)),
    archive: (task, value) =>
      void act(task.id, () => api.archiveWorkflow(task.id, value)),
    remove: (task) => setRemoving(task),
    askAi: (task) => configure(task),
    connect: () => onConnectTools?.(),
    open: (task) => onOpenTask?.(task),
  };
  return (
    <div className="page-sheet tasks-page">
      <div className="page-title">
        <div>
          <h2>快捷任务</h2>
          <p className="tasks-intro">
            把你指定的任务放在这里，之后点击即可运行。
          </p>
        </div>
        <button className="primary-action" onClick={() => configure()}>
          <PlusIcon className="button-icon" />
          添加快捷任务
        </button>
      </div>
      <div className="list-toolbar">
        <SlidingTabs
          ariaLabel="快捷任务"
          value={tab}
          onChange={setTab}
          items={[
            { id: "tasks", name: "已保存任务" },
            { id: "runs", name: "运行记录" },
          ]}
        />
        <span className="muted">
          {
            bootstrap.tasks.filter(
              (run) => run.source?.kind === "savedWorkflow" && isRunning(run),
            ).length
          }{" "}
          项正在运行
        </span>
      </div>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="list-toolbar">
        <label className="search-field">
          <SearchIcon />
          <input
            aria-label={tab === "tasks" ? "搜索快捷任务" : "搜索运行记录"}
            placeholder="搜索任务名称或应用"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        {tab === "tasks" && (
          <div className="filter-row">
            <button
              className="subtle-action"
              aria-pressed={!archived}
              onClick={() => setArchived(false)}
            >
              已保存
            </button>
            <button
              className="subtle-action"
              aria-pressed={archived}
              onClick={() => setArchived(true)}
            >
              已归档
            </button>
          </div>
        )}
      </div>
      {tab === "tasks" ? (
        <>
          {visible.length ? (
            <div className="task-grid">
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
            <div className="empty-state">
              <HistoryIcon />
              <h3>
                {needle
                  ? "没有匹配的任务"
                  : archived
                    ? "还没有归档任务"
                    : "还没有快捷任务"}
              </h3>
              <p>
                {needle
                  ? "换个名称或清除筛选。"
                  : archived
                    ? "暂时不用的入口可以归档，随时恢复。"
                    : "在对话中告诉 AI：把刚才配置好的某一项任务加入快捷任务。也可以在这里让 AI 配置。"}
              </p>
              {needle ? (
                <button
                  className="secondary-action"
                  onClick={() => {
                    setQuery("");
                    setArchived(false);
                  }}
                >
                  清除筛选
                </button>
              ) : (
                !archived && (
                  <button
                    className="secondary-action"
                    onClick={() => configure()}
                  >
                    让 AI 配置任务
                  </button>
                )
              )}
            </div>
          )}
          {legacy.length > 0 && !needle && !archived && (
            <details className="tasks-legacy">
              <summary>旧版流程 · {legacy.length} 项</summary>
              <p>
                原有内容已保留。指定其中的一项任务，让 AI 改为可直接运行的入口。
              </p>
              {legacy.map((task) => (
                <div key={task.id}>
                  <span>{task.name}</span>
                  <button
                    className="subtle-action"
                    onClick={() => configure(task)}
                  >
                    让 AI 调整
                  </button>
                  <button
                    className="subtle-action"
                    onClick={() => setRemoving(task)}
                  >
                    删除
                  </button>
                </div>
              ))}
            </details>
          )}
        </>
      ) : runs.length ? (
        <div className="record-list">
          {runs.map((run) => {
            const task = tasks.find(
              (task) =>
                run.source?.kind === "savedWorkflow" &&
                task.id === run.source.workflowId,
            );
            return (
              <div className="record" key={run.id}>
                <div className="record-main">
                  <span className="record-title">
                    {task?.name ?? run.prompt}
                  </span>
                  <small>{new Date(run.createdAt).toLocaleString()}</small>
                  {run.error && (
                    <small className="shortcut-run-error">
                      {taskError(run.error)}
                    </small>
                  )}
                </div>
                <span className="tag">
                  {taskLabels[run.state] ?? run.state}
                </span>
                {isRunning(run) ? (
                  <button
                    className="subtle-action"
                    disabled={busy === run.id || run.state === "cancelling"}
                    onClick={() =>
                      void act(run.id, () => api.cancelTask(run.id))
                    }
                  >
                    {run.state === "cancelling" ? "正在停止" : "停止"}
                  </button>
                ) : (
                  task?.shortcut && (
                    <button
                      className="subtle-action"
                      onClick={() => onOpenTask?.(task)}
                    >
                      查看任务
                    </button>
                  )
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <div className="empty-state">
          <HistoryIcon />
          <h3>{needle ? "没有匹配的记录" : "还没有运行记录"}</h3>
          <p>快捷任务的运行结果会显示在这里。</p>
        </div>
      )}
      <ShortcutConfigurator
        bootstrap={bootstrap}
        open={configuring}
        target={target}
        onClose={() => setConfiguring(false)}
        reload={reload}
      />
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
    </div>
  );
}
