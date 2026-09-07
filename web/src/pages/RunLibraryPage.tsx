import { useState } from "react";

import { api } from "../api";
import type { Bootstrap } from "../types";

interface Props {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onRun(conversationId: string): void;
}

const stateLabels: Record<string, string> = {
  draft: "等待校验",
  validating: "正在校验",
  awaitingAuthorization: "等待授权",
  preparing: "正在准备",
  prepared: "准备完成",
  committing: "正在提交",
  verifying: "正在验证",
  succeeded: "已验证",
  rollingBack: "正在回退",
  rolledBack: "已回退",
  recovering: "正在恢复",
  needsReview: "需要核对",
  failed: "失败",
  cancelled: "已取消",
};

export function RunLibraryPage({ bootstrap, reload, onRun }: Props) {
  const [running, setRunning] = useState<string>();
  const [error, setError] = useState("");
  const [executingOperation, setExecutingOperation] = useState<string>();

  const launch = async (id: string, legacy: boolean) => {
    setRunning(id);
    setError("");
    try {
      const run = legacy ? await api.runStrategy(id) : await api.runWorkflow(id);
      await reload();
      onRun(run.conversationId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRunning(undefined);
    }
  };

  const flows = [
    ...bootstrap.workflows.map((workflow) => ({
      id: workflow.id,
      name: workflow.name,
      description: workflow.description || "由已验证运行生成",
      steps: workflow.steps,
      legacy: false,
      unattended: workflow.unattended === "allowed",
    })),
    ...bootstrap.strategies.map((strategy) => ({
      id: strategy.id,
      name: strategy.name,
      description: "兼容的已验证流程",
      steps: strategy.plan.steps,
      legacy: true,
      unattended: false,
    })),
  ];

  return (
    <section className="run-library-workspace">
      <header className="library-heading">
        <span>AGENT CORE</span>
        <h1>流程与内核</h1>
        <p>领域能力来自 Plugin；内核负责可靠执行、验证、恢复和审计。</p>
      </header>

      <div className="library-grid kernel-grid">
        <section className="library-panel workflow-panel">
          <header>
            <h2>已验证流程</h2>
            <p>再次运行不请求模型，也不消耗对话 Token。</p>
          </header>
          {error ? <div className="inline-error" role="alert">{error}</div> : null}
          {flows.length ? (
            <div className="library-list">
              {flows.map((flow) => (
                <article className="library-card" key={`${flow.legacy}-${flow.id}`}>
                  <div>
                    <strong>{flow.name}</strong>
                    <small>{flow.steps.length} 个步骤 · {flow.description}</small>
                  </div>
                  <ol>
                    {flow.steps.map((step) => <li key={step.id}>{step.title}</li>)}
                  </ol>
                  <div className="card-actions">
                    <button
                      className="primary-library-action"
                      disabled={Boolean(running)}
                      onClick={() => void launch(flow.id, flow.legacy)}
                    >
                      {running === flow.id ? "正在加入队列" : "立即运行"}
                    </button>
                    <small>{flow.unattended ? "允许无人值守" : "仅手动运行"}</small>
                  </div>
                </article>
              ))}
            </div>
          ) : (
            <div className="library-empty">完成并验证一次领域流程后，可由前端保存到这里。</div>
          )}
        </section>

        <div className="kernel-column">
          <section className="library-panel">
            <header><h2>Plugin 资源</h2></header>
            {bootstrap.resources.length ? (
              <div className="compact-records">
                {bootstrap.resources.map((resource) => (
                  <article key={resource.id}>
                    <span className="record-mark" />
                    <div>
                      <strong>{resource.displayName}</strong>
                      <small>{resource.providerId} · {resource.kind} · v{resource.version}</small>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="library-empty">尚无领域 Plugin 提供资源。Core 不猜测文件和格式。</div>
            )}
          </section>

          <section className="library-panel">
            <header><h2>最近操作</h2></header>
            {bootstrap.operations.length ? (
              <div className="compact-records">
                {bootstrap.operations.slice(0, 8).map((operation) => (
                  <article key={operation.id}>
                    <span className={`record-mark ${operation.state}`} />
                    <div>
                      <strong>{operation.title}</strong>
                      <small>
                        {stateLabels[operation.state] ?? "状态未知"} · {operation.providerId}
                        {operation.error ? ` · ${operation.error}` : ""}
                      </small>
                    </div>
                    {operation.state === "awaitingAuthorization" ? (
                      <button
                        className="text-action record-action"
                        disabled={Boolean(executingOperation)}
                        onClick={() => {
                          setExecutingOperation(operation.id);
                          setError("");
                          void api
                            .executeOperation(operation.id)
                            .then(reload)
                            .catch((reason) => setError(String(reason)))
                            .finally(() => setExecutingOperation(undefined));
                        }}
                      >
                        {executingOperation === operation.id
                          ? "正在执行"
                          : "审核并执行"}
                      </button>
                    ) : null}
                    {operation.state === "needsReview" ? (
                      <button
                        className="text-action record-action"
                        disabled={Boolean(executingOperation)}
                        onClick={() => {
                          setExecutingOperation(operation.id);
                          setError("");
                          void api
                            .rollbackOperation(operation.id)
                            .then(reload)
                            .catch((reason) => setError(String(reason)))
                            .finally(() => setExecutingOperation(undefined));
                        }}
                      >
                        {executingOperation === operation.id ? "正在回退" : "安全回退"}
                      </button>
                    ) : null}
                  </article>
                ))}
              </div>
            ) : (
              <div className="library-empty">还没有通过事务内核执行的操作。</div>
            )}
          </section>

          <section className="library-panel">
            <header><h2>诊断结论</h2></header>
            {bootstrap.diagnostics.length ? (
              <div className="compact-records">
                {bootstrap.diagnostics.slice(0, 6).map((finding) => (
                  <article key={finding.id}>
                    <span className={`record-mark ${finding.kind}`} />
                    <div><strong>{finding.title}</strong><small>{finding.summary}</small></div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="library-empty">领域 Plugin 尚未提交诊断结论。</div>
            )}
          </section>
        </div>
      </div>
    </section>
  );
}
