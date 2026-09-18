import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import { isRunning, needsConfirmation, readError, taskLabels } from "../../session";
import { Toast } from "../overlay/Toast";
import { ConfirmDialog } from "../overlay/ConfirmDialog";
import type {
  Bootstrap,
  TaskInfo,
  TaskSummary,
  WorkflowDetail,
} from "../../ipc/types";
import { CloseIcon, HistoryIcon } from "../icons";
import { TaskCard, type TaskActions } from "../tasks/TaskCard";
import { MotionSwitch } from "../controls/MotionSwitch";
import "./details-panel.css";

/**
 * 右侧详情栏：当前对话产生的快捷任务，或选中的任务与运行详情。
 *
 * 不是常驻必需 —— 窄窗口转成抽屉，不挤压聊天正文。
 */
export function DetailsPanel({
  bootstrap,
  conversationId,
  selectedTask,
  onSelectTask,
  onOpenConversation,
  onConnectTools,
  reload,
  onClose,
}: {
  bootstrap: Bootstrap;
  conversationId?: string | undefined;
  selectedTask?: string | undefined;
  onSelectTask(id: string | undefined): void;
  onOpenConversation(id: string): void;
  onConnectTools?(): void;
  reload(): Promise<void>;
  onClose(): void;
}) {
  const [detail, setDetail] = useState<WorkflowDetail>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [pendingRemove, setPendingRemove] = useState<TaskSummary | null>(null);
  const tasks = conversationId
    ? bootstrap.workflows.filter(
        (task) => task.sourceConversationId === conversationId,
      )
    : [];
  const runs = conversationId
    ? bootstrap.tasks.filter((run) => run.conversationId === conversationId)
    : [];

  useEffect(() => {
    if (!selectedTask) {
      setDetail(undefined);
      return;
    }
    let alive = true;
    void api
      .workflowGet(selectedTask)
      .then((value) => {
        if (alive) setDetail(value);
      })
      .catch((reason) => {
        if (alive) setError(readError(reason));
      });
    return () => {
      alive = false;
    };
  }, [selectedTask, bootstrap.workflows]);

  const act = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError("");
    try {
      await action();
      await reload();
      if (selectedTask) setDetail(await api.workflowGet(selectedTask));
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy("");
    }
  };

  const actions: TaskActions = {
    run: (task: TaskSummary) =>
      void act(task.id, async () => {
        const run = await api.runWorkflow(
          task.id,
          task.publishedRevision ?? undefined,
        );
        onOpenConversation(run.conversationId);
      }),
    askAi: (task) =>
      onOpenConversation(task.sourceConversationId ?? conversationId ?? ""),
    connect: () => onConnectTools?.(),
    open: (task) => onSelectTask(task.id),
    openSource: (task) => {
      if (task.sourceConversationId)
        onOpenConversation(task.sourceConversationId);
    },
    archive: (task, archived) =>
      void act(task.id, () => api.archiveWorkflow(task.id, archived)),
    pin: (task, pinned) =>
      void act(task.id, () => api.pinWorkflow(task.id, pinned)),
    copy: (task) => void act(task.id, () => api.copyWorkflow(task.id)),
    remove: (task) => {
      if (!needsConfirmation(bootstrap.permission.mode)) {
        void act(task.id, () => api.deleteWorkflow(task.id));
        return;
      }
      setPendingRemove(task);
    },
  };

  return (
    <aside className="details-panel" aria-label="详情">
      <header className="details-head">
        <h2>
          {selectedTask ? (detail?.summary.name ?? "任务详情") : "此对话的任务"}
        </h2>
        <button
          className="icon-button"
          aria-label="关闭详情"
          title="关闭详情"
          onClick={onClose}
        >
          <CloseIcon className="button-icon" />
        </button>
      </header>
      {error && <Toast message={error} onDismiss={() => setError("")} />}
      <div className="details-body">
        <MotionSwitch
          viewKey={`${conversationId ?? "none"}:${selectedTask ?? "list"}`}
          kind="panel"
        >
        {selectedTask ? (
          detail ? (
            <TaskDetail
              detail={detail}
              runs={runs}
              busy={busy}
              actions={actions}
              onBack={() => onSelectTask(undefined)}
              onOpenConversation={onOpenConversation}
            />
          ) : (
            <p className="muted">载入中…</p>
          )
        ) : tasks.length ? (
          <>
            {tasks.map((task) => (
              <TaskCard
                key={task.id}
                task={task}
                busy={busy === task.id}
                actions={actions}
              />
            ))}
            {runs.length > 0 && <RunList runs={runs} />}
          </>
        ) : (
          <div className="empty-state is-compact">
            <HistoryIcon />
            <h3>这个对话还没有快捷任务</h3>
            <p>说明你想反复做的那件事，Agent 会把它做成一键运行的任务。</p>
          </div>
        )}
        </MotionSwitch>
      </div>
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
    </aside>
  );
}

