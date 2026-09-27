import { useLayoutEffect, useRef, useState } from "react";
import { Dialog } from "../overlay/Dialog";
import { ChatPage } from "../../pages/chat/ChatPage";
import { api } from "../../ipc/api";
import { isRunning, readError, useSession } from "../../session";
import type { Bootstrap, TaskSummary } from "../../ipc/types";
import "./shortcut-configurator.css";

export function ShortcutConfigurator(props: {
  bootstrap: Bootstrap;
  open: boolean;
  onClose(): void;
  reload(): Promise<void>;
  target?: TaskSummary | undefined;
}) {
  return <ConfigurationSession key={props.target?.id ?? "add"} {...props} />;
}
function ConfigurationSession({
  bootstrap,
  open,
  onClose,
  reload,
  target,
}: {
  bootstrap: Bootstrap;
  open: boolean;
  onClose(): void;
  reload(): Promise<void>;
  target?: TaskSummary | undefined;
}) {
  const key = `sleepy-doll-shortcut-configuration:${target?.id ?? "add"}`;
  const [conversation, setConversation] = useState<string | undefined>(
    () => localStorage.getItem(key) ?? undefined,
  );
  useLayoutEffect(() => {
    if (open) setConversation(localStorage.getItem(key) ?? undefined);
  }, [open, key]);
  const data = useSession(conversation);
  const [closing, setClosing] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const visible = useRef(open);
  visible.current = open;
  const accepted = (id: string) => {
    localStorage.setItem(key, id);
    setConversation(id);
    if (!visible.current)
      void api
        .tasks()
        .then((runs) =>
          Promise.all(
            runs
              .filter((run) => run.conversationId === id && isRunning(run))
              .map((run) => api.cancelTask(run.id)),
          ),
        )
        .catch((reason) => setError(readError(reason)));
  };
  const close = async () => {
    if (closing) return;
    if (sending) {
      setError("正在提交配置，请稍候再关闭。");
      return;
    }
    setClosing(true);
    setError("");
    try {
      const runs = conversation
        ? (await api.tasks()).filter(
            (run) => run.conversationId === conversation && isRunning(run),
          )
        : [];
      await Promise.all(runs.map((run) => api.cancelTask(run.id)));
      visible.current = false;
      onClose();
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setClosing(false);
    }
  };
  return (
    <Dialog
      open={open}
      onClose={() => void close()}
      title={target ? `修改「${target.name}」` : "添加快捷任务"}
      subtitle="告诉 AI 要保存哪项任务；关闭会停止当前配置，保留内容供下次继续。"
      className="shortcut-configuration-dialog"
      footer={
        <>
          <span className="shortcut-config-state">
            {error ||
              (closing
                ? "正在停止配置…"
                : isRunning(data.task)
                  ? "正在配置，可在对话中补充要求"
                  : "保存后的任务会出现在列表中")}
          </span>
          {!target && conversation && !isRunning(data.task) && (
            <button
              className="subtle-action"
              onClick={() => {
                localStorage.removeItem(key);
                setConversation(undefined);
              }}
            >
              配置另一项
            </button>
          )}
          <button
            className="secondary-action"
            disabled={closing}
            onClick={() => void close()}
          >
            {isRunning(data.task) ? "停止配置并关闭" : "完成"}
          </button>
        </>
      }
    >
      <ChatPage
        bootstrap={bootstrap}
        conversationId={conversation}
        onConversation={setConversation}
        onSubmitted={accepted}
        onSending={setSending}
        reload={reload}
        configuration
        configurationTarget={target?.id}
        draftScope={key}
        onOpenModels={() => {
          onClose();
          window.dispatchEvent(new CustomEvent("sleepy-doll:open-models"));
        }}
      />
    </Dialog>
  );
}
