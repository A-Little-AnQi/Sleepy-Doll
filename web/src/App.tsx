import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import { DesignLanding } from "./components/DesignLanding";
import { BridgePage } from "./pages/BridgePage";
import { ChatPage } from "./pages/ChatPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { ModelsPage } from "./pages/ModelsPage";
import { RunLibraryPage } from "./pages/RunLibraryPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { Bootstrap } from "./types";

export type Page = "chat" | "library" | "models" | "extensions" | "bridge" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("chat");
  const [conversation, setConversation] = useState<string>();
  const [bootstrap, setBootstrap] = useState<Bootstrap>();
  const [error, setError] = useState("");
  const reload = useCallback(async () => {
    try { setBootstrap(await api.bootstrap()); setError(""); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }, []);
  useEffect(() => { void reload(); }, [reload]);
  if (!bootstrap) return <div className="product-loading"><h1>Sleepy Doll</h1><p>{error || "正在载入工作空间…"}</p>{error ? <button onClick={() => void reload()}>重试</button> : null}</div>;
  const openConversation = (id: string) => { setConversation(id); setPage("chat"); };
  return <DesignLanding bootstrap={bootstrap} page={page} onPage={setPage} onConversation={openConversation}
    onNew={() => { setConversation(undefined); setPage("chat"); }}
    onModel={async id => { await api.useModel(id); await reload(); }}
    onSend={async prompt => { const task = await api.submitTask(prompt); openConversation(task.conversationId); await reload(); }}>
    {page === "chat" ? (conversation ? <ChatPage bootstrap={bootstrap} conversationId={conversation} onConversation={openConversation} reload={reload}/> : null)
      : page === "library" ? <RunLibraryPage bootstrap={bootstrap} reload={reload} onRun={openConversation}/>
      : page === "models" ? <ModelsPage bootstrap={bootstrap} reload={reload}/>
      : page === "extensions" ? <ExtensionsPage bootstrap={bootstrap} reload={reload}/>
      : page === "bridge" ? <BridgePage bootstrap={bootstrap}/>
      : <SettingsPage/>}
  </DesignLanding>;
}
