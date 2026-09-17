import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import { AppShell } from "./components/AppShell";
import { DetailsPanel } from "./components/DetailsPanel";
import { MotionSwitch } from "./components/MotionSwitch";
import { ChatPage } from "./pages/ChatPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TasksPage } from "./pages/TasksPage";
import { readError, watchTasks } from "./session";
import type { Bootstrap, TaskSummary } from "./types";

export type Page =
  | "chat"
  | "tasks"
  | "models"
  | "extensions"
  | "bridge"
  | "settings"
  | "sponsor";

export default function App() {
  const [page, setPage] = useState<Page>("chat");
  const [conversation, setConversation] = useState<string | undefined>(
    () => localStorage.getItem("sleepy-doll-active-conversation") ?? undefined,
  );
  const [bootstrap, setBootstrap] = useState<Bootstrap>();
  const [detailsOpen, setDetailsOpen] = useState(
    () => localStorage.getItem("sleepy-doll-details-open") !== "false",
  );
  const [selectedTask, setSelectedTask] = useState<string>();
  const [error, setError] = useState("");

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
    document.documentElement.dataset.theme =
      localStorage.getItem("sleepy-doll-theme") ?? "light";
    document.documentElement.lang =
      localStorage.getItem("sleepy-doll-locale") === "en" ? "en" : "zh-CN";
    delete document.documentElement.dataset.reducedMotion;
    localStorage.removeItem("sleepy-doll-reduced-motion");
  }, [reload]);

  useEffect(() => {
    let stopped = false;
    let timer: number;
    const poll = async () => {
      try {
        const tasks = await api.tasks();
        if (stopped) return;
        // 后台会话的运行由订阅自己推进，切页不取消，也不抢当前焦点。
        watchTasks(tasks);
        setBootstrap((previous) =>
          previous ? { ...previous, tasks } : previous,
        );
      } catch {
        /* Conversation subscriptions surface connection failures. */
      }
      if (!stopped) timer = window.setTimeout(() => void poll(), 3000);
    };
    void poll();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, []);

  if (!bootstrap) {
    return (
      <div className="product-loading">
        <h1>Sleepy Doll</h1>
        <p>{error || "正在载入…"}</p>
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

  return (
    <AppShell
      bootstrap={bootstrap}
      page={page}
      conversationId={conversation}
      detailsOpen={detailsOpen}
      details={
        <DetailsPanel
          bootstrap={bootstrap}
          conversationId={conversation}
          selectedTask={selectedTask}
          onSelectTask={setSelectedTask}
          onOpenConversation={openConversation}
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
    >
      <MotionSwitch
        viewKey={
          page === "chat"
            ? "chat"
            : page === "tasks"
              ? "tasks"
              : page === "extensions"
                ? "extensions"
                : "settings"
        }
      >
        {page === "chat" ? (
          <ChatPage
            bootstrap={bootstrap}
            conversationId={conversation}
            onConversation={openConversation}
            reload={reload}
          />
        ) : page === "tasks" ? (
          <TasksPage
            bootstrap={bootstrap}
            reload={reload}
            onOpenConversation={openConversation}
            onOpenTask={(task: TaskSummary) => {
              setSelectedTask(task.id);
              setDetailsOpen(true);
            }}
          />
        ) : page === "extensions" ? (
          <ExtensionsPage bootstrap={bootstrap} reload={reload} />
        ) : (
          <SettingsPage
            bootstrap={bootstrap}
            section={page}
            onSection={setPage}
            reload={reload}
          />
        )}
      </MotionSwitch>
    </AppShell>
  );
}
