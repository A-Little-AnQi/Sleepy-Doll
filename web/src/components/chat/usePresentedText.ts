import { useEffect, useLayoutEffect, useReducer, useRef } from "react";
import type { MessageInfo } from "../../ipc/types";

interface TextBuffer {
  target: string;
  visible: number;
  credit: number;
  kind: "stream" | "answer" | "process";
}

/** 实时正文按帧输出，已加载的历史直接显示。 */
export function usePresentedText(
  messages: MessageInfo[],
  stream: string,
  runId?: string,
) {
  const buffers = useRef(new Map<string, TextBuffer>());
  const initialized = useRef(false);
  const frame = useRef(0);
  const lastFrame = useRef(0);
  const [, redraw] = useReducer((value: number) => value + 1, 0);
  const ordinals = new Map<string, number>();
  const activeKeys = new Set<string>();
  const revealingMessages = new Set<number>();
  const historical = !initialized.current;
  const bufferFor = (
    key: string,
    text: string,
    kind: TextBuffer["kind"],
    immediate = false,
  ) => {
    activeKeys.add(key);
    let buffer = buffers.current.get(key);
    if (!buffer) {
      buffer = {
        target: text,
        visible: immediate ? text.length : 0,
        credit: 0,
        kind,
      };
      buffers.current.set(key, buffer);
    } else {
      buffer.kind = kind;
      if (immediate) buffer.visible = text.length;
      if (!text.startsWith(buffer.target.slice(0, buffer.visible))) {
        let shared = 0;
        while (
          shared < buffer.visible &&
          shared < text.length &&
          text[shared] === buffer.target[shared]
        )
          shared += 1;
        const previous = text.charCodeAt(shared - 1);
        if (previous >= 0xd800 && previous <= 0xdbff) shared -= 1;
        buffer.visible = shared;
      }
      buffer.target = text;
    }
    return buffer;
  };
  const presentedMessages = messages.map((message, index) => {
    if (message.role !== "assistant") return message;
    const owner = message.runId ?? "history";
    const ordinal = ordinals.get(owner) ?? 0;
    ordinals.set(owner, ordinal + 1);
    const buffer = bufferFor(
      `${owner}:${ordinal}`,
      message.content,
      message.toolCalls?.length ? "process" : "answer",
      historical,
    );
    if (buffer.visible < buffer.target.length) revealingMessages.add(index);
    return {
      ...message,
      content: buffer.target.slice(0, buffer.visible),
    };
  });
  if (messages.length) initialized.current = true;
  const streamOwner = runId ?? messages.at(-1)?.runId ?? "history";
  const streamBuffer = stream
    ? bufferFor(
        `${streamOwner}:${ordinals.get(streamOwner) ?? 0}`,
        stream,
        "stream",
      )
    : undefined;
  const presentedStream = streamBuffer
    ? streamBuffer.target.slice(0, streamBuffer.visible)
    : "";
  for (const key of buffers.current.keys()) {
    if (!activeKeys.has(key)) buffers.current.delete(key);
  }
  const pending = [...buffers.current.values()].some(
    (buffer) => buffer.visible < buffer.target.length,
  );

  useLayoutEffect(() => {
    if (!pending || frame.current) return;
    lastFrame.current = performance.now();
    const advance = (now: number) => {
      const elapsed = Math.min(64, now - lastFrame.current);
      lastFrame.current = now;
      let changed = false;
      let remaining = false;
      for (const buffer of buffers.current.values()) {
        const backlog = buffer.target.length - buffer.visible;
        if (backlog <= 0) continue;
        const speed = Math.min(240, Math.max(80, backlog / 4));
        buffer.credit += (elapsed * speed) / 1000;
        const count = Math.min(12, Math.floor(buffer.credit));
        buffer.credit -= count;
        const end = Math.min(buffer.target.length, buffer.visible + count);
        if (end > buffer.visible) {
          buffer.visible = end;
          const previous = buffer.target.charCodeAt(end - 1);
          if (
            previous >= 0xd800 &&
            previous <= 0xdbff &&
            end < buffer.target.length
          )
            buffer.visible += 1;
          changed = true;
        }
        remaining ||= buffer.visible < buffer.target.length;
      }
      frame.current = remaining ? requestAnimationFrame(advance) : 0;
      if (changed) redraw();
    };
    frame.current = requestAnimationFrame(advance);
  }, [pending, messages, stream, runId]);

  useEffect(
    () => () => {
      cancelAnimationFrame(frame.current);
      frame.current = 0;
    },
    [],
  );
  return {
    messages: presentedMessages,
    stream: presentedStream,
    pending,
    revealingMessages,
  };
}
