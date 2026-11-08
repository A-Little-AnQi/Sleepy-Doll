import { useDeferredValue, useMemo } from "react";
import type { MessageInfo } from "../../ipc/types";

/** 文字跟随实际数据；后台 Markdown 渲染可被交互打断，不用计时器逐字追赶。 */
export function usePresentedText(
  messages: MessageInfo[],
  stream: string,
  runId?: string,
) {
  const owner = runId ?? messages.at(-1)?.runId ?? "history";
  const current = useMemo(() => ({ owner, text: stream }), [owner, stream]);
  const deferred = useDeferredValue(current);
  // 清空、持久化和更换运行立即生效，不把旧轮次的延迟文本带入新轮次。
  const visible = !stream || deferred.owner !== owner ? stream : deferred.text;
  const revealingMessages = useMemo(() => new Set<number>(), []);
  return {
    messages,
    stream: visible,
    pending: Boolean(stream && visible !== stream),
    revealingMessages,
  };
}
