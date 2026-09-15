import { expect, it } from "vitest";

import { mockIpcUrl, eventsReadParams } from "./api";

it("uses same-origin ipc during Vite preview so localhost and 127.0.0.1 both work", () => {
  expect(mockIpcUrl(true)).toBe("/ipc");
  expect(mockIpcUrl(false)).toBe("http://127.0.0.1:47124/ipc");
});

it("does not long-poll events over HTTP mock", () => {
  expect(eventsReadParams("c1", 3, false)).toEqual({
    conversationId: "c1",
    after: 3,
  });
  expect(eventsReadParams("c1", 3, true)).toEqual({
    conversationId: "c1",
    after: 3,
    waitMs: 20_000,
  });
});
