import { describe, expect, it, vi } from "vitest";
import { api } from "../ipc/api";
import { session, subscribeRuns } from ".";
import type { RunEvent, TaskInfo } from "../ipc/types";

vi.mock("../ipc/api", () => ({
  api: { conversation: vi.fn(), events: vi.fn() },
}));

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

  it("notifies run subscribers so the shell can update without polling", async () => {
    const id = "run-listener";
    const pending: ReturnType<typeof deferred<{ events: RunEvent[] }>>[] = [];
    vi.mocked(api.conversation).mockResolvedValue({ id, messages: [] });
    vi.mocked(api.events).mockImplementation(() => {
      const request = deferred<{ events: RunEvent[] }>();
      pending.push(request);
      return request.promise;
    });
    const seen: string[] = [];
    const stop = subscribeRuns((task) => {
      seen.push(`${task.id}:${task.state}`);
    });
    const entry = session(id);
    const unsubscribe = entry.subscribe(vi.fn());
    await flush();
    pending[0]!.resolve({
      events: [
        {
          sequence: 1,
          conversationId: id,
          runId: "run-z",
          kind: "run.created",
          data: { id: "run-z", state: "deciding" },
        },
      ],
    });
    await flush();
    pending[1]!.resolve({
      events: [
        {
          sequence: 2,
          conversationId: id,
          runId: "run-z",
          kind: "run.changed",
          data: { id: "run-z", state: "answered" },
        },
      ],
    });
    await flush();
    expect(seen).toEqual(["run-z:deciding", "run-z:answered"]);
    stop();
    unsubscribe();
    pending[2]?.resolve({ events: [] });
    await flush();
  });

  it("reloads the snapshot when the event cursor has expired", async () => {
    const id = "expired-cursor";
    const pending: ReturnType<
      typeof deferred<{ events: RunEvent[]; snapshotRequired?: boolean }>
    >[] = [];
    vi.mocked(api.conversation).mockResolvedValue({ id, messages: [] });
    vi.mocked(api.events).mockImplementation(() => {
      const request = deferred<{
        events: RunEvent[];
        snapshotRequired?: boolean;
      }>();
      pending.push(request);
      return request.promise;
    });
    const entry = session(id);
    const unsubscribe = entry.subscribe(vi.fn());
    await flush();
    pending[0]!.resolve({
      events: [
        {
          sequence: 9,
          conversationId: id,
          runId: "r",
          kind: "run.created",
          data: { id: "r", state: "deciding" },
        },
      ],
    });
    await flush();
    pending[1]!.resolve({ events: [], snapshotRequired: true });
    await flush();
    await flush();
    expect(api.events).toHaveBeenLastCalledWith(id, 0);
    pending[2]!.resolve({
      events: [
        {
          sequence: 12,
          conversationId: id,
          runId: "r",
          kind: "run.changed",
          data: { id: "r", state: "answered" },
        },
      ],
    });
    await flush();
    expect(entry.read().task?.state).toBe("answered");
    unsubscribe();
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
