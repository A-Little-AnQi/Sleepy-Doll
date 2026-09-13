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

it("dismisses on its own after the duration", async () => {
  vi.useFakeTimers();
  const onDismiss = vi.fn();
  render(<Toast message="稍后自动消失" onDismiss={onDismiss} duration={1000} />);
  // 调用方传的是内联箭头函数，计时器不能被每次渲染重置 —— 它在 1 秒后必须触发。
  await act(async () => {
    vi.advanceTimersByTime(1000);
  });
  expect(onDismiss).toHaveBeenCalled();
  vi.useRealTimers();
});
