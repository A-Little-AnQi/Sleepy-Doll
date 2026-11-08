import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button, IconButton, SearchField } from "@sleepy-doll/ui";
import { Dialog } from "@sleepy-doll/ui/overlay/Dialog";
import { ConfirmDialog } from "@sleepy-doll/ui/overlay/ConfirmDialog";
import { useMessage } from "@sleepy-doll/ui/overlay/Message";
import { SlidingTabs } from "@sleepy-doll/ui/controls/SlidingTabs";
import { CalendarIcon, CloseIcon, PlusIcon } from "@sleepy-doll/ui/icons";
import {
  useFlipList,
  usePresence,
  motionDisabled,
} from "@sleepy-doll/ui/motion";
import {
  scheduler,
  scheduleTime,
  type Schedule,
  type ScheduleAction,
  type ScheduleEntry,
  type ScheduleInput,
  type ScheduleTrigger,
} from "../../ipc/scheduler";
import type { TaskSummary } from "../../ipc/types";
import { readError } from "../../session";
import { normalizeTime, ScheduleTimeField } from "./ScheduleTimeField";
import { normalizeDate, ScheduleDateField } from "./ScheduleDateField";
import { useSchedulePopover } from "./useSchedulePopover";
import { useScheduleDrag, type ScheduleDragSource } from "./useScheduleDrag";

type Mode = ScheduleTrigger["kind"];
type Entry = ScheduleEntry & { saved: boolean };
const modes = [
  { id: "once" as const, name: "指定日期" },
  { id: "daily" as const, name: "按天" },
  { id: "weekly" as const, name: "每周" },
  { id: "monthly" as const, name: "每月" },
];
const days = ["一", "二", "三", "四", "五", "六", "日"];
const dateValue = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
const upcomingDate = (value: string) => {
  const now = new Date(),
    next = new Date(now);
  const normalized = normalizeTime(value);
  if (normalized) {
    const [hour = 0, minute = 0] = normalized.split(":").map(Number);
    next.setHours(hour, minute, 0, 0);
    if (next.getTime() <= now.getTime()) next.setDate(next.getDate() + 1);
  }
  return dateValue(next);
};
const asEntry = (action: ScheduleAction): Entry => ({
  entryId: crypto.randomUUID(),
  title: action.title,
  saved: false,
  binding: action.binding ?? {
    kind: "shortcut",
    shortcutId: action.id,
    expectedPublishedRevision: action.publishedRevision!,
  },
});

