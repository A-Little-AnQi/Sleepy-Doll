import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import { AppShell } from "./components/AppShell";
import { DetailsPanel } from "./components/DetailsPanel";
import { ChatPage } from "./pages/ChatPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TasksPage } from "./pages/TasksPage";
import { readError, watchTasks } from "./session";
import type { Bootstrap, TaskSummary } from "./types";

export type Page =
  "chat" | "tasks" | "models" | "extensions" | "bridge" | "settings";

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
  /** 新对话尚未落库时先记住用户挑的模型，发第一条消息时写进会话。 */
  const [pendingModel, setPendingModel] = useState<string | null>(null);
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
    document.documentElement.dataset.reducedMotion =
      localStorage.getItem("sleepy-doll-reduced-motion") ?? "false";
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
          <button type="button" onClick={() => void reload()}>
            重试
          </button>
        )}
      </div>
    );
  }

  const openConversation = (id: string) => {
    setConversation(id || undefined);
    setSelectedTask(undefined);
    // 换会话就该丢掉上一个新对话的暂存选择。
    setPendingModel(null);
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
        if (next !== "chat") setSelectedTask(undefined);
      }}
      onNew={() => {
        setConversation(undefined);
        setSelectedTask(undefined);
        setPendingModel(null);
        setPage("chat");
      }}
      onConversation={openConversation}
      onModel={async (id) => {
        if (!conversation) {
          setPendingModel(id);
          return;
        }
        await api.setConversationModel(conversation, id);
        await reload();
      }}
      pendingModel={pendingModel ?? undefined}
      onToggleDetails={() => setDetailsOpen(!detailsOpen)}
      reload={reload}
    >
      {page === "chat" ? (
        <ChatPage
          bootstrap={bootstrap}
          conversationId={conversation}
          modelId={pendingModel}
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
    </AppShell>
  );
}
