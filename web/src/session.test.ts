import { describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { session } from "./session";
import type { RunEvent, TaskInfo } from "./types";

vi.mock("./api", () => ({ api: { conversation: vi.fn(), events: vi.fn() } }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const flush = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

describe("application-owned conversation subscriptions", () => {
  it("keeps streaming without a mounted view and resumes from its cursor", async () => {
    const id = "background-test";
    const pending: ReturnType<typeof deferred<{ events: RunEvent[] }>>[] = [];
    vi.mocked(api.conversation).mockResolvedValue({ id, messages: [] });
    vi.mocked(api.events).mockImplementation(() => {
      const request = deferred<{ events: RunEvent[] }>();
      pending.push(request);
      return request.promise;
    });
    const entry = session(id);
    const unsubscribe = entry.subscribe(vi.fn());
    await flush();
    const run = {
      id: "run-a",
      conversationId: id,
      state: "deciding",
      prompt: "a",
    } as TaskInfo;
    const event = (
      sequence: number,
      kind: string,
      data: Record<string, unknown>,
    ): RunEvent => ({
      sequence,
      kind,
      data,
      conversationId: id,
      runId: run.id,
    });
    pending[0]!.resolve({
      events: [
        event(1, "run.created", run as unknown as Record<string, unknown>),
        event(2, "assistant.delta", { text: "first " }),
      ],
    });
    await flush();
    unsubscribe();
    pending[1]!.resolve({
      events: [event(3, "assistant.delta", { text: "second" })],
    });
    await flush();
    expect(entry.read().stream).toBe("first second");
    const resubscribe = entry.subscribe(vi.fn());
    expect(api.events).toHaveBeenLastCalledWith(id, 3);
    expect(entry.read().task?.state).toBe("deciding");
    pending[2]!.resolve({
      events: [
        event(4, "assistant.completed", {}),
        event(5, "run.changed", { ...run, state: "answered" }),
      ],
    });
    await flush();
    expect(entry.read().task?.state).toBe("answered");
    resubscribe();
    pending[3]?.resolve({ events: [] });
    await flush();
  });

  it("deduplicates overlapping event batches after a reconnect", async () => {
    const id = "overlapping-events";
    const pending: ReturnType<typeof deferred<{ events: RunEvent[] }>>[] = [];
    vi.mocked(api.conversation).mockResolvedValue({ id, messages: [] });
    vi.mocked(api.events).mockImplementation(() => {
      const request = deferred<{ events: RunEvent[] }>();
      pending.push(request);
      return request.promise;
    });
    const entry = session(id);
    const unsubscribe = entry.subscribe(vi.fn());
    await flush();
    const events: RunEvent[] = [
      {
        sequence: 1,
        conversationId: id,
        runId: "b",
        kind: "run.created",
        data: { id: "b", state: "deciding" },
      },
      {
        sequence: 2,
        conversationId: id,
        runId: "b",
        kind: "assistant.delta",
        data: { text: "once" },
      },
    ];
    pending[0]!.resolve({ events });
    await flush();
    pending[1]!.resolve({ events });
    await flush();
    expect(entry.read().stream).toBe("once");
    unsubscribe();
    pending[2]!.resolve({
      events: [
        {
          sequence: 3,
          conversationId: id,
          runId: "b",
          kind: "run.changed",
          data: { id: "b", state: "answered" },
        },
      ],
    });
    await flush();
  });
});
