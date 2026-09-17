import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("./api", () => ({
  api: {
    bootstrap: vi.fn(),
  },
}));

import { api } from "./api";
import App from "./App";

afterEach(cleanup);

it("shows a compact primary retry when the local service is unreachable", async () => {
  vi.mocked(api.bootstrap).mockRejectedValue(new Error("无法连接本地服务。"));
  render(<App />);
  expect(await screen.findByText("无法连接本地服务。")).toBeTruthy();
  expect(screen.getByRole("button", { name: "重试" }).className).toContain(
    "primary-action",
  );
});
