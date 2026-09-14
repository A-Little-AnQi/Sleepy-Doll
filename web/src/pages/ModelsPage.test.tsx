import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ModelsPage } from "./ModelsPage";
import type { Bootstrap } from "../types";

vi.mock("../api", () => ({
  api: { saveModel: vi.fn(), useModel: vi.fn() },
}));

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(cleanup);

const bootstrap = {
  models: [
    {
      id: "a",
      name: "Model A",
      protocol: "openai-responses",
      model: "model-a",
      baseUrl: "https://a.example/v1",
      timeoutMs: 120000,
      active: true,
    },
    {
      id: "b",
      name: "Model B",
      protocol: "anthropic-messages",
      model: "model-b",
      baseUrl: "https://b.example/v1",
      timeoutMs: 90000,
      active: false,
    },
  ],
} as unknown as Bootstrap;

it("keeps unsaved drafts when switching between models", () => {
  render(<ModelsPage bootstrap={bootstrap} reload={vi.fn()} />);
  const name = screen.getByRole("textbox", { name: "名称" });
  fireEvent.change(name, { target: { value: "Unsaved A" } });

  const selector = screen.getByRole("combobox", { name: "选择模型" });
  fireEvent.click(selector);
  fireEvent.click(screen.getByRole("option", { name: /Model B/ }));
  expect(
    (screen.getByRole("textbox", { name: "名称" }) as HTMLInputElement).value,
  ).toBe("Model B");

  fireEvent.click(screen.getByRole("combobox", { name: "选择模型" }));
  fireEvent.click(screen.getByRole("option", { name: /Model A/ }));
  expect(
    (screen.getByRole("textbox", { name: "名称" }) as HTMLInputElement).value,
  ).toBe("Unsaved A");
});
