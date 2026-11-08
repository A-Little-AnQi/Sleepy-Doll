import {
  TaskCard as Card,
  type TaskActions as CardActions,
} from "@sleepy-doll/ui";
import type { Bootstrap, TaskSummary, TaskInfo } from "../../ipc/types";
import { isRunning } from "../../session";
import { taskError } from "./task-display";
import { taskTargets } from "./task-plan";
import { providerOfTool } from "../../ipc/providers";

export type TaskActions = CardActions<TaskSummary>;

/** 业务层只负责运行状态和错误文案，卡片由内部组件包绘制。 */
export function TaskCard({
  task,
  activeRun,
  catalog,
  ...props
}: {
  task: TaskSummary;
  activeRun?: TaskInfo | undefined;
  catalog?: Pick<Bootstrap, "plugins" | "tools">;
  busy?: boolean;
  actions: TaskActions;
  showSource?: boolean;
  showDescription?: boolean;
}) {
  const binding = task.shortcut;
  const calls = binding?.steps?.length
    ? binding.steps.flatMap((step) => (step.action ? [step.action] : []))
    : binding?.action
      ? [binding.action]
      : [];
  const applications = [
    ...new Set(
      calls.flatMap((call) => {
        const tool = catalog?.tools.find((tool) => tool.name === call.tool);
        const provider = providerOfTool(call.tool, tool?.source ?? "");
        const plugin = catalog?.plugins.find(
          (plugin) => plugin.manifest.id === provider,
        );
        return plugin ? [plugin.manifest.name] : [];
      }),
    ),
  ];
  if (!applications.length && binding?.applicationName)
    applications.push(
      ...binding.applicationName
        .split(/[、，,；;]/u)
        .map((name) => name.trim())
        .filter(Boolean),
    );
  return (
    <Card
      {...props}
      task={task}
      activeRun={activeRun}
      issueText={taskError(task.issue ?? undefined)}
      runErrorText={taskError(activeRun?.error)}
      running={isRunning(activeRun)}
      applications={applications}
      targets={taskTargets(binding)}
    />
  );
}
