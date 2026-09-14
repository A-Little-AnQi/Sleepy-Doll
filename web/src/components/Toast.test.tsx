import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { Toast } from "./Toast";

afterEach(cleanup);

it("shows the message and dismisses on click", () => {
  const onDismiss = vi.fn();
  const { container } = render(
    <Toast message="连接失败：桥未注入" onDismiss={onDismiss} />,
  );
  const toast = container.querySelector(".toast");
  expect(toast?.getAttribute("role")).toBe("alert");
  expect(toast?.textContent).toBe("连接失败：桥未注入");

  act(() => toast?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
  expect(onDismiss).toHaveBeenCalled();
});

it("runs the action instead of just closing when one is given", () => {
  const onDismiss = vi.fn();
  const onAction = vi.fn();
  const { container } = render(
    <Toast
      message="连接中断"
      duration={0}
      action={{ label: "刷新状态", onAction }}
      onDismiss={onDismiss}
    />,
  );
  const button = container.querySelector(".toast-action");
  expect(button?.textContent).toBe("刷新状态");
  act(() => button?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
  expect(onAction).toHaveBeenCalled();
  // 动作按钮只跑动作，不额外触发一次「点哪都关」。
  expect(onDismiss).toHaveBeenCalledTimes(1);
});

it("dismisses on its own after the duration", async () => {
  vi.useFakeTimers();
  const onDismiss = vi.fn();
  render(
    <Toast message="稍后自动消失" onDismiss={onDismiss} duration={1000} />,
  );
  // 调用方传的是内联箭头函数，计时器不能被每次渲染重置 —— 它在 1 秒后必须触发。
  await act(async () => {
    vi.advanceTimersByTime(1000);
  });
  expect(onDismiss).toHaveBeenCalled();
  vi.useRealTimers();
});
