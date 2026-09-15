import { expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MotionSwitch } from "./MotionSwitch";

it("replays enter motion when the view key changes", () => {
  const { rerender, container } = render(
    <MotionSwitch viewKey="one">第一页</MotionSwitch>,
  );
  const first = container.querySelector("[data-motion='page']");
  expect(first).toBeTruthy();
  expect(screen.getByText("第一页")).toBeTruthy();

  rerender(<MotionSwitch viewKey="two">第二页</MotionSwitch>);
  expect(screen.getByText("第二页")).toBeTruthy();
  expect(screen.queryByText("第一页")).toBeNull();
  const second = container.querySelector("[data-motion='page']");
  expect(second).toBeTruthy();
  expect(second).not.toBe(first);
});

it("uses panel motion for inner tab switches", () => {
  const { container } = render(
    <MotionSwitch viewKey="tasks" kind="panel">
      列表
    </MotionSwitch>,
  );
  expect(container.querySelector("[data-motion='panel']")).toBeTruthy();
});
