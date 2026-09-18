import { useEffect, useMemo, useState } from "react";
import { api } from "../../ipc/api";
import { HistoryIcon, SearchIcon } from "../../components/icons";
import { TaskCard, type TaskActions } from "../../components/tasks/TaskCard";
import {
  isRunning,
  needsConfirmation,
  readError,
  taskLabels,
} from "../../session";
import { Toast } from "../../components/overlay/Toast";
import { ConfirmDialog } from "../../components/overlay/ConfirmDialog";
import type { Bootstrap, TaskInfo, TaskSummary } from "../../ipc/types";
import { MotionSwitch } from "../../components/controls/MotionSwitch";
import { SlidingTabs } from "../../components/controls/SlidingTabs";
import "./TasksPage.css";

type Filter = "all" | "runnable" | "attention" | "archived";

const FILTERS: Array<{ id: Filter; label: string }> = [
  { id: "all", label: "全部" },
  { id: "runnable", label: "可运行" },
  { id: "attention", label: "需处理" },
  { id: "archived", label: "已归档" },
];

function matches(task: TaskSummary, filter: Filter) {
  switch (filter) {
    case "runnable":
      return task.runnable;
    case "attention":
      return ["draft", "invalid", "unavailable"].includes(task.state);
    case "archived":
      return task.state === "archived";
    default:
      return task.state !== "archived";
  }
}

/**
 * 快捷任务页：默认「快捷任务」，第二标签「运行记录」。
 *
 * 任务是可以复用的小工具，运行记录是它跑过的每一次 —— 两者不能混成一个列表，
 * 否则「运行」和「做过的某件事」会看起来像同一类东西。
 */
