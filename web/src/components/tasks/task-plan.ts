import type { ShortcutBinding } from "../../ipc/types";

const GROUP_TOOL = "bgi.api.invoke";
const GROUP_METHOD = "bgi.run_script_groups";

function groupNames(action: {
  tool: string;
  arguments: Record<string, unknown>;
}): string[] {
  const args = action.arguments ?? {};
  if (action.tool !== GROUP_TOOL || args.methodId !== GROUP_METHOD) return [];
  const inner = (args.arguments ?? {}) as Record<string, unknown>;
  return Array.isArray(inner.groupNames)
    ? inner.groupNames.filter(
        (name): name is string =>
          typeof name === "string" && Boolean(name.trim()),
      )
    : [];
}

/** 卡片展示绑定的组项名称，不附加软件名或执行说明。 */
export function taskTargets(
  binding: ShortcutBinding | null | undefined,
): string[] {
  if (!binding) return [];
  const groups = binding.steps?.length
    ? binding.steps.flatMap((step) =>
        step.action ? groupNames(step.action) : [],
      )
    : binding.action
      ? groupNames(binding.action)
      : [];
  const names = groups.length ? groups : binding.targetName.split(/[、，,]/u);
  return [...new Set(names.map((name) => name.trim()).filter(Boolean))];
}

/**
 * 运行配置组的批量动作展开成「运行配置组：名称」列表；
 * 只有严格 true 才追加关闭游戏。识别不了返回空，由调用方退回兜底文案。
 */
function groupLines(action: {
  tool: string;
  arguments: Record<string, unknown>;
}): string[] {
  const args = action.arguments ?? {};
  if (
    action.tool === GROUP_TOOL &&
    ["bgi.run_host_command_plan", "bgi.run_game_command_plan"].includes(
      String(args.methodId),
    )
  ) {
    const plan = args.arguments as
      { steps?: Array<{ title?: string; waitSeconds?: number }> } | undefined;
    return Array.isArray(plan?.steps)
      ? plan.steps.map((step) =>
          step.waitSeconds != null
            ? `等待 ${step.waitSeconds} 秒`
            : step.title || "原生命令",
        )
      : [];
  }
  if (action.tool !== GROUP_TOOL || args.methodId !== GROUP_METHOD) return [];
  const inner = (args.arguments ?? {}) as Record<string, unknown>;
  const names = groupNames(action);
  const lines = names.map((name) => `运行配置组：${name}`);
  if (inner.closeGameAfter === true) lines.push("完成后关闭原神");
  return lines;
}

/** 「执行内容」有序列表：来自真实 shortcut binding，不做猜测。 */
export function taskPlan(
  binding: ShortcutBinding | null | undefined,
): string[] {
  if (!binding) return [];
  if (binding.steps?.length) {
    return binding.steps.flatMap((step) => {
      if (step.waitSeconds != null) return [`等待 ${step.waitSeconds} 秒`];
      const expanded = step.action ? groupLines(step.action) : [];
      return expanded.length ? expanded : [step.title || binding.targetName];
    });
  }
  const expanded = binding.action ? groupLines(binding.action) : [];
  return expanded.length ? expanded : [binding.targetName];
}
