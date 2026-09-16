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

it("hangs its menu off the body so no ancestor can cover it", () => {
  // 留在原地时，弹层会被祖先的层叠上下文困住：面板的入场动画会留下 transform，
  // 再高的 z-index 也只在那一层里有效，选项会被后面的兄弟元素盖住、点不中。
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
  expect(menu.parentElement).toBe(document.body);
  expect(menu.closest('[data-ui="select-root"]')).toBeNull();
  // 挂出去之后滚动容器不会带着它跑，也不该顺手滚动页面。
  fireEvent.scroll(screen.getByTestId("scroll-owner"));
  expect(screen.getByRole("listbox")).toBe(menu);
  expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
});

it("sizes the menu to the trigger when options are only labels", () => {
  const rect = {
    width: 128,
    height: 32,
    top: 400,
    bottom: 432,
    left: 80,
    right: 208,
    x: 80,
    y: 400,
    toJSON() {},
  };
  render(
    <Select
      label="语言"
      value="zh"
      onChange={vi.fn()}
      options={[
        { value: "zh", label: "简体中文" },
        { value: "en", label: "English" },
      ]}
    />,
  );
  const trigger = screen.getByRole("combobox");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(rect);
  fireEvent.click(trigger);
  expect(screen.getByRole("listbox").style.width).toBe("128px");
});

it("widens the menu when an option carries a description", () => {
  const rect = {
    width: 128,
    height: 32,
    top: 400,
    bottom: 432,
    left: 80,
    right: 208,
    x: 80,
    y: 400,
    toJSON() {},
  };
  render(
    <Select
      label="模型"
      value="a"
      onChange={vi.fn()}
      options={[
        { value: "a", label: "Alpha", description: "alpha-id" },
        { value: "b", label: "Beta", description: "beta-id" },
      ]}
    />,
  );
  const trigger = screen.getByRole("combobox");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(rect);
  fireEvent.click(trigger);
  expect(screen.getByRole("listbox").style.width).toBe("240px");
});

it("closes on Escape from anywhere, not only from the trigger", () => {
  render(
    <Select
      label="group"
      value="a"
      onChange={vi.fn()}
      options={[
        { value: "a", label: "Alpha" },
        { value: "b", label: "Beta" },
      ]}
    />,
  );
  fireEvent.click(screen.getByRole("combobox"));
  expect(screen.getByRole("listbox")).toBeTruthy();
  // 焦点可能已经落到弹层里，Esc 必须仍然关得掉。
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("listbox")).toBeNull();
});