export function ScheduleEditor({
  open,
  schedule,
  initialTask,
  onClose,
  onSaved,
  enable,
}: {
  open: boolean;
  schedule: Schedule | null;
  initialTask?: TaskSummary | undefined;
  onClose(): void;
  onSaved(): Promise<void>;
  enable(): Promise<void>;
}) {
  const [mode, setMode] = useState<Mode>(schedule?.trigger.kind ?? "daily");
  const [date, setDate] = useState(() =>
    dateValue(
      schedule?.trigger.kind === "once"
        ? new Date(schedule.trigger.at)
        : new Date(Date.now() + 3600_000),
    ),
  );
  const [times, setTimes] = useState<Record<Mode, string>>(() => {
    const base = new Date(Date.now() + 3600_000);
    const fallback = `${String(base.getHours()).padStart(2, "0")}:${String(base.getMinutes()).padStart(2, "0")}`;
    const result = {
      once: fallback,
      daily: fallback,
      weekly: fallback,
      monthly: fallback,
    };
    if (schedule) {
      const t = schedule.trigger;
      result[t.kind] =
        t.kind === "once"
          ? new Date(t.at).toTimeString().slice(0, 5)
          : (t.localTimes[0] ?? fallback);
    }
    return result;
  });
  const initialDaily =
    schedule?.trigger.kind === "daily" ? schedule.trigger : null;
  const [intervalDays, setIntervalDays] = useState(
    String(initialDaily?.intervalDays ?? 1),
  );
  const lastValidInterval = useRef(initialDaily?.intervalDays ?? 1);
  const intervalNumber = /^\d+$/.test(intervalDays.trim())
    ? Number(intervalDays)
    : NaN;
  const intervalValid =
    Number.isInteger(intervalNumber) &&
    intervalNumber >= 1 &&
    intervalNumber <= 365;
  const [intervalStart, setIntervalStart] = useState(
    initialDaily?.startDate ?? upcomingDate(times.daily),
  );
  const startEdited = useRef(Boolean(initialDaily?.startDate));
  const manualStartBeforeEdit = useRef(startEdited.current);
  const explicitDailyStart = useRef(
    Boolean(initialDaily?.startDate && (initialDaily.intervalDays ?? 1) === 1),
  );
  const showStart =
    mode === "daily" &&
    ((intervalValid ? intervalNumber : lastValidInterval.current) > 1 ||
      explicitDailyStart.current);
  const showDate = mode === "once" || showStart;
  const dateGroup = useRef<HTMLDivElement>(null);
  const dateKind = useRef<"once" | "daily">(mode === "once" ? "once" : "daily");
  if (showDate) dateKind.current = mode === "once" ? "once" : "daily";
  const datePresent = usePresence(showDate, dateGroup, { y: 4, scale: 1 });
  const timeGroup = useRef<HTMLDivElement>(null);
  const timeMotion = useRef<Animation | null>(null);
  const previousTime = useRef<{ x: number; y: number } | null>(null);
  const [weekdays, setWeekdays] = useState(
    schedule?.trigger.kind === "weekly"
      ? schedule.trigger.weekdays
      : [1, 2, 3, 4, 5],
  );
  const [monthDays, setMonthDays] = useState(
    schedule?.trigger.kind === "monthly" ? schedule.trigger.daysOfMonth : [1],
  );
  const [lastDay, setLastDay] = useState(
    schedule?.trigger.kind === "monthly"
      ? schedule.trigger.lastDayOfMonth
      : false,
  );
  const [monthOpen, setMonthOpen] = useState(false);
  const month = useRef<HTMLDivElement>(null);
  const monthGrid = useRef<HTMLDivElement>(null);
  const monthPresent = usePresence(monthOpen, monthGrid, { y: 4, scale: 0.99 });
  useSchedulePopover(monthOpen, monthPresent, month, monthGrid);
  useEffect(() => {
    if (!monthOpen) return;
    const outside = (event: PointerEvent) => {
      if (
        !month.current?.contains(event.target as Node) &&
        !monthGrid.current?.contains(event.target as Node)
      )
        setMonthOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        setMonthOpen(false);
        month.current?.querySelector("button")?.focus({ preventScroll: true });
      }
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape, true);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape, true);
    };
  }, [monthOpen]);
  const [entries, setEntries] = useState<Entry[]>(() =>
    schedule
      ? schedule.entries.map((entry) => ({ ...entry, saved: true }))
      : initialTask?.publishedRevision
        ? [
            asEntry({
              id: initialTask.id,
              title: initialTask.name,
              content: initialTask.description,
              publishedRevision: initialTask.publishedRevision,
              available: true,
            }),
          ]
        : [],
  );
  const [query, setQuery] = useState("");
  const [libraryTab, setLibraryTab] = useState<"shortcuts" | "defaults">(
    "shortcuts",
  );
  const [actions, setActions] = useState<ScheduleAction[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [libraryError, setLibraryError] = useState("");
  const [libraryReload, setLibraryReload] = useState(0);
  const [loading, setLoading] = useState(true),
    [loadingMore, setLoadingMore] = useState(false);
  const latestQuery = useRef(query);
  latestQuery.current = query;
  const latestLibraryTab = useRef(libraryTab);
  latestLibraryTab.current = libraryTab;
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const message = useMessage();
  const [saving, setSaving] = useState(false);
  const [replacement, setReplacement] = useState<ScheduleInput | null>(null);
  const [discard, setDiscard] = useState(false);
  const [pane, setPane] = useState<"settings" | "library">("settings");
  const queue = useRef<HTMLDivElement>(null),
    library = useRef<HTMLDivElement>(null);
  const timeSection = useRef<HTMLDivElement>(null),
    timeSectionBefore = useRef<number | null>(null),
    timeSectionMotion = useRef<Animation | null>(null);
  const changeLayout = (change: () => void) => {
    timeSectionBefore.current = timeSection.current
      ? parseFloat(getComputedStyle(timeSection.current).height)
      : null;
    change();
  };
  const shortMonthHint =
    mode === "monthly" && monthDays.some((day) => day > 28);
  useLayoutEffect(() => {
    const node = timeGroup.current?.querySelector<HTMLElement>(
      ".schedule-time-field",
    );
    const section = timeSection.current;
    if (!node || !section) return;
    const box = node.getBoundingClientRect(),
      parent = section.getBoundingClientRect();
    const scale = parent.width / (section.offsetWidth || parent.width || 1);
    const matrix = new DOMMatrixReadOnly(getComputedStyle(node).transform);
    const target = {
      x: (box.left - parent.left) / scale - matrix.e,
      y: (box.top - parent.top) / scale - matrix.f,
    };
    const before = previousTime.current;
    previousTime.current = target;
    timeMotion.current?.cancel();
    if (!before || motionDisabled()) return;
    const x = before.x + matrix.e - target.x,
      y = before.y + matrix.f - target.y;
    if (Math.abs(x) + Math.abs(y) < 0.5) return;
    const animation = node.animate(
      [
        { transform: `translate(${x}px,${y}px)` },
        { transform: "translate(0,0)" },
      ],
      { duration: 200, easing: "cubic-bezier(0.2,0,0,1)" },
    );
    timeMotion.current = animation;
    void animation.finished.then(
      () => {
        if (timeMotion.current === animation) timeMotion.current = null;
      },
      () => {},
    );
  }, [mode, datePresent]);
  useLayoutEffect(() => {
    const section = timeSection.current;
    if (!section) return;
    let width = section.offsetWidth;
    const observer = new ResizeObserver(() => {
      if (section.offsetWidth === width) return;
      width = section.offsetWidth;
      const node = timeGroup.current?.querySelector<HTMLElement>(
        ".schedule-time-field",
      );
      if (!node) return;
      const box = node.getBoundingClientRect(),
        parent = section.getBoundingClientRect();
      const scale = parent.width / (section.offsetWidth || parent.width || 1);
      const matrix = new DOMMatrixReadOnly(getComputedStyle(node).transform);
      previousTime.current = {
        x: (box.left - parent.left) / scale - matrix.e,
        y: (box.top - parent.top) / scale - matrix.f,
      };
      timeMotion.current?.cancel();
      timeMotion.current = null;
    });
    observer.observe(section);
    return () => observer.disconnect();
  }, []);
  useEffect(() => () => timeMotion.current?.cancel(), []);
  useLayoutEffect(() => {
    const node = timeSection.current,
      previous = timeSectionBefore.current;
    timeSectionBefore.current = null;
    if (!node || previous === null) return;
    timeSectionMotion.current?.cancel();
    const height = parseFloat(getComputedStyle(node).height);
    if (!motionDisabled() && Math.abs(previous - height) > 0.5)
      timeSectionMotion.current = node.animate(
        [{ height: `${previous}px` }, { height: `${height}px` }],
        { duration: 200, easing: "cubic-bezier(0.2,0,0,1)" },
      );
  }, [mode, datePresent, shortMonthHint]);
  useEffect(() => () => timeSectionMotion.current?.cancel(), []);
  const name = useRef(schedule?.name ?? "");
  const snapshot = JSON.stringify({
    mode,
    date,
    times,
    intervalDays,
    intervalStart,
    weekdays,
    monthDays,
    lastDay,
    entries,
  });
  const initial = useRef(snapshot);
  useEffect(() => message.dismiss(), [snapshot, message]);
  const insert = (source: ScheduleDragSource, index: number) => {
    if (
      source.kind === "library" &&
      (!source.action.available || entries.length >= 32)
    )
      return;
    const entry =
      source.kind === "library"
        ? asEntry(source.action)
        : entries.find((entry) => entry.entryId === source.key);
    if (!entry) return;
    setEntries((current) => {
      const next = current.filter(
        (entry) => source.kind !== "entry" || entry.entryId !== source.key,
      );
      if (
        source.kind === "library" &&
        (!source.action.available || current.length >= 32)
      )
        return current;
      next.splice(Math.max(0, Math.min(index, next.length)), 0, entry);
      return next;
    });
    return entry.entryId;
  };
  const drag = useScheduleDrag(queue, library, insert);
  useFlipList(queue, entries.map((entry) => entry.entryId).join(","));
  useEffect(() => {
    let alive = true;
    setLoading(true);
    setNextOffset(null);
    setLoadingMore(false);
    setLibraryError("");
    const timer = setTimeout(
      () => {
        void scheduler
          .actions(query, 0, libraryTab)
          .then((result) => {
            if (alive) {
              setActions(result.items.filter((action) => action.available));
              setNextOffset(result.nextOffset);
            }
          })
          .catch((reason) => {
            if (alive) {
              setLibraryError(readError(reason));
              message.error(readError(reason));
            }
          })
          .finally(() => {
            if (alive) setLoading(false);
          });
      },
      query ? 180 : 0,
    );
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [query, libraryTab, libraryReload]);
  const requestClose = () => {
    if (saving) return;
    message.dismiss();
    if (snapshot !== initial.current) setDiscard(true);
    else onClose();
  };
  const loadMore = async () => {
    if (nextOffset === null || loadingMore) return;
    const currentQuery = query;
    const currentTab = libraryTab;
    setLoadingMore(true);
    try {
      const result = await scheduler.actions(
        currentQuery,
        nextOffset,
        libraryTab,
      );
      if (
        !alive.current ||
        latestQuery.current !== currentQuery ||
        latestLibraryTab.current !== currentTab
      )
        return;
      setActions((current) => [
        ...current,
        ...result.items.filter((action) => action.available),
      ]);
      setNextOffset(result.nextOffset);
    } catch (reason) {
      if (
        alive.current &&
        latestQuery.current === currentQuery &&
        latestLibraryTab.current === currentTab
      )
        message.error(readError(reason));
    } finally {
      if (
        alive.current &&
        latestQuery.current === currentQuery &&
        latestLibraryTab.current === currentTab
      )
        setLoadingMore(false);
    }
  };
  const add = (action: ScheduleAction) => {
    insert({ kind: "library", action }, entries.length);
    setPane("settings");
  };
  const input = (): ScheduleInput => {
    if (!entries.length) throw new Error("请添加要执行的任务");
    const time = normalizeTime(times[mode]);
    if (!time) throw new Error("请输入有效时间：小时 00–23，分钟 00–59");
    const normalized = [time];
    let trigger: ScheduleTrigger;
    if (mode === "once") {
      const normalizedDate = normalizeDate(date);
      const at = new Date(`${normalizedDate}T${normalized[0]}:00`);
      if (
        !normalizedDate ||
        !Number.isFinite(at.getTime()) ||
        at.getTime() <= Date.now()
      )
        throw new Error("请选择晚于现在的日期和时间");
      trigger = { kind: "once", at: at.toISOString() };
    } else if (mode === "daily") {
      if (!intervalValid) throw new Error("天数间隔须为 1–365 的整数");
      const start = normalizeDate(intervalStart);
      if (showStart && (!start || start < "1601-01-01"))
        throw new Error("请选择有效的开始日期");
      trigger = {
        kind: "daily",
        localTimes: normalized,
        ...(showStart
          ? { intervalDays: intervalNumber, startDate: start! }
          : {}),
      };
    } else if (mode === "weekly") {
      if (!weekdays.length) throw new Error("请至少选择一个星期");
      trigger = {
        kind: "weekly",
        weekdays: [...weekdays].sort((a, b) => a - b),
        localTimes: normalized,
      };
    } else {
      if (!monthDays.length && !lastDay)
        throw new Error("请至少选择一个日期或月末");
      trigger = {
        kind: "monthly",
        daysOfMonth: [...monthDays].sort((a, b) => a - b),
        lastDayOfMonth: lastDay,
        localTimes: normalized,
      };
    }
    return {
      name:
        name.current ||
        entries
          .map((entry) => entry.title)
          .join("、")
          .slice(0, 80),
      trigger,
      entries: entries.map((entry) => ({
        ...(entry.saved ? { entryId: entry.entryId } : {}),
        binding: entry.binding,
      })),
    };
  };
  const persist = async (value: ScheduleInput) => {
    if (saving) return;
    setSaving(true);
    try {
      await scheduler.preview(value);
      await enable();
      const result = schedule
        ? await scheduler.update(schedule, value)
        : await scheduler.create(value);
      if (result.registration !== "verified" || result.error)
        throw new Error(result.error || "定时任务注册尚未完成，请核对后重试");
      await onSaved();
      onClose();
    } catch (reason) {
      message.error(readError(reason));
    } finally {
      setSaving(false);
    }
  };
  const save = async () => {
    message.dismiss();
    let value: ScheduleInput;
    try {
      value = input();
    } catch (reason) {
      message.error(readError(reason));
      setPane("settings");
      return;
    }
    if (
      schedule &&
      schedule.trigger.kind !== "once" &&
      schedule.trigger.localTimes.length > 1
    ) {
      setReplacement(value);
      return;
    }
    await persist(value);
  };
  const toggle = (values: number[], value: number) =>
    values.includes(value)
      ? values.filter((day) => day !== value)
      : [...values, value];
  return (
    <>
      <Dialog
        open={open}
        title={schedule ? "编辑定时任务" : "新建定时任务"}
        className="schedule-dialog"
        initialFocus="dialog"
        onClose={requestClose}
        footer={
          <>
            <Button variant="subtle" disabled={saving} onClick={requestClose}>
              取消
            </Button>
            <Button
              variant="primary"
              disabled={saving}
              onClick={() => void save()}
            >
              {saving ? "正在保存…" : schedule ? "保存更改" : "创建并启用"}
            </Button>
          </>
        }
      >
        <div className="schedule-mobile-tabs">
          <SlidingTabs
            ariaLabel="定时任务设置"
            value={pane}
            onChange={setPane}
            items={[
              { id: "settings", name: "执行设置" },
              { id: "library", name: "任务库" },
            ]}
          />
        </div>
        <div className="schedule-editor" data-pane={pane}>
          <section className="schedule-settings">
            <div className="schedule-time-section" ref={timeSection}>
              <h3>执行时间</h3>
              <SlidingTabs
                ariaLabel="重复方式"
                value={mode}
                onChange={(next) => {
                  setMonthOpen(false);
                  changeLayout(() => setMode(next));
                }}
                items={modes}
              />
              {mode === "weekly" && (
                <div className="schedule-weekdays" aria-label="执行星期">
                  {days.map((day, i) => (
                    <button
                      key={day}
                      type="button"
                      aria-label={`周${day}`}
                      aria-pressed={weekdays.includes(i + 1)}
                      onClick={() => setWeekdays(toggle(weekdays, i + 1))}
                    >
                      {day}
                    </button>
                  ))}
                </div>
              )}
              {mode === "daily" && (
                <label className="schedule-day-interval">
                  <span>每</span>
                  <input
                    aria-label="间隔天数"
                    inputMode="numeric"
                    value={intervalDays}
                    aria-invalid={!intervalValid || undefined}
                    onChange={(event) => {
                      const value = event.target.value;
                      const n = /^\d+$/.test(value.trim())
                        ? Number(value)
                        : NaN;
                      if (Number.isInteger(n) && n >= 1 && n <= 365)
                        lastValidInterval.current = n;
                      changeLayout(() => setIntervalDays(value));
                      if (!startEdited.current)
                        setIntervalStart(upcomingDate(times.daily));
                    }}
                    onBlur={() => {
                      if (intervalValid)
                        setIntervalDays(String(intervalNumber));
                      else message.error("天数间隔须为 1–365 的整数");
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        event.stopPropagation();
                        if (intervalValid) event.currentTarget.blur();
                        else message.error("天数间隔须为 1–365 的整数");
                      }
                    }}
                  />
                  <span>天</span>
                </label>
              )}
              <div className="schedule-primary-timing">
                {datePresent && (
                  <div
                    className="schedule-timing-field"
                    ref={dateGroup}
                    inert={!showDate}
                  >
                    {dateKind.current === "daily" && (
                      <span className="schedule-field-label">开始日期</span>
                    )}
                    <ScheduleDateField
                      key={dateKind.current}
                      min={
                        dateKind.current === "once"
                          ? dateValue(new Date())
                          : "1601-01-01"
                      }
                      label={
                        dateKind.current === "once" ? "执行日期" : "开始日期"
                      }
                      fallback={dateValue(new Date())}
                      onEditStart={() => {
                        manualStartBeforeEdit.current = startEdited.current;
                      }}
                      onEditCancel={() => {
                        startEdited.current = manualStartBeforeEdit.current;
                      }}
                      minimumMessage={
                        dateKind.current === "once"
                          ? "执行日期不能早于今天"
                          : "开始日期不能早于1601年1月1日"
                      }
                      value={dateKind.current === "once" ? date : intervalStart}
                      onChange={(value) => {
                        if (dateKind.current === "once") setDate(value);
                        else {
                          startEdited.current = true;
                          setIntervalStart(value);
                        }
                      }}
                    />
                  </div>
                )}
                {mode === "monthly" && (
                  <div className="schedule-month" ref={month}>
                    <Button
                      variant="secondary"
                      aria-expanded={monthOpen}
                      onClick={() => setMonthOpen(!monthOpen)}
                    >
                      <CalendarIcon className="button-icon" />
                      <span>
                        {[
                          ...monthDays
                            .slice()
                            .sort((a, b) => a - b)
                            .map((day) => `${day}日`),
                          ...(lastDay ? ["月末"] : []),
                        ].join("、") || "选择日期"}
                      </span>
                    </Button>
                    {monthPresent &&
                      createPortal(
                        <div
                          ref={monthGrid}
                          inert={!monthOpen}
                          className="schedule-month-grid"
                          role="group"
                          aria-label="每月日期"
                        >
                          {Array.from({ length: 31 }, (_, i) => i + 1).map(
                            (day) => (
                              <button
                                key={day}
                                type="button"
                                aria-pressed={monthDays.includes(day)}
                                onClick={() =>
                                  changeLayout(() =>
                                    setMonthDays(toggle(monthDays, day)),
                                  )
                                }
                              >
                                {day}
                              </button>
                            ),
                          )}
                          <button
                            type="button"
                            className="schedule-last-day"
                            aria-pressed={lastDay}
                            onClick={() => setLastDay(!lastDay)}
                          >
                            月末
                          </button>
                        </div>,
                        month.current?.closest(".sd-dialog-layer") ??
                          document.body,
                      )}
                  </div>
                )}
                <div
                  className="schedule-timing-field"
                  ref={timeGroup}
                  key="time"
                >
                  {datePresent && dateKind.current === "daily" && (
                    <span className="schedule-field-label">执行时刻</span>
                  )}
                  <ScheduleTimeField
                    key={mode}
                    index={0}
                    value={times[mode]}
                    onChange={(value) => {
                      setTimes((current) => ({ ...current, [mode]: value }));
                      if (
                        mode === "daily" &&
                        !startEdited.current &&
                        normalizeTime(value)
                      )
                        setIntervalStart(upcomingDate(value));
                    }}
                  />
                </div>
              </div>
              {mode === "monthly" && monthDays.some((day) => day > 28) && (
                <small className="schedule-hint">当月没有的日期将跳过。</small>
              )}
            </div>
            <div className="schedule-execution">
              <div className="schedule-queue-heading">
                <h3>执行顺序</h3>
              </div>
              <div
                className="schedule-queue"
                ref={queue}
                aria-label="执行任务列表"
              >
                {entries.length ? (
                  entries.map((entry, index) => (
                    <div
                      className="schedule-queue-row"
                      key={entry.entryId}
                      data-entry-key={entry.entryId}
                      data-sd-slot=""
                      data-sd-id={entry.entryId}
                      data-dragging={
                        (drag.source?.kind === "entry" &&
                          drag.source.key === entry.entryId) ||
                        undefined
                      }
                    >
                      <button
                        className="schedule-drag-handle"
                        aria-label={`移动第 ${index + 1} 项 ${entry.title}`}
                        onPointerDown={(event) =>
                          drag.start(event, {
                            kind: "entry",
                            key: entry.entryId,
                            title: entry.title,
                          })
                        }
                        onKeyDown={(event) => {
                          if (
                            event.key !== "ArrowUp" &&
                            event.key !== "ArrowDown"
                          )
                            return;
                          event.preventDefault();
                          insert(
                            {
                              kind: "entry",
                              key: entry.entryId,
                              title: entry.title,
                            },
                            Math.max(
                              0,
                              Math.min(
                                entries.length - 1,
                                index + (event.key === "ArrowUp" ? -1 : 1),
                              ),
                            ),
                          );
                        }}
                      >
                        ⠿
                      </button>
                      <span className="schedule-order">{index + 1}</span>
                      <span className="schedule-entry-title">
                        {entry.title}
                      </span>
                      <IconButton
                        aria-label={`移除第 ${index + 1} 项 ${entry.title}`}
                        onClick={() =>
                          setEntries((current) =>
                            current.filter(
                              (item) => item.entryId !== entry.entryId,
                            ),
                          )
                        }
                      >
                        <CloseIcon className="button-icon" />
                      </IconButton>
                    </div>
                  ))
                ) : (
                  <div className="schedule-queue-empty">
                    <span className="schedule-desktop-hint">
                      从右侧拖入任务
                    </span>
                    <Button
                      className="schedule-mobile-add"
                      variant="subtle"
                      onClick={() => setPane("library")}
                    >
                      添加任务
                    </Button>
                  </div>
                )}
              </div>
              {entries.length >= 32 && (
                <small className="schedule-hint">最多添加 32 项任务。</small>
              )}
            </div>
          </section>
          <section className="schedule-library">
            <h3>任务库</h3>
            <SlidingTabs
              ariaLabel="任务来源"
              value={libraryTab}
              onChange={(value) => {
                setLibraryTab(value);
                setQuery("");
              }}
              items={[
                { id: "shortcuts", name: "快捷任务" },
                { id: "defaults", name: "默认" },
              ]}
            />
            <SearchField
              aria-label="搜索标题或内容"
              placeholder="搜索标题或内容"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <div
              className="schedule-library-list"
              ref={library}
              aria-label="可绑定任务"
            >
              {libraryError ? (
                <div className="schedule-library-empty">
                  <Button
                    variant="subtle"
                    onClick={() => setLibraryReload((value) => value + 1)}
                  >
                    重新读取任务库
                  </Button>
                </div>
              ) : loading ? (
                <div className="schedule-library-empty">正在查找…</div>
              ) : actions.length ? (
                actions.map((action) => (
                  <div
                    className="schedule-library-row"
                    key={action.id}
                    role="button"
                    tabIndex={action.available ? 0 : -1}
                    aria-disabled={!action.available || entries.length >= 32}
                    aria-label={`添加 ${action.title}`}
                    onPointerDown={(event) => {
                      if (
                        action.available &&
                        entries.length < 32 &&
                        !(event.target as HTMLElement).closest("button")
                      )
                        drag.start(event, { kind: "library", action });
                    }}
                    onKeyDown={(event) => {
                      if (event.target !== event.currentTarget) return;
                      if (
                        (event.key === "Enter" || event.key === " ") &&
                        action.available
                      ) {
                        event.preventDefault();
                        add(action);
                      }
                    }}
                  >
                    <span className="schedule-library-grip" aria-hidden="true">
                      ⠿
                    </span>
                    <div className="schedule-library-body">
                      <strong>{action.title}</strong>
                      {action.content && <p>{action.content}</p>}
                    </div>
                    <IconButton
                      className="schedule-library-add"
                      aria-label={`添加一份 ${action.title}`}
                      disabled={!action.available || entries.length >= 32}
                      onClick={() => add(action)}
                    >
                      <PlusIcon className="button-icon" />
                    </IconButton>
                  </div>
                ))
              ) : (
                <div className="schedule-library-empty">
                  {query ? "没有匹配的任务" : "还没有可绑定的快捷任务"}
                </div>
              )}
              {nextOffset !== null && !loading && (
                <Button
                  variant="subtle"
                  disabled={loadingMore}
                  onClick={() => void loadMore()}
                >
                  {loadingMore ? "正在加载…" : "加载更多"}
                </Button>
              )}
            </div>
          </section>
        </div>
        {drag.source &&
          createPortal(
            <>
              <div ref={drag.ghost} className="schedule-drag-ghost">
                {drag.source.kind === "library"
                  ? drag.source.action.title
                  : drag.source.title}
              </div>
              <div className="schedule-drop-marker" ref={drag.marker} />
            </>,
            queue.current?.closest(".sd-dialog-layer") ?? document.body,
          )}
        <span className="sr-only" aria-live="polite">
          执行列表共 {entries.length} 项
        </span>
      </Dialog>
      <ConfirmDialog
        open={discard}
        title="放弃更改？"
        confirmLabel="放弃更改"
        onClose={() => setDiscard(false)}
        onConfirm={() => {
          setDiscard(false);
          onClose();
        }}
      >
        <p>当前的时间和执行列表尚未保存。</p>
      </ConfirmDialog>
      <ConfirmDialog
        open={replacement !== null}
        title="更新执行时间？"
        confirmLabel="保存更改"
        onClose={() => setReplacement(null)}
        onConfirm={() => {
          if (!replacement) return;
          const value = replacement;
          setReplacement(null);
          void persist(value);
        }}
      >
        <p>此任务原有多个执行时刻，保存后仅按以下时间执行：</p>
        {replacement && <p>{scheduleTime(replacement.trigger)}</p>}
      </ConfirmDialog>
    </>
  );
}
