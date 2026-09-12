import { useState } from "react";
import { api } from "../api";
import { HistoryIcon, SearchIcon } from "../components/icons";
import { isRunning, taskLabels } from "../session";
import type { Bootstrap } from "../types";
import "./RunLibraryPage.css";
export function RunLibraryPage({
  bootstrap,
  reload,
  onRun,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onRun(id: string): void;
}) {
  const [tab, setTab] = useState("tasks");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const act = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError("");
    try {
      await action();
      await reload();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  };
  const tasks = bootstrap.tasks.filter(
    (task) =>
      (filter !== "active" || isRunning(task)) &&
      task.prompt.toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <div className="page-sheet run-library-page">
      <div className="page-title">
        <h2>运行记录</h2>
        <span className="muted">
          {bootstrap.tasks.filter(isRunning).length} 项运行中
        </span>
      </div>
      <div className="list-toolbar">
        <div className="segmented" role="tablist" aria-label="任务分类">
          {[
            { id: "tasks", name: "全部任务" },
            { id: "flows", name: "已存流程" },
            { id: "operations", name: "操作" },
            { id: "resources", name: "资源" },
            { id: "diagnostics", name: "诊断" },
          ].map((item) => (
            <button
              key={item.id}
              role="tab"
              aria-selected={tab === item.id}
              className={tab === item.id ? "is-active" : ""}
              onClick={() => setTab(item.id)}
            >
              {item.name}
            </button>
          ))}
        </div>
      </div>
      {error && (
        <div className="inline-error" role="alert">
          {error}
        </div>
      )}
      {tab === "tasks" ? (
        <>
          <div className="list-toolbar">
            <label className="search-field">
              <SearchIcon />
              <input
                aria-label="搜索任务"
                placeholder="搜索任务"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <button
              className="subtle-action"
              aria-pressed={filter === "active"}
              onClick={() => setFilter(filter === "active" ? "all" : "active")}
            >
              {filter === "active" ? "显示全部" : "仅运行中"}
            </button>
          </div>
          {tasks.length ? (
            <div className="record-list">
              {tasks.map((task) => (
                <div className="record" key={task.id}>
                  <div className="record-main">
                    <button
                      className="record-title"
                      onClick={() => onRun(task.conversationId)}
                    >
                      {task.prompt}
                    </button>
                    <small>
                      {new Date(task.createdAt).toLocaleString("zh-CN", {
                        month: "2-digit",
                        day: "2-digit",
                        hour: "2-digit",
                        minute: "2-digit",
                      })}
                    </small>
                    {task.error && <small>{task.error}</small>}
                  </div>
                  <span className="tag">
                    {taskLabels[task.state] ?? task.state}
                  </span>
                  {isRunning(task) ? (
                    <button
                      className="subtle-action"
                      disabled={busy === task.id}
                      onClick={() =>
                        void act(task.id, () => api.cancelTask(task.id))
                      }
                    >
                      停止
                    </button>
                  ) : (
                    <button
                      className="subtle-action"
                      onClick={() => onRun(task.conversationId)}
                    >
                      打开
                    </button>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state">
              <HistoryIcon />
              <h3>{query ? "无匹配任务" : "暂无任务"}</h3>
            </div>
          )}
        </>
      ) : tab === "flows" ? (
        <>
          <div className="flow-grid">
            {[
              ...bootstrap.workflows.map((flow) => ({
                ...flow,
                legacy: false,
              })),
              ...bootstrap.strategies.map((flow) => ({
                ...flow,
                steps: flow.plan.steps,
                legacy: true,
              })),
            ].map((flow) => (
              <article className="flow-card" key={flow.id}>
                <strong>{flow.name}</strong>
                <ol>
                  {flow.steps.map((step) => (
                    <li key={step.id}>{step.title}</li>
                  ))}
                </ol>
                <div>
                  <button
                    className="secondary-action"
                    disabled={busy === flow.id}
                    onClick={() =>
                      void act(flow.id, async () => {
                        const task = await (flow.legacy
                          ? api.runStrategy(flow.id)
                          : api.runWorkflow(flow.id));
                        onRun(task.conversationId);
                      })
                    }
                  >
                    运行
                  </button>
                </div>
              </article>
            ))}
          </div>
          {!bootstrap.workflows.length && !bootstrap.strategies.length && (
            <div className="empty-state">
              <HistoryIcon />
              <h3>暂无已存流程</h3>
            </div>
          )}
        </>
      ) : tab === "operations" ? (
        <div className="record-list">
          {bootstrap.operations.map((operation) => (
            <div className="record" key={operation.id}>
              <div className="record-main">
                <strong>{operation.title}</strong>
                <small>{operation.error || operation.providerId}</small>
                <small>
                  {(
                    {
                      observe: "只读",
                      low: "低风险",
                      standard: "修改状态",
                      high: "高风险",
                      irreversible: "不可撤销",
                    } as Record<string, string>
                  )[operation.risk] ?? operation.risk}
                </small>
              </div>
              <span className="tag">
                {taskLabels[operation.state] ?? operation.state}
              </span>
              <button
                className="subtle-action"
                disabled={
                  busy === operation.id ||
                  operation.state !== "awaitingAuthorization"
                }
                onClick={() =>
                  void act(operation.id, () =>
                    api.executeOperation(operation.id),
                  )
                }
              >
                执行
              </button>
              <button
                className="subtle-action"
                disabled={
                  busy === operation.id || operation.state !== "needsReview"
                }
                onClick={() =>
                  void act(operation.id, () =>
                    api.rollbackOperation(operation.id),
                  )
                }
              >
                撤销
              </button>
            </div>
          ))}
          {!bootstrap.operations.length && (
            <div className="empty-state">
              <h3>暂无操作</h3>
            </div>
          )}
        </div>
      ) : tab === "resources" ? (
        <div className="record-list">
          {bootstrap.resources.map((resource) => (
            <div className="record" key={resource.id}>
              <div className="record-main">
                <strong>{resource.displayName}</strong>
                <small>{resource.providerId}</small>
              </div>
              <span className="tag">{resource.kind}</span>
            </div>
          ))}
          {!bootstrap.resources.length && (
            <div className="empty-state">
              <h3>暂无资源</h3>
            </div>
          )}
        </div>
      ) : (
        <div className="record-list">
          {bootstrap.diagnostics.map((item) => (
            <div className="record" key={item.id}>
              <div className="record-main">
                <strong>{item.title}</strong>
                <small>{item.summary}</small>
              </div>
            </div>
          ))}
          {!bootstrap.diagnostics.length && (
            <div className="empty-state">
              <h3>暂无诊断记录</h3>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
