import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import {
  isRunning,
  needsConfirmation,
  readError,
  taskLabels,
} from "../../session";
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
import { useT } from "../../i18n";

/**
 * 右侧详情栏：当前对话产生的快捷任务，或选中的任务与运行详情。
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
  const t = useT();
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
    <aside className="details-panel" aria-label={t.details.panel}>
      <header className="details-head">
        <h2>
          {selectedTask ? (detail?.summary.name ?? t.details.taskDetail) : t.chat.tasksPanel}
        </h2>
        <button
          className="icon-button"
          aria-label={t.nav.closeDetails}
          title={t.nav.closeDetails}
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
              <p className="muted">{t.common.loading}</p>
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
              <h3>{t.tasks.empty}</h3>
              <p>{t.tasks.emptyHint}</p>
            </div>
          )}
        </MotionSwitch>
      </div>
      <ConfirmDialog
        open={pendingRemove != null}
        title={t.tasks.deleteTitle}
        confirmLabel={t.tasks.deleteAction}
        onClose={() => setPendingRemove(null)}
        onConfirm={() => {
          const task = pendingRemove;
          setPendingRemove(null);
          if (task) void act(task.id, () => api.deleteWorkflow(task.id));
        }}
      >
        {pendingRemove ? (
          <>
            <p>{t.tasks.deleteNote(pendingRemove.name)}</p>
            <p>{t.tasks.deleteKeepsRuns}</p>
            {pendingRemove.runnable ? (
              <p>{t.tasks.deleteKeepsActive}</p>
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
  const t = useT();
  const { summary, revision } = detail;
  const related = runs.filter(
    (run) =>
      run.source?.kind === "savedWorkflow" &&
      run.source.workflowId === summary.id,
  );
  return (
    <>
      <button className="subtle-action" onClick={onBack}>
        {t.details.backToTasks}
      </button>
      <section className="detail-block">
        <h3>{t.apiExplorer.purpose}</h3>
        <p>{summary.description || t.details.noDescriptionYet}</p>
        <dl className="detail-facts">
          <div>
            <dt>{t.details.statusLabel}</dt>
            <dd>{summary.stateLabel}</dd>
          </div>
          <div>
            <dt>{t.details.dependsTools}</dt>
            <dd>{t.details.stepsCount(summary.nodeCount)}</dd>
          </div>
          <div>
            <dt>{t.details.callsModel}</dt>
            <dd>
              {summary.zeroToken
                ? t.bridge.no
                : summary.modelUsage === "possible"
                  ? t.details.maybeCalls
                  : t.details.cannotConfirm}
            </dd>
          </div>
          <div>
            <dt>{t.details.sourceChat}</dt>
            <dd>
              {summary.sourceDeleted ? (
                t.details.sourceDeletedNote
              ) : summary.sourceConversationId ? (
                <button
                  className="subtle-action"
                  onClick={() =>
                    onOpenConversation(summary.sourceConversationId as string)
                  }
                >
                  {t.details.openSourceChat}
                </button>
              ) : (
                t.details.extractedFromRun
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
