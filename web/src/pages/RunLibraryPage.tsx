import { useState } from "react";

import { api } from "../api";
import { AlertIcon } from "../components/icons";
import type { Bootstrap } from "../types";

interface Props {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  onRun(conversationId: string): void;
}

const stateLabels: Record<string, string> = {
  draft: "等待检查",
  validating: "正在检查",
  awaitingAuthorization: "等你确认",
  preparing: "正在准备",
  prepared: "准备完成",
  committing: "正在执行",
  verifying: "正在核对结果",
  succeeded: "已完成",
  rollingBack: "正在撤销",
  rolledBack: "已撤销",
  recovering: "正在恢复",
  needsReview: "结果待确认",
  failed: "失败",
  cancelled: "已取消",
};

/** What agreeing to an operation actually risks, in plain words. The value was
 * already being sent to the interface but never shown, so users were asked to
 * approve actions without seeing how risky they were. */
const riskLabels: Record<string, { text: string; tone: string }> = {
  observe: { text: "只读，不改动游戏", tone: "is-calm" },
  low: { text: "改动很小，容易撤销", tone: "is-calm" },
  standard: { text: "会改动游戏状态", tone: "is-warn" },
  high: { text: "改动较大，请确认后再执行", tone: "is-warn" },
  irreversible: { text: "不可撤销，做了就回不去", tone: "is-danger" },
};

function repairText(action: unknown): string {
  if (typeof action === "string") return action;
  if (action && typeof action === "object") {
    const record = action as Record<string, unknown>;
    for (const key of ["description", "summary", "title", "text"]) {
      if (typeof record[key] === "string") return record[key] as string;
    }
    return JSON.stringify(action, null, 2);
  }
  return "";
}

export function RunLibraryPage({ bootstrap, reload, onRun }: Props) {
  const [running, setRunning] = useState<string>();
  const [error, setError] = useState("");
  const [busyOperation, setBusyOperation] = useState<string>();

  const launch = async (id: string, legacy: boolean) => {
    setRunning(id);
    setError("");
    try {
      const run = legacy
        ? await api.runStrategy(id)
        : await api.runWorkflow(id);
      await reload();
      onRun(run.conversationId);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRunning(undefined);
    }
  };

  const flows = [
    ...bootstrap.workflows.map((workflow) => ({
      id: workflow.id,
      name: workflow.name,
      steps: workflow.steps,
      legacy: false,
      autonomous: workflow.unattended === "allowed",
      replayed: true,
    })),
    ...bootstrap.strategies.map((strategy) => ({
      id: strategy.id,
      name: strategy.name,
      steps: strategy.plan.steps,
      legacy: true,
      autonomous: false,
      replayed: true,
    })),
  ];

  const runOperation = async (
    id: string,
    action: "executeOperation" | "rollbackOperation",
  ) => {
    setBusyOperation(id);
    setError("");
    try {
      await api[action](id);
      await reload();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusyOperation(undefined);
    }
  };

  return (
    <div className="page-sheet run-page">
      {error ? (
        <div className="inline-error" role="alert">
          <strong>操作失败</strong>
          <span>{error}</span>
        </div>
      ) : null}

      <section className="page-block">
        <h2>可以重跑的流程</h2>
        <p className="muted">
          这些是之前成功跑通、并且核对过结果的流程。重跑时不会再问模型，也不用等它思考。
        </p>
        {flows.length ? (
          <div className="flow-grid">
            {flows.map((flow) => (
              <article className="flow-card" key={`${flow.legacy}-${flow.id}`}>
                <strong>{flow.name}</strong>
                <small>{flow.steps.length} 个步骤</small>
                <ol>
                  {flow.steps.map((step) => (
                    <li key={step.id}>{step.title}</li>
                  ))}
                </ol>
                <div className="flow-actions">
                  <button
                    type="button"
                    className="primary-action"
                    disabled={Boolean(running)}
                    title={
                      running ? "已经有一条流程在跑了，等它结束" : undefined
                    }
                    onClick={() => void launch(flow.id, flow.legacy)}
                  >
                    {running === flow.id ? "正在开始…" : "开始运行"}
                  </button>
                  <small>
                    {flow.autonomous
                      ? "可以直接跑完，不用逐步确认"
                      : "过程中需要你确认的操作会停下来问你"}
                  </small>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-note">
            还没有可重跑的流程。在对话里成功完成一次操作后，选择“保存到运行记录”，
            它就会出现在这里。
          </p>
        )}
      </section>

      <section className="page-block">
        <h2>最近的操作</h2>
        <p className="muted">Sleepy Doll 对游戏做过的改动，以及它们的结果。</p>
        {bootstrap.operations.length ? (
          <div className="record-list">
            {bootstrap.operations.map((operation) => {
              const risk = riskLabels[operation.risk];
              return (
                <article className="record" key={operation.id}>
                  <div className="record-main">
                    <strong>{operation.title}</strong>
                    <small>
                      {stateLabels[operation.state] ?? operation.state}
                      {operation.error ? ` · ${operation.error}` : ""}
                    </small>
                    {risk ? (
                      <span className={`tag ${risk.tone}`}>
                        {risk.tone === "is-calm" ? null : (
                          <AlertIcon className="tag-icon" />
                        )}
                        {risk.text}
                      </span>
                    ) : null}
                  </div>
                  {operation.state === "awaitingAuthorization" ? (
                    <button
                      type="button"
                      className="runtime-action"
                      disabled={Boolean(busyOperation)}
                      onClick={() =>
                        void runOperation(operation.id, "executeOperation")
                      }
                    >
                      {busyOperation === operation.id
                        ? "正在执行…"
                        : "执行这项操作"}
                    </button>
                  ) : null}
                  {operation.state === "needsReview" ? (
                    <button
                      type="button"
                      className="runtime-action"
                      disabled={Boolean(busyOperation)}
                      onClick={() =>
                        void runOperation(operation.id, "rollbackOperation")
                      }
                    >
                      {busyOperation === operation.id
                        ? "正在撤销…"
                        : "撤销这项操作"}
                    </button>
                  ) : null}
                </article>
              );
            })}
          </div>
        ) : (
          <p className="empty-note">还没有执行过会改动游戏的操作。</p>
        )}
      </section>

      <section className="page-block">
        <h2>插件提供的内容</h2>
        <p className="muted">插件告诉 Sleepy Doll 它有哪些东西可以操作。</p>
        {bootstrap.resources.length ? (
          <div className="record-list">
            {bootstrap.resources.map((resource) => (
              <article className="record" key={resource.id}>
                <div className="record-main">
                  <strong>{resource.displayName}</strong>
                  <small>
                    {resource.kind} · 版本 {resource.version} · 来自{" "}
                    {resource.providerId}
                  </small>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-note">
            当前没有插件提供内容。装一个插件后，它能操作的东西会列在这里。
          </p>
        )}
      </section>

      <section className="page-block">
        <h2>检查发现的问题</h2>
        {bootstrap.diagnostics.length ? (
          <div className="record-list">
            {bootstrap.diagnostics.map((finding) => {
              const repair = repairText(finding.repairAction);
              return (
                <article className="record" key={finding.id}>
                  <div className="record-main">
                    <strong>{finding.title}</strong>
                    <small>{finding.summary}</small>
                    {repair ? (
                      <span className="record-repair">怎么处理：{repair}</span>
                    ) : null}
                  </div>
                </article>
              );
            })}
          </div>
        ) : (
          <p className="empty-note">没有发现问题。</p>
        )}
      </section>
    </div>
  );
}
