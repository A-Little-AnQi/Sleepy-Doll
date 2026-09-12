import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import { AppShell } from "./components/AppShell";
import { ChatPage } from "./pages/ChatPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { watchTasks } from "./session";
import { RunLibraryPage } from "./pages/RunLibraryPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { Bootstrap } from "./types";

export type Page =
  "chat" | "library" | "models" | "extensions" | "bridge" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("chat");
  const [conversation, setConversation] = useState<string | undefined>(
    () => localStorage.getItem("sleepy-doll-active-conversation") ?? undefined,
  );
  const [bootstrap, setBootstrap] = useState<Bootstrap>();
  const [error, setError] = useState("");

  useEffect(() => {
    if (conversation)
      localStorage.setItem("sleepy-doll-active-conversation", conversation);
    else localStorage.removeItem("sleepy-doll-active-conversation");
  }, [conversation]);

  const reload = useCallback(async () => {
    try {
      setBootstrap(await api.bootstrap());
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
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
    setConversation(id);
    setPage("chat");
  };

  return (
    <AppShell
      bootstrap={bootstrap}
      page={page}
      conversationId={conversation}
      onPage={setPage}
      onNew={() => {
        setConversation(undefined);
        setPage("chat");
      }}
      onConversation={openConversation}
      onModel={async (id) => {
        await api.useModel(id);
        await reload();
      }}
    >
      {page === "chat" ? (
        <ChatPage
          bootstrap={bootstrap}
          conversationId={conversation}
          onConversation={openConversation}
          reload={reload}
        />
      ) : page === "library" ? (
        <RunLibraryPage
          bootstrap={bootstrap}
          reload={reload}
          onRun={openConversation}
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
