import { useCallback, useEffect, useState } from "react";

import { api } from "./ipc/api";
import { Wordmark } from "./components/icons";
import { AppShell } from "./components/shell/AppShell";
import { DetailsPanel } from "./components/shell/DetailsPanel";
import { MotionSwitch } from "./components/controls/MotionSwitch";
import { ChatPage } from "./pages/chat/ChatPage";
import { ExtensionsPage } from "./pages/extensions/ExtensionsPage";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { TasksPage } from "./pages/tasks/TasksPage";
import { restoreTheme } from "./appearance";
import { restoreLocale } from "./appearance/locale";
import { isRunning, readError, subscribeRuns, watchTasks } from "./session";
import type { Bootstrap, TaskSummary } from "./ipc/types";
import { hostPluginEnabled } from "./ipc/providers";
import { useT } from "./i18n";

export type Page =
  | "chat"
  | "tasks"
  | "models"
  | "extensions"
  | "bridge"
  | "settings"
  | "help"
  | "sponsor";

export default function App() {
  const t = useT();
  const [page, setPage] = useState<Page>("chat");
  const [extensionsTab, setExtensionsTab] = useState<"skills" | "plugins">(
    "skills",
  );
  const [conversation, setConversation] = useState<string | undefined>(
    () => localStorage.getItem("sleepy-doll-active-conversation") ?? undefined,
  );
  const [bootstrap, setBootstrap] = useState<Bootstrap>();
  const [detailsOpen, setDetailsOpen] = useState(
    () => localStorage.getItem("sleepy-doll-details-open") !== "false",
  );
  const [selectedTask, setSelectedTask] = useState<string>();
  const [error, setError] = useState("");
  const [composingNewChat, setComposingNewChat] = useState(false);

  useEffect(() => {
    if (conversation)
      localStorage.setItem("sleepy-doll-active-conversation", conversation);
    else localStorage.removeItem("sleepy-doll-active-conversation");
  }, [conversation]);

  useEffect(() => {
    localStorage.setItem("sleepy-doll-details-open", String(detailsOpen));
  }, [detailsOpen]);

  const reload = useCallback(async () => {
    try {
      setBootstrap(await api.bootstrap());
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : readError(reason));
    }
  }, []);

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
          conversationId={conversation}
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
        if (next !== "chat") {
          setSelectedTask(undefined);
          setDetailsOpen(false);
        }
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
            onOpenHelp={() => setPage("help")}
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
              visiblePage === "help" ||
              visiblePage === "sponsor"
                ? visiblePage
                : "settings"
            }
            onSection={setPage}
            reload={reload}
          />
        )}
      </MotionSwitch>
    </AppShell>
  );
}
