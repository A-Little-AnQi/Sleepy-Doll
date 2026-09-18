import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  api: {
    windowDrag: vi.fn(),
    windowMinimize: vi.fn(),
    windowToggleMaximize: vi.fn(),
    windowClose: vi.fn(),
  },
}));

import { api } from "../api";
import { TitleBar } from "./TitleBar";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  delete document.documentElement.dataset.frameless;
  delete document.documentElement.dataset.maximized;
});

it("marks the document frameless so the layout makes room for the bar", () => {
  render(<TitleBar />);
  expect(document.documentElement.dataset.frameless).toBe("true");
});

it("leaves the left side empty so the sidebar carries the product name", () => {
  render(<TitleBar />);
  const bar = document.querySelector(".title-bar")!;
  expect(bar.textContent).toBe("");
  expect(bar.firstElementChild).toBe(document.querySelector(".title-bar-controls"));
});

it("drags the window from the empty area beside the controls", () => {
  render(<TitleBar />);
  fireEvent.pointerDown(document.querySelector(".title-bar")!, { button: 0 });
  expect(api.windowDrag).toHaveBeenCalled();
});

it("squares the window corners while maximized", () => {
  render(<TitleBar />);
  expect(document.documentElement.dataset.maximized).toBeUndefined();
  act(() => window.__sleepyDollWindow?.({ maximized: true }));
  expect(document.documentElement.dataset.maximized).toBe("true");
  act(() => window.__sleepyDollWindow?.({ maximized: false }));
  expect(document.documentElement.dataset.maximized).toBeUndefined();
});

it("drags the window from the bar but not from a control", () => {
  render(<TitleBar />);
  fireEvent.pointerDown(screen.getByLabelText("最小化"), { button: 0 });
  expect(api.windowDrag).not.toHaveBeenCalled();
  fireEvent.pointerDown(document.querySelector(".title-bar")!, { button: 0 });
  expect(api.windowDrag).toHaveBeenCalled();
});

it("runs each control", () => {
  render(<TitleBar />);
  fireEvent.click(screen.getByLabelText("最小化"));
  expect(api.windowMinimize).toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("最大化"));
  expect(api.windowToggleMaximize).toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("关闭"));
  expect(api.windowClose).toHaveBeenCalled();
});

it("toggles from a double click on the bar", () => {
  render(<TitleBar />);
  fireEvent.doubleClick(screen.getByLabelText("最小化"));
  expect(api.windowToggleMaximize).not.toHaveBeenCalled();
  fireEvent.doubleClick(document.querySelector(".title-bar")!);
  expect(api.windowToggleMaximize).toHaveBeenCalled();
});

it("offers restore instead of maximize while the window is maximized", () => {
  render(<TitleBar />);
  expect(screen.getByLabelText("最大化")).toBeTruthy();
  act(() => window.__sleepyDollWindow?.({ maximized: true }));
  expect(screen.queryByLabelText("最大化")).toBeNull();
  fireEvent.click(screen.getByLabelText("向下还原"));
  expect(api.windowToggleMaximize).toHaveBeenCalled();
});
