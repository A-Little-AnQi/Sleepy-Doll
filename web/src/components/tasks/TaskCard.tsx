import {
  TaskCard as Card,
  type TaskActions as CardActions,
} from "@sleepy-doll/ui";
import type { TaskSummary, TaskInfo } from "../../ipc/types";
import { isRunning, taskLabels } from "../../session";
import { taskError } from "./task-display";

export type TaskActions = CardActions<TaskSummary>;

/** 业务层只负责运行状态和错误文案，卡片由内部组件包绘制。 */
export function TaskCard({
  task,
  activeRun,
  ...props
}: {
  task: TaskSummary;
  activeRun?: TaskInfo | undefined;
  busy?: boolean;
  actions: TaskActions;
  showSource?: boolean;
  showDescription?: boolean;
}) {
  return (
    <Card
      {...props}
      task={task}
      activeRun={activeRun}
      issueText={taskError(task.issue ?? undefined)}
      runErrorText={taskError(activeRun?.error)}
      running={isRunning(activeRun)}
      runStateLabel={activeRun ? taskLabels[activeRun.state] : undefined}
    />
  );
}