export function TasksPage({
  bootstrap,
  reload,
  onOpenConversation,
  onOpenTask,
  onConnectTools,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onOpenConversation(id: string): void;
  onOpenTask?: ((task: TaskSummary) => void) | undefined;
  onConnectTools?: (() => void) | undefined;
}) {
  const [tab, setTab] = useState<"tasks" | "runs">("tasks");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [tasks, setTasks] = useState<TaskSummary[]>(bootstrap.workflows);
  const [pendingRemove, setPendingRemove] = useState<TaskSummary | null>(
    null,
  );

  useEffect(() => {
    setTasks(bootstrap.workflows);
  }, [bootstrap.workflows]);

  const act = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError("");
    try {
      await action();
      await reload();
      setTasks(await api.workflowList());
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy("");
    }
  };

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return tasks.filter(
      (task) =>
        matches(task, filter) &&
        (!needle ||
          task.name.toLowerCase().includes(needle) ||
          task.description.toLowerCase().includes(needle)),
    );
  }, [tasks, filter, query]);

  const actions: TaskActions = {
    run: (task) =>
      void act(task.id, async () => {
        const run = await api.runWorkflow(
          task.id,
          task.publishedRevision ?? undefined,
        );
        onOpenConversation(run.conversationId);
      }),
    rename: (task) => {
      const name = window.prompt("给这个快捷任务换个名字", task.name);
      if (name === null) return;
      void act(task.id, () => api.renameWorkflow(task.id, name));
    },
    pin: (task, pinned) =>
      void act(task.id, () => api.pinWorkflow(task.id, pinned)),
    archive: (task, archived) =>
      void act(task.id, () => api.archiveWorkflow(task.id, archived)),
    copy: (task) => void act(task.id, () => api.copyWorkflow(task.id)),
    remove: (task) => {
      if (!needsConfirmation(bootstrap.permission.mode)) {
        void act(task.id, () => api.deleteWorkflow(task.id));
        return;
      }
      setPendingRemove(task);
    },
    askAi: (task) => {
      onOpenConversation(task.sourceConversationId ?? "");
      setError(
        `请在这个对话里说明要改什么；「${task.name}」会生成新版本，旧版本继续可用。`,
      );
    },
    connect: () => onConnectTools?.(),
    open: (task) => onOpenTask?.(task),
    openSource: (task) => {
      if (task.sourceConversationId)
        onOpenConversation(task.sourceConversationId);
    },
  };

  const runs = bootstrap.tasks.filter(
    (run) =>
      !query.trim() ||
      run.prompt.toLowerCase().includes(query.trim().toLowerCase()),
  );

  return (
    <div className="page-sheet tasks-page">
      <div className="page-title">
        <h2>快捷任务</h2>
        <span className="muted">
          {bootstrap.tasks.filter(isRunning).length} 项正在运行
        </span>
      </div>
      <div className="list-toolbar">
        <SlidingTabs
          ariaLabel="快捷任务与运行记录"
          value={tab}
          onChange={setTab}
          items={[
            { id: "tasks", name: "快捷任务" },
            { id: "runs", name: "运行记录" },
          ]}
        />
      </div>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="list-toolbar">
        <label className="search-field">
          <SearchIcon />
          <input
            aria-label={tab === "tasks" ? "搜索快捷任务" : "搜索运行记录"}
            placeholder={tab === "tasks" ? "搜索快捷任务" : "搜索运行记录"}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        {tab === "tasks" && (
          <div className="filter-row">
            {FILTERS.map((item) => (
              <button
                key={item.id}
                className="subtle-action"
                aria-pressed={filter === item.id}
                onClick={() => setFilter(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <MotionSwitch viewKey={tab} kind="panel">
      {tab === "tasks" ? (
        visible.length ? (
          <div className="task-grid">
            {visible.map((task) => (
              <TaskCard
                key={task.id}
                task={task}
                busy={busy === task.id}
                actions={actions}
                showSource
              />
            ))}
          </div>
        ) : (
          <div className="empty-state">
            <HistoryIcon />
            <h3>{query ? "没有找到匹配任务" : "还没有快捷任务"}</h3>
            <p>
              {query
                ? "换个词试试，或者清掉筛选条件。"
                : "在对话里说明你想反复做的那件事，Agent 会把它做成一键运行的任务。"}
            </p>
            {query ? (
              <button
                className="secondary-action"
                onClick={() => {
                  setQuery("");
                  setFilter("all");
                }}
              >
                清除筛选
              </button>
            ) : (
              <button
                className="secondary-action"
                onClick={() => onOpenConversation("")}
              >
                通过对话创建
              </button>
            )}
          </div>
        )
      ) : runs.length ? (
        <div className="record-list">
          {runs.map((run) => (
            <RunRow
              key={run.id}
              run={run}
              busy={busy === run.id}
              onOpen={() => onOpenConversation(run.conversationId)}
              onStop={() => void act(run.id, () => api.cancelTask(run.id))}
            />
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <HistoryIcon />
          <h3>{query ? "没有找到匹配的运行记录" : "还没有运行记录"}</h3>
          <p>
            {query ? "换个词试试。" : "聊天和快捷任务执行都会在这里留下痕迹。"}
          </p>
        </div>
      )}
      </MotionSwitch>
      <ConfirmDialog
        open={pendingRemove != null}
        title="删除快捷任务"
        confirmLabel="删除任务"
        onClose={() => setPendingRemove(null)}
        onConfirm={() => {
          const task = pendingRemove;
          setPendingRemove(null);
          if (task) void act(task.id, () => api.deleteWorkflow(task.id));
        }}
      >
        {pendingRemove ? (
          <>
            <p>删除「{pendingRemove.name}」。</p>
            <p>已经跑过的运行记录和它改过的文件都会保留。</p>
            {pendingRemove.runnable ? (
              <p>如果有运行正在进行，那一次会继续跑完。</p>
            ) : null}
          </>
        ) : null}
      </ConfirmDialog>
    </div>
  );
}

function RunRow({
  run,
  busy,
  onOpen,
  onStop,
}: {
  run: TaskInfo;
  busy: boolean;
  onOpen(): void;
  onStop(): void;
}) {
  return (
    <div className="record">
      <div className="record-main">
        <button className="record-title" onClick={onOpen}>
          {run.prompt}
        </button>
        <small>
          {new Date(run.createdAt).toLocaleString("zh-CN", {
            month: "2-digit",
            day: "2-digit",
            hour: "2-digit",
            minute: "2-digit",
          })}
          {run.source?.kind === "savedWorkflow" && " · 快捷任务"}
        </small>
        {run.error && <small>{run.error}</small>}
      </div>
      <span className="tag">{taskLabels[run.state] ?? run.state}</span>
      {isRunning(run) ? (
        <button className="subtle-action" disabled={busy} onClick={onStop}>
          停止
        </button>
      ) : (
        <button className="subtle-action" onClick={onOpen}>
          打开
        </button>
      )}
    </div>
  );
}
