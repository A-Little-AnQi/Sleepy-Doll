import { SearchField, EmptyState } from "@sleepy-doll/ui";
import { Button, Badge } from "@sleepy-doll/ui";
import { useMemo, useRef, useState } from "react";
import { api } from "../../ipc/api";
import { HistoryIcon } from "@sleepy-doll/ui/icons";
import { TaskCard, type TaskActions } from "../../components/tasks/TaskCard";
import { taskRun, taskError } from "../../components/tasks/task-display";
import { isRunning, readError, taskLabels } from "../../session";
import { Toast } from "@sleepy-doll/ui/overlay/Toast";
import { ConfirmDialog } from "@sleepy-doll/ui/overlay/ConfirmDialog";
import { SlidingTabs } from "@sleepy-doll/ui/controls/SlidingTabs";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import "./TasksPage.css";

export function TasksPage({
  bootstrap,
  reload,
  onOpenTask,
  onConnectTools,
  onSchedule,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onOpenConversation(id: string): void;
  onOpenTask?: (task: TaskSummary) => void;
  onConnectTools?: () => void;
  onSchedule(task: TaskSummary): void;
}) {
  const [tab, setTab] = useState<"tasks" | "runs">("tasks");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<Set<string>>(() => new Set());
  const pending = useRef(new Set<string>());
  const [error, setError] = useState("");
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
            `${task.name} ${task.description} ${task.shortcut.applicationName} ${task.shortcut.targetName}`
              .toLowerCase()
              .includes(needle),
        )
        .sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) || a.name.localeCompare(b.name),
        ),
    [bootstrap.workflows, needle],
  );
  const legacy = tasks.filter((task) => !task.shortcut);
  const runs = bootstrap.tasks.filter(
    (run) =>
      (run.source?.kind === "savedWorkflow" ||
        run.source?.kind === "boundWorkflow") &&
      run.prompt.toLowerCase().includes(needle),
  );
  const act = async (id: string, action: () => Promise<unknown>) => {
    if (pending.current.has(id)) return;
    pending.current.add(id);
    setBusy((current) => new Set(current).add(id));
    setError("");
    try {
      await action();
      await reload();
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      pending.current.delete(id);
      setBusy((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };
  const actions: TaskActions = {
    schedule: onSchedule,
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
    remove: (task) => setRemoving(task),
    connect: () => onConnectTools?.(),
    open: (task) => onOpenTask?.(task),
  };
  return (
    <div className="page-sheet tasks-page">
      <div className="page-title">
        <div>
          <h2>快捷任务</h2>
          <p className="tasks-intro">
            在对话里输入「创建快捷任务」并描述目标，保存后在这里一键运行
          </p>
        </div>
      </div>
      <div className="list-toolbar page-list-toolbar tasks-tabs-toolbar">
        <SlidingTabs
          ariaLabel="快捷任务"
          value={tab}
          onChange={setTab}
          items={[
            { id: "tasks", name: "已保存任务" },
            { id: "runs", name: "运行记录" },
          ]}
        />
        <SearchField
          aria-label={tab === "tasks" ? "搜索快捷任务" : "搜索运行记录"}
          placeholder="搜索任务名称或应用"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      {tab === "tasks" ? (
        <>
          {visible.length ? (
            <div className="task-grid">
              {visible.map((task) => (
                <TaskCard
                  key={task.id}
                  task={task}
                  activeRun={taskRun(task, bootstrap.tasks)}
                  busy={busy.has(task.id)}
                  actions={actions}
                />
              ))}
            </div>
          ) : (
            <EmptyState
              icon={<HistoryIcon />}
              title={<>{needle ? "没有匹配的任务" : "还没有快捷任务"}</>}
              description={
                <>
                  {needle
                    ? "换个名称或清除筛选。"
                    : "在对话里输入「创建快捷任务」，描述要加入的已有项，也可以点输入框左下角的 + 选择它。"}
                </>
              }
            >
              {needle && (
                <Button
                  variant="secondary"
                  onClick={() => {
                    setQuery("");
                  }}
                >
                  清除筛选
                </Button>
              )}
            </EmptyState>
          )}
          {legacy.length > 0 && !needle && (
            <details className="tasks-legacy">
              <summary>旧版流程 · {legacy.length} 项</summary>
              <p>
                原有内容已保留。在对话里说要转换哪一项，让 AI
                把它改成可直接运行的入口。
              </p>
              {legacy.map((task) => (
                <div key={task.id}>
                  <span>{task.name}</span>
                  <Button variant="subtle" onClick={() => setRemoving(task)}>
                    删除
                  </Button>
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
                (run.source?.kind === "savedWorkflow" ||
                  run.source?.kind === "boundWorkflow") &&
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
                <Badge>{taskLabels[run.state] ?? run.state}</Badge>
                {isRunning(run) ? (
                  <Button
                    variant="subtle"
                    disabled={busy.has(run.id) || run.state === "cancelling"}
                    onClick={() =>
                      void act(run.id, () => api.cancelTask(run.id))
                    }
                  >
                    {run.state === "cancelling" ? "正在停止" : "停止"}
                  </Button>
                ) : (
                  task?.shortcut && (
                    <Button variant="subtle" onClick={() => onOpenTask?.(task)}>
                      查看任务
                    </Button>
                  )
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <EmptyState
          icon={<HistoryIcon />}
          title={<>{needle ? "没有匹配的记录" : "还没有运行记录"}</>}
          description={<>快捷任务的运行结果会显示在这里。</>}
        ></EmptyState>
      )}
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
        <p>永久删除「{removing?.name}」及其全部修订？</p>
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
