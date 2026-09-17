import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ConfirmDialog } from "./ConfirmDialog";

afterEach(cleanup);

it("runs the action only after the user confirms", () => {
  const onConfirm = vi.fn();
  const onClose = vi.fn();
  render(
    <ConfirmDialog
      open
      title="删除模型"
      confirmLabel="删除模型"
      onConfirm={onConfirm}
      onClose={onClose}
    >
      <p>删除「主模型」。</p>
    </ConfirmDialog>,
  );
  expect(screen.getByRole("dialog", { name: "删除模型" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(onClose).toHaveBeenCalled();
  expect(onConfirm).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "删除模型" }));
  expect(onConfirm).toHaveBeenCalled();
});
