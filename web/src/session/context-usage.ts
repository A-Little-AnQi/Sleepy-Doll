import type { TaskInfo } from "../ipc/types";

export interface ContextUsage {
  used: number;
  window: number;
  cacheHit: number;
}

/** 后端只提交已结算的主请求用量；新运行尚无统计时沿用本会话上一份。 */
export function latestContextUsage(
  runs: Iterable<TaskInfo>,
  current?: TaskInfo,
): ContextUsage | undefined {
  const measured = (run: TaskInfo) =>
    (!run.source || run.source.kind === "agent") &&
    Number.isFinite(run.contextTokens) &&
    (run.contextTokens ?? 0) > 0;
  const run =
    current && measured(current)
      ? current
      : [...runs]
          .filter(measured)
          .sort(
            (a, b) =>
              b.createdAt.localeCompare(a.createdAt) ||
              b.id.localeCompare(a.id),
          )[0];
  return run
    ? {
        used: run.contextTokens!,
        window: run.contextWindow ?? 0,
        cacheHit: run.cacheReadTokens ?? 0,
      }
    : undefined;
}

export function formatTokens(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 10_000) return `${Math.round(value / 1000)}k`;
  if (value >= 1000) return `${(value / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  return String(Math.max(0, Math.round(value)));
}
