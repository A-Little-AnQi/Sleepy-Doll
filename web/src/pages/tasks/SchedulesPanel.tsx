import { useCallback, useEffect, useRef, useState } from "react";
import { Button, EmptyState, SearchField } from "@sleepy-doll/ui";
import { Switch } from "@sleepy-doll/ui/controls/Switch";
import { Dialog } from "@sleepy-doll/ui/overlay/Dialog";
import { ConfirmDialog } from "@sleepy-doll/ui/overlay/ConfirmDialog";
import { useMessage } from "@sleepy-doll/ui/overlay/Message";
import { HistoryIcon, PlusIcon } from "@sleepy-doll/ui/icons";
import { useFlipList } from "@sleepy-doll/ui/motion";
import { api } from "../../ipc/api";
import {
  scheduler,
  scheduleTime,
  type Schedule,
  type ScheduleOccurrence,
} from "../../ipc/scheduler";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import { readError } from "../../session";
import { useT } from "../../i18n";
import { ScheduleEditor } from "./ScheduleEditor";
import { ScheduleMenu } from "./ScheduleMenu";
import "./SchedulesPanel.css";

const stateLabels: Record<string, string> = {
  queued: "等待执行",
  running: "正在执行",
  submitted: "已提交",
  completed: "已完成",
  succeeded: "成功",
  success: "成功",
  failed: "失败",
  timeout: "执行超时",
  timedOut: "超时",
  unknown: "结果未确认",
  cancelled: "已停止",
  cancelling: "正在停止",
  skipped: "未执行",
  partial: "部分完成",
};
const stateLabel = (state: string) => stateLabels[state] ?? "待核对";
export function SchedulesPanel({
  bootstrap,
  reload,
  initialTask,
  onConsumed,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  initialTask?: TaskSummary | undefined;
  onConsumed(): void;
}) {
  const t = useT();
  const [items, setItems] = useState<Schedule[]>([]),
    [loading, setLoading] = useState(true);
  const [query, setQuery] = useState(""),
    [busy, setBusy] = useState<Set<string>>(() => new Set());
  const message = useMessage();
  const reportedIssues = useRef(new Map<string, string>());
  const pending = useRef(new Set<string>());
  const [editor, setEditor] = useState<{
    key: string;
    open: boolean;
    schedule: Schedule | null;
    task?: TaskSummary;
  } | null>(null);
  const [menu, setMenuState] = useState(""),
    [removing, setRemoving] = useState<Schedule | null>(null);
  const [renaming, setRenaming] = useState<Schedule | null>(null),
    [name, setName] = useState("");
  const [history, setHistory] = useState<{
    open: boolean;
    id: string;
    name: string;
    items: ScheduleOccurrence[];
    loading: boolean;
  }>({ open: false, id: "", name: "", items: [], loading: false });
  const list = useRef<HTMLDivElement>(null);
  const menuId = useRef("");
  const blockedClick = useRef(false);
  const [menuPoint, setMenuPoint] = useState<{ x: number; y: number } | null>(
    null,
  );
  const setMenu = useCallback((id: string) => {
    menuId.current = id;
    setMenuState(id);
  }, []);
  const closeMenu = useCallback(() => setMenu(""), [setMenu]);
  useEffect(() => {
    const capture = (event: PointerEvent) => {
      blockedClick.current =
        event.button === 0 &&
        !!menuId.current &&
        !(event.target as Element).closest(
          ".schedule-row-menu,.schedule-action-menu",
        );
    };
    document.addEventListener("pointerdown", capture, true);
    return () => document.removeEventListener("pointerdown", capture, true);
  }, []);
  const plugin = bootstrap.plugins.find(
    (plugin) => plugin.manifest.id === "windows-scheduler",
  );
  const enabled = plugin?.configuredEnabled ?? plugin?.status === "enabled";
  const load = async () => {
    const result = await scheduler.list();
    acceptItems(result.items);
  };
  const openEditor = (schedule: Schedule | null = null, task?: TaskSummary) =>
    setEditor({
      key: crypto.randomUUID(),
      open: true,
      schedule,
      ...(task ? { task } : {}),
    });
  const acceptItems = (values: Schedule[]) => {
    setItems(values);
    const issue = values.find(
      (item) =>
        item.error &&
        reportedIssues.current.get(item.id) !==
          `${item.revision}:${item.error}`,
    );
    reportedIssues.current = new Map(
      values
        .filter((item) => item.error)
        .map((item) => [item.id, `${item.revision}:${item.error}`]),
    );
    if (issue) message.error(`「${issue.name}」：${issue.error}`);
  };
  useEffect(() => {
    let alive = true;
    void scheduler
      .list()
      .then((result) => {
        if (alive) acceptItems(result.items);
      })
      .catch((reason) => {
        if (alive) message.error(readError(reason));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);
  useEffect(() => {
    if (!initialTask) return;
    const revision = initialTask.publishedRevision;
    if (!revision) {
      message.error(`「${initialTask.name}」暂不支持定时执行`);
      onConsumed();
      return;
    }
    let alive = true;
    void scheduler
      .preview({
        name: initialTask.name,
        trigger: { kind: "daily", localTimes: ["00:00"] },
        entries: [
          {
            binding: {
              kind: "shortcut",
              shortcutId: initialTask.id,
              expectedPublishedRevision: revision,
            },
          },
        ],
      })
      .then(() => {
        if (alive) openEditor(null, initialTask);
      })
      .catch(() => {
        if (alive) message.error(`「${initialTask.name}」暂不支持定时执行`);
      })
      .finally(() => {
        if (alive) onConsumed();
      });
    return () => {
      alive = false;
    };
  }, [initialTask]);
  useEffect(() => {
    const visible = () => {
      if (document.visibilityState === "visible")
        void load().catch((reason) => message.error(readError(reason)));
    };
    document.addEventListener("visibilitychange", visible);
    return () => document.removeEventListener("visibilitychange", visible);
  }, []);
  const enable = async () => {
    if (!enabled) {
      await api.setPluginEnabled("windows-scheduler", true);
      await reload();
    }
  };
  const act = async (id: string, action: () => Promise<unknown>) => {
    if (pending.current.has(id)) return;
    pending.current.add(id);
    setBusy((current) => new Set(current).add(id));
    message.dismiss();
    try {
      await action();
      await load();
    } catch (reason) {
      const title =
        items.find((item) => item.id === id)?.name ??
        (history.items.some((item) => item.id === id)
          ? history.name
          : undefined);
      message.error(`${title ? `「${title}」：` : ""}${readError(reason)}`);
      await load().catch(() => {});
    } finally {
      pending.current.delete(id);
      setBusy((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };
  const toggle = (schedule: Schedule, value = !schedule.enabled) => {
    if (blockedClick.current) return;
    if (
      value &&
      schedule.trigger.kind === "once" &&
      Date.parse(schedule.trigger.at) <= Date.now()
    ) {
      message.error(`「${schedule.name}」的执行时间已过，请修改时间`, {
        label: "修改时间",
        onAction: () => openEditor(schedule),
      });
      return;
    }
    void act(schedule.id, async () => {
      await enable();
      return scheduler.enabled(schedule, value);
    });
  };
  const showHistory = async (schedule: Schedule) => {
    setHistory({
      open: true,
      id: schedule.id,
      name: schedule.name,
      items: [],
      loading: true,
    });
    try {
      const result = await scheduler.history(schedule.id);
      setHistory((current) =>
        current.id === schedule.id
          ? { ...current, items: result.items, loading: false }
          : current,
      );
    } catch (reason) {
      message.error(`「${schedule.name}」：${readError(reason)}`, {
        label: "重新读取",
        onAction: () => {
          void showHistory(schedule);
        },
      });
      setHistory((current) =>
        current.id === schedule.id
          ? { ...current, open: false, loading: false }
          : current,
      );
    }
  };
  useEffect(() => {
    if (
      !history.open ||
      !history.items.some((item) =>
        ["queued", "running", "cancelling"].includes(item.state),
      )
    )
      return;
    let active = true;
    const timer = setTimeout(() => {
      void scheduler
        .history(history.id)
        .then((result) => {
          if (active)
            setHistory((current) => ({ ...current, items: result.items }));
        })
        .catch((reason) => {
          if (active)
            message.error(`「${history.name}」：${readError(reason)}`);
        });
    }, 2000);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [history]);
  const needle = query.trim().toLowerCase();
  const visible = items.filter((item) =>
    `${item.name} ${item.entries.map((entry) => entry.title).join(" ")} ${scheduleTime(item.trigger)}`
      .toLowerCase()
      .includes(needle),
  );
  const visibleIds = visible.map((item) => item.id).join(",");
  useFlipList(list, visibleIds);
  useEffect(() => {
    if (menu && !visible.some((item) => item.id === menu)) closeMenu();
  }, [menu, visibleIds, closeMenu]);
  return (
    <div
      className="schedule-panel"
      onKeyDownCapture={() => {
        blockedClick.current = false;
      }}
    >
      <div className="page-title">
        <h2>{t.nav.schedules}</h2>
        <Button variant="primary" onClick={() => openEditor()}>
          <PlusIcon className="button-icon" />
          新建定时任务
        </Button>
      </div>
      <div className="list-toolbar page-list-toolbar schedule-toolbar">
        <SearchField
          aria-label="搜索定时任务"
          placeholder="搜索定时任务"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      <div className="schedule-list-region">
        {loading ? (
          <div className="schedule-loading">正在读取…</div>
        ) : visible.length ? (
          <div ref={list} className="schedule-list">
            {visible.map((schedule) => {
              const summary = schedule.entries
                .map((entry) => entry.title)
                .join(" → ");
              return (
                <div
                  className="schedule-row list-item"
                  key={schedule.id}
                  data-sd-slot=""
                  data-sd-id={schedule.id}
                  data-context-menu-owner="schedule"
                  data-busy={busy.has(schedule.id) || undefined}
                  data-list-surface="true"
                  onClick={(event) => {
                    if (
                      event.button !== 0 ||
                      (event.target as Element).closest(
                        "button,a,input,select,textarea,[role=menu]",
                      )
                    )
                      return;
                    const selection = window.getSelection();
                    if (
                      selection &&
                      !selection.isCollapsed &&
                      (event.currentTarget.contains(selection.anchorNode) ||
                        event.currentTarget.contains(selection.focusNode))
                    )
                      return;
                    toggle(schedule);
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    setMenuPoint(
                      event.clientX || event.clientY
                        ? { x: event.clientX, y: event.clientY }
                        : null,
                    );
                    setMenu(schedule.id);
                  }}
                >
                  <div className="schedule-row-main">
                    <span className="schedule-row-title">{schedule.name}</span>
                    {summary !== schedule.name &&
                      schedule.entries
                        .map((entry) => entry.title)
                        .join("、") !== schedule.name && (
                        <p title={summary}>{summary}</p>
                      )}
                    <span className="schedule-row-time">
                      {scheduleTime(schedule.trigger)}
                    </span>
                  </div>
                  <Switch
                    checked={schedule.enabled}
                    label={`${schedule.name} 定时执行`}
                    disabled={busy.has(schedule.id)}
                    onChange={(value) => toggle(schedule, value)}
                  />
                  <ScheduleMenu
                    open={menu === schedule.id}
                    point={menuPoint}
                    label={`${schedule.name} 的更多操作`}
                    onToggle={() => {
                      setMenuPoint(null);
                      setMenu(
                        menuId.current === schedule.id ? "" : schedule.id,
                      );
                    }}
                    onClose={closeMenu}
                  >
                    {schedule.error && (
                      <button
                        role="menuitem"
                        onClick={() => {
                          setMenu("");
                          message.error(
                            `「${schedule.name}」：${schedule.error}`,
                          );
                        }}
                      >
                        查看问题
                      </button>
                    )}
                    <button
                      role="menuitem"
                      disabled={busy.has(schedule.id)}
                      onClick={() => {
                        setMenu("");
                        openEditor(schedule);
                      }}
                    >
                      编辑
                    </button>
                    <button
                      role="menuitem"
                      disabled={busy.has(schedule.id)}
                      onClick={() => {
                        setMenu("");
                        setRenaming(schedule);
                        setName(schedule.name);
                      }}
                    >
                      重命名
                    </button>
                    <button
                      role="menuitem"
                      disabled={busy.has(schedule.id)}
                      onClick={() => {
                        setMenu("");
                        const key = crypto.randomUUID();
                        void act(schedule.id, async () => {
                          await enable();
                          await scheduler.run(schedule, key);
                          await showHistory(schedule);
                        });
                      }}
                    >
                      立即运行
                    </button>
                    <button
                      role="menuitem"
                      onClick={() => {
                        setMenu("");
                        void showHistory(schedule);
                      }}
                    >
                      运行记录
                    </button>
                    <button
                      role="menuitem"
                      disabled={busy.has(schedule.id)}
                      onClick={() => {
                        setMenu("");
                        setRemoving(schedule);
                      }}
                    >
                      删除
                    </button>
                  </ScheduleMenu>
                </div>
              );
            })}
          </div>
        ) : (
          <EmptyState
            icon={<HistoryIcon />}
            title={<>{needle ? "没有匹配的定时任务" : "还没有定时任务"}</>}
          >
            {needle && (
              <Button variant="subtle" onClick={() => setQuery("")}>
                清除搜索
              </Button>
            )}
          </EmptyState>
        )}
      </div>
      {editor && (
        <ScheduleEditor
          key={editor.key}
          open={editor.open}
          schedule={editor.schedule}
          initialTask={editor.task}
          enable={enable}
          onClose={() =>
            setEditor((current) =>
              current ? { ...current, open: false } : null,
            )
          }
          onSaved={load}
        />
      )}
      <ConfirmDialog
        open={removing !== null}
        title="删除定时任务"
        confirmLabel="删除"
        busy={removing !== null && busy.has(removing.id)}
        onClose={() => setRemoving(null)}
        onConfirm={() => {
          if (removing)
            void act(removing.id, async () => {
              await enable();
              await scheduler.remove(removing);
              setRemoving(null);
            });
        }}
      >
        <p>删除「{removing?.name}」的时间安排？原快捷任务会保留。</p>
      </ConfirmDialog>
      <ConfirmDialog
        open={renaming !== null}
        title="重命名定时任务"
        confirmLabel="保存"
        busy={renaming !== null && busy.has(renaming.id)}
        onClose={() => setRenaming(null)}
        onConfirm={() => {
          if (renaming && name.trim())
            void act(renaming.id, async () => {
              await enable();
              await scheduler.update(renaming, {
                name: name.trim(),
                trigger: renaming.trigger,
                entries: renaming.entries.map((entry) => ({
                  entryId: entry.entryId,
                  binding: entry.binding,
                })),
              });
              setRenaming(null);
            });
        }}
      >
        <label className="shortcut-name-field">
          名称
          <input
            value={name}
            maxLength={80}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
      </ConfirmDialog>
      <Dialog
        compact
        open={history.open}
        title={`${history.name} · 运行记录`}
        onClose={() => setHistory((current) => ({ ...current, open: false }))}
        className="schedule-history-dialog"
      >
        {history.loading ? (
          <p className="schedule-history-loading">正在读取…</p>
        ) : history.items.length ? (
          history.items.map((occurrence) => (
            <section className="schedule-occurrence" key={occurrence.id}>
              <header>
                <time>{new Date(occurrence.createdAt).toLocaleString()}</time>
                {!(
                  occurrence.results.length === 1 &&
                  occurrence.results[0]?.state === occurrence.state
                ) && <span>{stateLabel(occurrence.state)}</span>}
                {["queued", "running"].includes(occurrence.state) && (
                  <Button
                    variant="subtle"
                    onClick={() =>
                      void act(occurrence.id, async () => {
                        await scheduler.stop(occurrence.id);
                        await showHistory(occurrence.schedule);
                      })
                    }
                  >
                    停止
                  </Button>
                )}
              </header>
              {occurrence.error && (
                <p className="schedule-error">{occurrence.error}</p>
              )}
              {occurrence.results.map((entry, index) => (
                <div key={entry.entryId}>
                  <span>
                    {index + 1}. {entry.title}
                  </span>
                  <span>{stateLabel(entry.state)}</span>
                  {entry.error && <small>{entry.error}</small>}
                </div>
              ))}
            </section>
          ))
        ) : (
          <EmptyState icon={<HistoryIcon />} title={<>还没有运行记录</>} />
        )}
      </Dialog>
    </div>
  );
}
