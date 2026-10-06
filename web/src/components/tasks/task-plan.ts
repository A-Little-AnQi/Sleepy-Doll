import type { ShortcutBinding } from "../../ipc/types";

const GROUP_TOOL = "bgi.api.invoke";
const GROUP_METHOD = "bgi.run_script_groups";

/**
 * 运行配置组的批量动作展开成「运行配置组：名称」列表；
 * 只有严格 true 才追加关闭游戏。识别不了返回空，由调用方退回兜底文案。
 */
function groupLines(action: { tool: string; arguments: Record<string, unknown> }): string[] {
  const args = action.arguments ?? {};
  if (action.tool !== GROUP_TOOL || args.methodId !== GROUP_METHOD) return [];
  const inner = (args.arguments ?? {}) as Record<string, unknown>;
  const names = Array.isArray(inner.groupNames)
    ? inner.groupNames.filter((name): name is string => typeof name === "string")
    : [];
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
      const expanded = groupLines(step.action);
      return expanded.length ? expanded : [step.title || binding.targetName];
    });
  }
  const expanded = binding.action ? groupLines(binding.action) : [];
  return expanded.length ? expanded : [binding.targetName];
}
