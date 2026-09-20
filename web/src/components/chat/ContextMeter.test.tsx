import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ContextMeter } from "./ContextMeter";

afterEach(cleanup);

it("shows only the cache prefix confirmed by the agent", () => {
  const { rerender } = render(
    <ContextMeter used={12_000} window={200_000} cacheHit={0} />,
  );
  expect(screen.queryByText(/^缓存 /)).toBeNull();

  rerender(<ContextMeter used={18_000} window={200_000} cacheHit={12_000} />);
  expect(screen.getByText("缓存 12k")).toBeTruthy();
});
