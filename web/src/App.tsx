import { useCallback, useEffect, useRef, useState } from "react";

import { api, subscribeNativeEvents } from "./ipc/api";
import { Wordmark } from "./components/icons";
import { AppShell } from "./components/shell/AppShell";
import { DetailsPanel } from "./components/shell/DetailsPanel";
import { Toast } from "./components/overlay/Toast";
import { MotionSwitch } from "./components/controls/MotionSwitch";
import { ChatPage } from "./pages/chat/ChatPage";
import { ExtensionsPage } from "./pages/extensions/ExtensionsPage";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { TasksPage } from "./pages/tasks/TasksPage";
import { UpdateDialog } from "./app/UpdateDialog";
import { ReleaseNotifications } from "./app/ReleaseSettings";
import { restoreTheme } from "./appearance";
import { restoreLocale } from "./appearance/locale";
import { isRunning, readError, subscribeRuns, watchTasks } from "./session";
import type { Bootstrap, TaskSummary } from "./ipc/types";
import { hostPluginEnabled } from "./ipc/providers";
import { useT } from "./i18n";
import { applyBridgeStatus, watchBridgeStatus } from "./session/bridge-status";

export type Page =
  | "chat"
  | "tasks"
  | "models"
  | "extensions"
  | "bridge"
  | "settings"
  | "sponsor";

