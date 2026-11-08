import { invoke } from "./api";

export type ScheduleTrigger =
  | { kind: "once"; at: string }
  | {
      kind: "daily";
      localTimes: string[];
      intervalDays?: number;
      startDate?: string;
    }
  | { kind: "weekly"; weekdays: number[]; localTimes: string[] }
  | {
      kind: "monthly";
      daysOfMonth: number[];
      lastDayOfMonth: boolean;
      localTimes: string[];
    };
export type ScheduleBinding =
  | {
      kind: "shortcut";
      shortcutId: string;
      expectedPublishedRevision: number;
    }
  | { kind: "closeGame" }
  | { kind: "closeToolSoftware" };
export interface ScheduleEntry {
  entryId: string;
  title: string;
  binding: ScheduleBinding;
}
export interface ScheduleInput {
  name: string;
  trigger: ScheduleTrigger;
  entries: { entryId?: string; binding: ScheduleBinding }[];
}
export interface Schedule {
  id: string;
  revision: number;
  name: string;
  enabled: boolean;
  registration: string;
  error?: string | null;
  trigger: ScheduleTrigger;
  entries: ScheduleEntry[];
}
export interface ScheduleAction {
  id: string;
  title: string;
  content: string;
  publishedRevision?: number;
  binding?: ScheduleBinding;
  available: boolean;
  issue?: string | null;
}
export interface ScheduleOccurrence {
  id: string;
  schedule: Schedule;
  state: string;
  createdAt: string;
  error?: string | null;
  results: {
    entryId: string;
    title: string;
    state: string;
    runId?: string | null;
    error?: string | null;
    stopState?: "requested" | "confirmed" | "unknown";
  }[];
}
const revision = (schedule: Schedule) => ({
  id: schedule.id,
  expectedRevision: schedule.revision,
});
export const scheduler = {
  list: () => invoke<{ items: Schedule[] }>("scheduler.list"),
  actions: (
    query = "",
    offset = 0,
    category: "shortcuts" | "defaults" = "shortcuts",
  ) =>
    invoke<{
      items: ScheduleAction[];
      total: number;
      nextOffset: number | null;
    }>("scheduler.actions.list", { query, offset, limit: 50, category }),
  preview: (input: ScheduleInput) =>
    invoke<{ nextRuns: string[] }>("scheduler.preview", { ...input }),
  create: (input: ScheduleInput) =>
    invoke<Schedule>("scheduler.create", { ...input }),
  update: (schedule: Schedule, input: ScheduleInput) =>
    invoke<Schedule>("scheduler.update", {
      ...revision(schedule),
      schedule: input,
    }),
  enabled: (schedule: Schedule, enabled: boolean) =>
    invoke<Schedule>("scheduler.set_enabled", {
      ...revision(schedule),
      enabled,
    }),
  remove: (schedule: Schedule) =>
    invoke("scheduler.delete", revision(schedule)),
  run: (schedule: Schedule, requestKey: string) =>
    invoke<ScheduleOccurrence>("scheduler.run_now", {
      ...revision(schedule),
      requestKey,
    }),
  history: (id?: string) =>
    invoke<{ items: ScheduleOccurrence[] }>("scheduler.history", {
      ...(id ? { id } : {}),
      limit: 50,
    }),
  stop: (occurrenceId: string) => invoke("scheduler.stop", { occurrenceId }),
};

const week = ["一", "二", "三", "四", "五", "六", "日"];
export function scheduleTime(trigger: ScheduleTrigger) {
  if (trigger.kind === "once")
    return new Date(trigger.at).toLocaleString("zh-CN", {
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  const prefix =
    trigger.kind === "daily"
      ? (trigger.intervalDays ?? 1) > 1
        ? `每 ${trigger.intervalDays} 天`
        : "每天"
      : trigger.kind === "weekly"
        ? `每周${trigger.weekdays.map((day) => week[day - 1]).join("、")}`
        : `每月${[...trigger.daysOfMonth.map((day) => `${day}日`), ...(trigger.lastDayOfMonth ? ["月末"] : [])].join("、")}`;
  return `${prefix} ${trigger.localTimes.join("、")}${trigger.kind === "daily" && trigger.startDate ? ` · ${trigger.startDate.replaceAll("-", "/")} 起` : ""}`;
}
