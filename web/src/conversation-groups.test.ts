import { afterEach, expect, it } from "vitest";
import {
  byRecent,
  createGroup,
  deleteGroup,
  layoutAfterDrop,
  moveGroupBefore,
  moveInOrder,
  orderConversations,
  placeConversation,
  readLayout,
  setMembership,
  splitConversations,
  writeLayout,
  type GroupLayout,
} from "./conversation-groups";

afterEach(() => localStorage.clear());

const empty: GroupLayout = { groups: [], membership: {}, order: [] };

it("creates a group and can assign a conversation into it", () => {
  const withGroup = createGroup(empty, "路线");
  expect(withGroup.groups).toHaveLength(1);
  expect(withGroup.groups[0]?.name).toBe("路线");
  const grouped = setMembership(withGroup, "conv-1", withGroup.groups[0]!.id);
  expect(grouped.membership["conv-1"]).toBe(withGroup.groups[0]?.id);
});

it("deleting a group ungroups its conversations", () => {
  const created = createGroup(empty, "临时");
  const id = created.groups[0]!.id;
  const assigned = setMembership(created, "conv-1", id);
  const removed = deleteGroup(assigned, id);
  expect(removed.groups).toHaveLength(0);
  expect(removed.membership["conv-1"]).toBeUndefined();
});

it("reorders groups by moving an item in front of another", () => {
  const first = createGroup(empty, "甲");
  const second = createGroup(first, "乙");
  const moved = moveGroupBefore(
    second,
    second.groups[1]!.id,
    second.groups[0]!.id,
  );
  expect(moved.groups.map((group) => group.name)).toEqual(["乙", "甲"]);
});

it("splits conversations into grouped and ungrouped lists", () => {
  const created = createGroup(empty, "工作");
  const id = created.groups[0]!.id;
  const layout = setMembership(created, "a", id);
  const { groups, ungrouped } = splitConversations(
    [{ id: "a" }, { id: "b" }],
    layout,
  );
  expect(groups[0]?.items.map((item) => item.id)).toEqual(["a"]);
  expect(ungrouped.map((item) => item.id)).toEqual(["b"]);
});

it("splitConversations follows layout.order", () => {
  const layout: GroupLayout = {
    groups: [{ id: "g", name: "工作", collapsed: false }],
    membership: { a: "g", c: "g" },
    order: ["c", "a", "b"],
  };
  const { groups, ungrouped } = splitConversations(
    [{ id: "a" }, { id: "b" }, { id: "c" }],
    layout,
  );
  expect(groups[0]?.items.map((item) => item.id)).toEqual(["c", "a"]);
  expect(ungrouped.map((item) => item.id)).toEqual(["b"]);
});

it("round-trips layout through localStorage", () => {
  const layout = createGroup(empty, "收藏");
  writeLayout(layout);
  expect(readLayout().groups[0]?.name).toBe("收藏");
});

it("puts recently updated conversations first", () => {
  expect(
    byRecent([
      { id: "old", updatedAt: "2026-09-01T10:00:00Z" },
      { id: "new", updatedAt: "2026-09-15T12:00:00Z" },
      { id: "mid", updatedAt: "2026-09-10T08:00:00Z" },
    ]).map((item) => item.id),
  ).toEqual(["new", "mid", "old"]);
});

it("does not swap two conversations when streaming refreshes updatedAt", () => {
  const first = orderConversations(
    [
      { id: "a", updatedAt: "2026-09-01T10:00:00Z" },
      { id: "b", updatedAt: "2026-09-15T12:00:00Z" },
    ],
    [],
    [],
    [],
  );
  expect(first.map((item) => item.id)).toEqual(["b", "a"]);
  const streaming = orderConversations(
    [
      { id: "a", updatedAt: "2026-09-15T12:00:02Z" },
      { id: "b", updatedAt: "2026-09-15T12:00:01Z" },
    ],
    ["b", "a"],
    ["a", "b"],
    ["a", "b"],
  );
  expect(streaming.map((item) => item.id)).toEqual(["b", "a"]);
});

it("promotes a conversation only when the user starts a new run", () => {
  const bumped = orderConversations(
    [
      { id: "a", updatedAt: "2026-09-15T12:00:03Z" },
      { id: "b", updatedAt: "2026-09-15T12:00:02Z" },
    ],
    ["b", "a"],
    ["a"],
    [],
  );
  expect(bumped.map((item) => item.id)).toEqual(["a", "b"]);
});

it("puts a newly created conversation on top without reshuffling the rest", () => {
  const withNew = orderConversations(
    [
      { id: "a", updatedAt: "2026-09-01T10:00:00Z" },
      { id: "b", updatedAt: "2026-09-15T12:00:00Z" },
      { id: "c", createdAt: "2026-09-15T13:00:00Z" },
    ],
    ["b", "a"],
    [],
    [],
  );
  expect(withNew.map((item) => item.id)).toEqual(["c", "b", "a"]);
});

it("moves a conversation before another and into a group", () => {
  const created = createGroup(empty, "工作");
  const groupId = created.groups[0]!.id;
  const placed = placeConversation(
    { ...created, order: ["a", "b", "c"] },
    "c",
    groupId,
    "a",
  );
  expect(placed.membership["c"]).toBe(groupId);
  expect(placed.order).toEqual(["c", "a", "b"]);
});

it("appends when moveInOrder has no before id", () => {
  expect(moveInOrder(["a", "b", "c"], "a", null)).toEqual(["b", "c", "a"]);
});

it("layoutAfterDrop reorders a conversation before another", () => {
  const layout: GroupLayout = {
    groups: [],
    membership: {},
    order: ["a", "b", "c"],
  };
  const moved = layoutAfterDrop(
    layout,
    { kind: "conversation", id: "c" },
    { target: "conversation", id: "a", groupId: null, edge: "before" },
  );
  expect(moved.order).toEqual(["c", "a", "b"]);
});

it("layoutAfterDrop puts a conversation into a group after existing members", () => {
  const created = createGroup(empty, "工作");
  const groupId = created.groups[0]!.id;
  const layout: GroupLayout = {
    ...created,
    membership: { a: groupId },
    order: ["a", "b"],
  };
  const moved = layoutAfterDrop(
    layout,
    { kind: "conversation", id: "b" },
    { target: "group", id: groupId, edge: "into" },
  );
  expect(moved.membership["b"]).toBe(groupId);
  expect(moved.order).toEqual(["a", "b"]);
});

it("layoutAfterDrop reorders groups after another", () => {
  const first = createGroup(empty, "甲");
  const second = createGroup(first, "乙");
  const moved = layoutAfterDrop(
    second,
    { kind: "group", id: second.groups[0]!.id },
    { target: "group", id: second.groups[1]!.id, edge: "after" },
  );
  expect(moved.groups.map((group) => group.name)).toEqual(["乙", "甲"]);
});
