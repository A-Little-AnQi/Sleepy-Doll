import { useEffect, useRef, useState } from "react";
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
import { ChevronIcon, CloseIcon, HistoryIcon } from "../icons";
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
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const [drawer, setDrawer] = useState(false);

  useEffect(() => {
    const panel = panelRef.current;
    if (!panel?.closest(".details-drawer")) return;
    setDrawer(true);
    const previous = document.activeElement as HTMLElement | null;
    const controls = () =>
      Array.from(
        panel.querySelectorAll<HTMLElement>(
          'button:not(:disabled), a[href], input:not(:disabled), summary, [tabindex="0"]',
        ),
      ).filter((element) => !element.closest('[hidden], [aria-hidden="true"]'));
    controls()[0]?.focus();
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        // An open task menu handles Escape first.
        if (panel.querySelector('[aria-expanded="true"]')) return;
        event.preventDefault();
        event.stopPropagation();
        closeRef.current();
      }
      if (event.key !== "Tab") return;
      const items = controls();
      const first = items[0];
      const last = items.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    panel.addEventListener("keydown", handleKey);
    return () => {
      panel.removeEventListener("keydown", handleKey);
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  useEffect(() => {
    // Navigation replaces the task button; keep keyboard focus inside the drawer.
    if (drawer)
      panelRef.current
        ?.querySelector<HTMLButtonElement>(".details-head button")
        ?.focus();
  }, [drawer, selectedTask]);
  // Bootstrap also updates during streaming. Only refresh detail when this
  // workflow changes, and never show the previous workflow while loading another.
  const selectedVersion = JSON.stringify(
    bootstrap.workflows.find((task) => task.id === selectedTask),
  );
  const currentDetail =
    detail?.summary.id === selectedTask ? detail : undefined;
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
    setError("");
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
  }, [selectedTask, selectedVersion]);

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
    <aside
      ref={panelRef}
      className="details-panel"
      aria-label={t.details.panel}
      role={drawer ? "dialog" : undefined}
      aria-modal={drawer ? true : undefined}
    >
      <header className="details-head">
        {selectedTask ? (
          <button
            className="icon-button details-back"
            aria-label={t.details.backToTasks}
            title={t.details.backToTasks}
            onClick={() => onSelectTask(undefined)}
          >
            <ChevronIcon className="button-icon" />
          </button>
        ) : (
          <span className="details-heading-icon" aria-hidden="true">
            <HistoryIcon />
          </span>
        )}
        <h2
          title={selectedTask ? currentDetail?.summary.name : t.chat.tasksPanel}
        >
          {selectedTask
            ? (currentDetail?.summary.name ?? t.details.taskDetail)
            : t.chat.tasksPanel}
        </h2>
        {!selectedTask && tasks.length > 0 && (
          <span className="details-count">{tasks.length}</span>
        )}
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
          className="details-content"
        >
          {selectedTask ? (
            currentDetail ? (
              <TaskDetail
                detail={currentDetail}
                runs={runs}
                busy={busy}
                actions={actions}
                onOpenConversation={onOpenConversation}
              />
            ) : (
              <div className="details-loading" role="status" aria-busy={!error}>
                {!error && (
                  <>
                    <span />
                    <span />
                    <span />
                  </>
                )}
                <p>{error || t.common.loading}</p>
              </div>
            )
          ) : tasks.length || runs.length ? (
            <>
              {tasks.length > 0 && (
                <section
                  className="details-section"
                  aria-label={t.tasks.tabTasks}
                >
                  <h3 className="details-section-title">{t.tasks.tabTasks}</h3>
                  {tasks.map((task) => (
                    <TaskCard
                      key={task.id}
                      task={task}
                      busy={busy === task.id}
                      actions={actions}
                    />
                  ))}
                </section>
              )}
              {runs.length > 0 && (
                <section
                  className="details-section"
                  aria-label={t.details.recentRuns}
                >
                  <h3 className="details-section-title">
                    {t.details.recentRuns}
                    <span>{runs.length}</span>
                  </h3>
                  <RunList runs={runs} />
                </section>
              )}
            </>
          ) : (
            <div className="details-empty">
              <span className="details-empty-icon" aria-hidden="true">
                <HistoryIcon />
              </span>
              <h3>{t.tasks.empty}</h3>
              <p>{t.tasks.emptyHow}</p>
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
            {pendingRemove.runnable ? <p>{t.tasks.deleteKeepsActive}</p> : null}
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
  onOpenConversation,
}: {
  detail: WorkflowDetail;
  runs: TaskInfo[];
  busy: string;
  actions: TaskActions;
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
        <h3>{t.details.version}</h3>
        <p className="muted">
          {summary.publishedRevision
            ? t.details.publishedRevision(summary.publishedRevision)
            : t.details.noRelease}
          {revision
            ? ` · ${t.details.nodeCount(revision.validation.nodeCount)}`
            : ""}
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
        <h3>{t.details.recentRuns}</h3>
        {related.length ? (
          <RunList runs={related} />
        ) : (
          <p className="muted">{t.details.noRuns}</p>
        )}
      </section>
    </>
  );
}

function RunList({ runs }: { runs: TaskInfo[] }) {
  const t = useT();
  return (
    <ul className="detail-runs">
      {[...runs]
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
        .slice(0, 10)
        .map((run) => (
          <li key={run.id} data-running={isRunning(run)}>
            <div className="detail-run-meta">
              <span className="detail-run-state">
                <i aria-hidden="true" />
                {taskLabels[run.state] ?? run.state}
              </span>
              <time
                dateTime={run.createdAt}
                title={new Date(run.createdAt).toLocaleString()}
              >
                {new Date(run.createdAt).toLocaleString("zh-CN", {
                  month: "2-digit",
                  day: "2-digit",
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </time>
            </div>
            <p className="detail-run-prompt" title={run.prompt}>
              {run.prompt || t.details.noDescriptionYet}
            </p>
          </li>
        ))}
    </ul>
  );
}