function TaskDetail({
  detail,
  runs,
  busy,
  actions,
  onBack,
  onOpenConversation,
}: {
  detail: WorkflowDetail;
  runs: TaskInfo[];
  busy: string;
  actions: TaskActions;
  onBack(): void;
  onOpenConversation(id: string): void;
}) {
  const { summary, revision } = detail;
  const related = runs.filter(
    (run) =>
      run.source?.kind === "savedWorkflow" &&
      run.source.workflowId === summary.id,
  );
  return (
    <>
      <button className="subtle-action" onClick={onBack}>
        返回此对话的任务
      </button>
      <section className="detail-block">
        <h3>用途</h3>
        <p>{summary.description || "还没有写说明。"}</p>
        <dl className="detail-facts">
          <div>
            <dt>状态</dt>
            <dd>{summary.stateLabel}</dd>
          </div>
          <div>
            <dt>依赖工具</dt>
            <dd>{summary.nodeCount} 个步骤</dd>
          </div>
          <div>
            <dt>运行是否调用模型</dt>
            <dd>
              {summary.zeroToken
                ? "否"
                : summary.modelUsage === "possible"
                  ? "可能调用"
                  : "无法确认"}
            </dd>
          </div>
          <div>
            <dt>来源对话</dt>
            <dd>
              {summary.sourceDeleted ? (
                "来源对话已删除，任务仍可使用"
              ) : summary.sourceConversationId ? (
                <button
                  className="subtle-action"
                  onClick={() =>
                    onOpenConversation(summary.sourceConversationId as string)
                  }
                >
                  打开来源对话
                </button>
              ) : (
                "由运行提取"
              )}
            </dd>
          </div>
        </dl>
        {summary.issue && <p className="muted">{summary.issue}</p>}
        <div className="detail-actions">
          {summary.runnable ? (
            <button
              className="primary-action"
              disabled={busy === summary.id}
              onClick={() => actions.run(summary)}
            >
              运行
            </button>
          ) : (
            <button
              className="secondary-action"
              disabled={busy === summary.id}
              onClick={() =>
                summary.state === "unavailable"
                  ? actions.connect?.(summary)
                  : actions.askAi?.(summary)
              }
            >
              {summary.actionLabel}
            </button>
          )}
          {actions.remove ? (
            <button
              className="secondary-action"
              disabled={busy === summary.id}
              onClick={() => actions.remove?.(summary)}
            >
              删除任务
            </button>
          ) : null}
        </div>
      </section>
      <section className="detail-block">
        <h3>版本</h3>
        <p className="muted">
          {summary.publishedRevision
            ? `当前发布第 ${summary.publishedRevision} 版`
            : "还没有发布版本"}
          {revision ? ` · 共 ${revision.validation.nodeCount} 个节点` : ""}
        </p>
        {revision && revision.validation.issues.length > 0 && (
          <details>
            <summary>
              待解决的问题 · {revision.validation.issues.length} 项
            </summary>
            <ul>
              {revision.validation.issues.map((issue) => (
                <li key={`${issue.nodeId}-${issue.message}`}>
                  {issue.nodeId ? `${issue.nodeId}：` : ""}
                  {issue.message}
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>
      <section className="detail-block">
        <h3>最近运行</h3>
        {related.length ? (
          <RunList runs={related} />
        ) : (
          <p className="muted">还没有运行过。</p>
        )}
      </section>
    </>
  );
}

function RunList({ runs }: { runs: TaskInfo[] }) {
  return (
    <ul className="detail-runs">
      {runs.slice(0, 10).map((run) => (
        <li key={run.id}>
          <span>{taskLabels[run.state] ?? run.state}</span>
          <small>
            {new Date(run.createdAt).toLocaleString("zh-CN", {
              month: "2-digit",
              day: "2-digit",
              hour: "2-digit",
              minute: "2-digit",
            })}
          </small>
          {isRunning(run) && <em>进行中</em>}
        </li>
      ))}
    </ul>
  );
}