export default function App() {
  const t = useT();
  // 每次启动都从对话页开始；要记住的是窗口在桌面上的位置与尺寸，由桌面壳负责。
  const [page, setPage] = useState<Page>("chat");
  const [extensionsTab, setExtensionsTab] = useState<"skills" | "plugins">(
    "skills",
  );
  const [conversation, setConversation] = useState<string | undefined>();
  const [bootstrap, setBootstrap] = useState<Bootstrap>();
  const bootstrapGeneration = useRef(0);
  // 快捷任务侧栏只在用户点开时出现，启动一律收起，也不记忆开关状态。
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [selectedTask, setSelectedTask] = useState<string>();
  const [error, setError] = useState("");
  const [panicNotice, setPanicNotice] = useState<number | null>(null);
  const [trayNotice, setTrayNotice] = useState("");
  const [releaseNotesTab, setReleaseNotesTab] = useState("changelog");
  const [releaseNotesOpen, setReleaseNotesOpen] = useState(
    // 更新弹窗：本地记录的版本与当前不同（包括第一次使用）就弹。
    () => localStorage.getItem("sleepy-doll-version") !== __APP_VERSION__,
  );
  const [composingNewChat, setComposingNewChat] = useState(false);

  useEffect(() => {
    if (!releaseNotesOpen) return;
    localStorage.setItem("sleepy-doll-version", __APP_VERSION__);
  }, [releaseNotesOpen]);

  const reload = useCallback(async () => {
    const generation = ++bootstrapGeneration.current;
    try {
      const value = await api.bootstrap();
      if (generation !== bootstrapGeneration.current) return;
      bootstrapGeneration.current += 1;
      setBootstrap(value);
      setError("");
    } catch (reason) {
      if (generation !== bootstrapGeneration.current) return;
      bootstrapGeneration.current += 1;
      setError(reason instanceof Error ? reason.message : readError(reason));
    }
  }, []);

  const bridgeVisible = bootstrap ? hostPluginEnabled(bootstrap) : false;
  useEffect(() => {
    if (!bridgeVisible) return;
    return watchBridgeStatus({
      read: api.bridgeStatus,
      generation: () => bootstrapGeneration.current,
      publish: (bridge) =>
        setBootstrap((previous) => applyBridgeStatus(previous, bridge)),
    });
  }, [bridgeVisible]);

  useEffect(() => {
    void reload();
    restoreTheme();
    restoreLocale();
    delete document.documentElement.dataset.reducedMotion;
    localStorage.removeItem("sleepy-doll-reduced-motion");
  }, [reload]);

  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === "visible") void reload();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [reload]);

  useEffect(
    // 急停热键：掐掉运行后壳会把窗口带回前台，这里只负责告知。
    () =>
      subscribeNativeEvents((name, payload) => {
        if (name === "panicStop") {
          setPanicNotice(
            payload && typeof payload === "object" && "stopped" in payload
              ? Number(payload.stopped)
              : 1,
          );
        }
        if (
          name === "trayError" &&
          payload &&
          typeof payload === "object" &&
          "message" in payload
        ) {
          setTrayNotice(String(payload.message));
        }
        if (name === "openSettings") setPage("settings");
        if (name === "openHelp") {
          setReleaseNotesTab("guide");
          setReleaseNotesOpen(true);
        }
      }),
    [],
  );

  useEffect(() => {
    // 「使用说明」「版本与更新」等入口：detail 指定要打开的标签页。
    const open = (event: Event) => {
      const tab = (event as CustomEvent<string>).detail;
      setReleaseNotesTab(typeof tab === "string" ? tab : "changelog");
      setReleaseNotesOpen(true);
    };
    window.addEventListener("sleepy-doll:open-release-notes", open);
    return () =>
      window.removeEventListener("sleepy-doll:open-release-notes", open);
  }, []);

  useEffect(() => {
    // 运行列表靠会话事件推进，不定时打 task.list。
    return subscribeRuns((task) => {
      setBootstrap((previous) => {
        if (!previous) return previous;
        const tasks = previous.tasks.some((item) => item.id === task.id)
          ? previous.tasks.map((item) => (item.id === task.id ? task : item))
          : [task, ...previous.tasks];
        return { ...previous, tasks };
      });
      if (isRunning(task)) watchTasks([task]);
    });
  }, []);

  useEffect(() => {
    if (bootstrap) watchTasks(bootstrap.tasks);
  }, [bootstrap]);

  useEffect(() => {
    const saved = () => void reload();
    const models = () => setPage("models");
    window.addEventListener("sleepy-doll:open-models", models);
    window.addEventListener("sleepy-doll:shortcut-saved", saved);
    return () => {
      window.removeEventListener("sleepy-doll:shortcut-saved", saved);
      window.removeEventListener("sleepy-doll:open-models", models);
    };
  }, [reload]);

  if (!bootstrap) {
    return (
      <div className="product-loading">
        <h1>
          <Wordmark />
        </h1>
        <p>{error || t.app.loading}</p>
        {error && (
          <button
            type="button"
            className="primary-action"
            onClick={() => void reload()}
          >
            重试
          </button>
        )}
      </div>
    );
  }

  const openConversation = (id: string) => {
    setConversation(id || undefined);
    setSelectedTask(undefined);
    setPage("chat");
  };

  const hostOn = hostPluginEnabled(bootstrap);
  const visiblePage = page === "bridge" && !hostOn ? "settings" : page;
  const openConnect = () => {
    setSelectedTask(undefined);
    setDetailsOpen(false);
    if (hostOn) setPage("bridge");
    else {
      setExtensionsTab("plugins");
      setPage("extensions");
    }
  };

  return (
    <AppShell
      bootstrap={bootstrap}
      page={visiblePage}
      conversationId={conversation}
      detailsOpen={detailsOpen}
      details={
        <DetailsPanel
          bootstrap={bootstrap}
          selectedTask={selectedTask}
          onSelectTask={setSelectedTask}
          onOpenConversation={openConversation}
          onConnectTools={openConnect}
          reload={reload}
          onClose={() => setDetailsOpen(false)}
        />
      }
      onPage={(next) => {
        setPage(next);
      }}
      onNew={() => {
        setConversation(undefined);
        setSelectedTask(undefined);
        setPage("chat");
      }}
      onConversation={openConversation}
      onToggleDetails={() => setDetailsOpen(!detailsOpen)}
      reload={reload}
      composingNewChat={composingNewChat}
    >
      <MotionSwitch
        viewKey={
          visiblePage === "chat"
            ? "chat"
            : visiblePage === "tasks"
              ? "tasks"
              : visiblePage === "extensions"
                ? "extensions"
                : "settings"
        }
      >
        {visiblePage === "chat" ? (
          <ChatPage
            bootstrap={bootstrap}
            conversationId={conversation}
            onConversation={openConversation}
            reload={reload}
            onComposerDraft={setComposingNewChat}
            onOpenModels={() => setPage("models")}
          />
        ) : visiblePage === "tasks" ? (
          <TasksPage
            bootstrap={bootstrap}
            reload={reload}
            onOpenConversation={openConversation}
            onOpenTask={(task: TaskSummary) => {
              setSelectedTask(task.id);
              setDetailsOpen(true);
            }}
            onConnectTools={openConnect}
          />
        ) : visiblePage === "extensions" ? (
          <ExtensionsPage
            bootstrap={bootstrap}
            reload={reload}
            tab={extensionsTab}
            onTab={setExtensionsTab}
            onOpenHost={() => setPage("bridge")}
          />
        ) : (
          <SettingsPage
            bootstrap={bootstrap}
            section={
              visiblePage === "models" ||
              visiblePage === "bridge" ||
              visiblePage === "sponsor"
                ? visiblePage
                : "settings"
            }
            onSection={setPage}
            reload={reload}
          />
        )}
      </MotionSwitch>
      {panicNotice !== null && (
        <Toast
          message={
            panicNotice === 0
              ? t.app.panicStopIdle
              : bootstrap.tasks.some(isRunning)
                ? t.app.panicStop
                : t.app.panicStopDone
          }
          duration={6000}
          onDismiss={() => setPanicNotice(null)}
        />
      )}
      {trayNotice && (
        <Toast
          message={trayNotice}
          duration={6000}
          onDismiss={() => setTrayNotice("")}
        />
      )}
      <UpdateDialog
        open={releaseNotesOpen}
        initialTab={releaseNotesTab}
        onClose={() => setReleaseNotesOpen(false)}
      />
      <ReleaseNotifications />
    </AppShell>
  );
}
