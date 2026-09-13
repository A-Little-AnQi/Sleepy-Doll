import { useEffect, useRef } from "react";
import "./Toast.css";

/**
 * 悬浮提示。用于操作失败或一句性的告知，不占页面版面 —— 页面正文只放状态。
 * 到时自动消失，点击立即消失。
 */
export function Toast({
  message,
  onDismiss,
  duration = 4000,
}: {
  message: string;
  onDismiss(): void;
  duration?: number;
}) {
  // 调用方通常传内联箭头函数，直接进依赖会让计时器每次渲染都重置、永远不触发。
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    const timer = setTimeout(() => dismiss.current(), duration);
    return () => clearTimeout(timer);
  }, [message, duration]);
  return (
    <div className="toast" role="alert" onClick={() => dismiss.current()}>
      {message}
    </div>
  );
}
