import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Select } from "./Select";

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("supports arrow-key selection, closes, and returns focus to its trigger", () => {
  const change = vi.fn();
  render(
    <Select
      label="model"
      value="a"
      onChange={change}
      options={[
        { value: "a", label: "Alpha" },
        { value: "b", label: "Beta" },
      ]}
    />,
  );
  const trigger = screen.getByRole("combobox");
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  expect(screen.getByRole("listbox")).toBeTruthy();
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  fireEvent.keyDown(trigger, { key: "Enter" });
  expect(change).toHaveBeenCalledWith("b");
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(document.activeElement).toBe(trigger);
  expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
});

it("dismisses without a change when clicking outside or pressing Escape", () => {
  const change = vi.fn();
  render(
    <Select
      label="model"
      value="a"
      onChange={change}
      options={[{ value: "a", label: "Alpha" }]}
    />,
  );
  const trigger = screen.getByRole("combobox");
  fireEvent.click(trigger);
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("listbox")).toBeNull();
  fireEvent.click(trigger);
  fireEvent.keyDown(trigger, { key: "Escape" });
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(change).not.toHaveBeenCalled();
});

it("keeps its menu in the same scroll context without moving the page", () => {
  render(
    <div data-testid="scroll-owner">
      <Select
        label="group"
        value="a"
        onChange={vi.fn()}
        options={[
          { value: "a", label: "Alpha" },
          { value: "b", label: "Beta" },
        ]}
      />
    </div>,
  );
  fireEvent.click(screen.getByRole("combobox"));
  const menu = screen.getByRole("listbox");
  expect(menu.closest('[data-ui="select-root"]')).toBeTruthy();
  fireEvent.scroll(screen.getByTestId("scroll-owner"));
  expect(screen.getByRole("listbox")).toBe(menu);
  expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
});
