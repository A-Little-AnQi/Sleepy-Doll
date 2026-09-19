import { useEffect, useRef } from "react";
import "./Toast.css";

/**
 * 悬浮提示。到时自动消失，点击立即消失；`duration` 为 0 表示一直留到用户处理。
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
  /** 需要用户执行明确动作（例如重连）时给出的按钮。 */
  action?: { label: string; onAction(): void };
}) {
  // 回调存进 ref，计时器不受重新渲染影响。
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
      // 模态 <dialog> 位于顶层，用 popover 才能浮在它上面。
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
            // 动作按钮自己处理，不触发整条的点击关闭。
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
