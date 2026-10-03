import test from "node:test";
import assert from "node:assert/strict";
import worker, { validateEvent } from "./worker.mjs";
const event = {
  clientId: "12345678-1234-1234-1234-123456789abc",
  sessionId: 1,
  event: "app_start",
  version: "0.0.1",
  channel: "test",
  platform: "windows",
};
test("事件白名单只转发基础字段", () => {
  const output = validateEvent({
    ...event,
    prompt: "private",
    apiKey: "secret",
  });
  assert.equal(output.events[0].params.app_version, "0.0.1");
  assert(!JSON.stringify(output).includes("private"));
  assert.equal(validateEvent({ ...event, event: "conversation" }), null);
});
test("统计拒绝其他来源与无效事件", async () => {
  const response = await worker.fetch(
    new Request("https://sleepy-doll.restless-nh3.com/api/events", {
      method: "POST",
      headers: {
        Origin: "https://other.example",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(event),
    }),
    {},
  );
  assert.equal(response.status, 403);
});
test("没有版本时返回 404", async () => {
  const response = await worker.fetch(
    new Request("https://sleepy-doll.restless-nh3.com/api/releases/stable"),
    { RELEASES: { get: async () => null } },
  );
  assert.equal(response.status, 404);
});
