import { useEffect, useRef } from "react";
import "./Toast.css";

/**
 * 悬浮提示。用于操作失败或一句性的告知，不占页面版面 —— 页面正文只放状态。
 * 到时自动消失，点击立即消失；`duration` 为 0 表示要一直留到用户处理。
 */
export function Toast({
  message,
  onDismiss,
  duration = 4000,
  action,
}: {
  message: string;
  onDismiss(): void;
  duration?: number;
  /** 需要一个明确动作才能继续的错误（例如重连）时给出，不留一个只能干看的提示。 */
  action?: { label: string; onAction(): void };
}) {
  // 调用方通常传内联箭头函数，直接进依赖会让计时器每次渲染都重置、永远不触发。
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    element.showPopover?.();
    return () => {
      if (element.matches(":popover-open")) element.hidePopover?.();
    };
  }, []);
  useEffect(() => {
    if (duration <= 0) return;
    const timer = setTimeout(() => dismiss.current(), duration);
    return () => clearTimeout(timer);
  }, [message, duration]);
  return (
    <div
      ref={host}
      // 用顶层 popover 而不是 z-index：模态 <dialog> 打开时它属于顶层，
      // 再高的 z-index 也压不过，提示会被静默吞掉。
      popover="manual"
      className="toast"
      role="alert"
      onClick={() => dismiss.current()}
    >
      <span className="toast-message">{message}</span>
      {action && (
        <button
          className="toast-action"
          onClick={(event) => {
            // 点动作按钮就是处理它，不要再走一遍「点哪都关」。
            event.stopPropagation();
            action.onAction();
            dismiss.current();
          }}
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
