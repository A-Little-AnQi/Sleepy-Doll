import { expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SlidingTabs } from "./SlidingTabs";

it("changes the active tab without remounting the tablist", () => {
  const seen: string[] = [];
  const { rerender } = render(
    <SlidingTabs
      ariaLabel="设置分类"
      value="settings"
      onChange={(id) => seen.push(id)}
      items={[
        { id: "settings", name: "通用" },
        { id: "models", name: "模型" },
      ]}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "模型" }));
  expect(seen).toEqual(["models"]);
  rerender(
    <SlidingTabs
      ariaLabel="设置分类"
      value="models"
      onChange={(id) => seen.push(id)}
      items={[
        { id: "settings", name: "通用" },
        { id: "models", name: "模型" },
      ]}
    />,
  );
  expect(screen.getByRole("button", { name: "模型" }).getAttribute("aria-current")).toBe(
    "page",
  );
});

it("does not replay the pill motion when the active tab did not change", () => {
  const animate = vi.fn();
  HTMLElement.prototype.animate = animate;
  const view = (value: "settings" | "models") => (
    <SlidingTabs
      ariaLabel="设置分类"
      value={value}
      onChange={() => undefined}
      items={[
        { id: "settings", name: "通用" },
        { id: "models", name: "模型" },
      ]}
    />
  );
  const { rerender } = render(view("settings"));
  animate.mockClear();
  rerender(view("settings"));
  expect(animate).not.toHaveBeenCalled();
});
