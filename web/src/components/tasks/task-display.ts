import type { TaskInfo, TaskSummary } from "../../ipc/types";
import { isRunning } from "../../session";
export function taskRun(task: TaskSummary, runs: TaskInfo[]) {
  const related = runs.filter(
    (run) =>
      run.source?.kind === "savedWorkflow" && run.source.workflowId === task.id,
  );
  return (
    related.find(isRunning) ??
    related.find((run) => run.id === task.lastRunId) ??
    related[0]
  );
}
export function taskError(error?: string) {
  if (!error) return "";
  return /bridge|接口|methodId|catalogVersion|程序集|schema/i.test(error)
    ? "未能运行这项任务，请检查应用连接和任务设置。"
    : error;
}
