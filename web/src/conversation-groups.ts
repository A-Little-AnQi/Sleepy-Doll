export const GROUPS_KEY = "sleepy-doll-conversation-groups";

export type ConversationGroup = {
  id: string;
  name: string;
  collapsed: boolean;
};

export type GroupLayout = {
  groups: ConversationGroup[];
  membership: Record<string, string>;
  order: string[];
};

export function emptyLayout(): GroupLayout {
  return { groups: [], membership: {}, order: [] };
}

export function readLayout(): GroupLayout {
  try {
    const raw = JSON.parse(localStorage.getItem(GROUPS_KEY) ?? "null");
    if (!raw || !Array.isArray(raw.groups)) return emptyLayout();
    // 显式标注：JSON.parse 返回 any，不标注的话下面 some/find 的参数都是隐式 any。
    const groups: ConversationGroup[] = raw.groups
      .filter(
        (group: ConversationGroup) =>
          group && typeof group.id === "string" && typeof group.name === "string",
      )
      .map((group: ConversationGroup) => ({
        id: group.id,
        name: group.name,
        collapsed: Boolean(group.collapsed),
      }));
    const membership =
      raw.membership && typeof raw.membership === "object"
        ? Object.fromEntries(
            Object.entries(raw.membership as Record<string, string>).filter(
              ([, groupId]) => groups.some((group) => group.id === groupId),
            ),
          )
        : {};
    const order = Array.isArray(raw.order)
      ? (raw.order as unknown[]).filter(
          (id): id is string => typeof id === "string",
        )
      : [];
    return { groups, membership, order };
  } catch {
    return emptyLayout();
  }
}

export function writeLayout(layout: GroupLayout) {
  localStorage.setItem(GROUPS_KEY, JSON.stringify(layout));
}

export function createGroup(
  layout: GroupLayout,
  name = "新分组",
): GroupLayout {
  const trimmed = name.trim() || "新分组";
  return {
    ...layout,
    groups: [
      ...layout.groups,
      {
        id: `group-${crypto.randomUUID()}`,
        name: trimmed,
        collapsed: false,
      },
    ],
  };
}

export function renameGroup(
  layout: GroupLayout,
  id: string,
  name: string,
): GroupLayout {
  const trimmed = name.trim();
  if (!trimmed) return layout;
  return {
    ...layout,
    groups: layout.groups.map((group) =>
      group.id === id ? { ...group, name: trimmed } : group,
    ),
  };
}

export function deleteGroup(layout: GroupLayout, id: string): GroupLayout {
  const membership = { ...layout.membership };
  for (const [conversationId, groupId] of Object.entries(membership)) {
    if (groupId === id) delete membership[conversationId];
  }
  return {
    groups: layout.groups.filter((group) => group.id !== id),
    membership,
    order: layout.order,
  };
}

export function moveInOrder(
  order: string[],
  id: string,
  beforeId: string | null,
): string[] {
  const next = order.filter((item) => item !== id);
  if (!beforeId || beforeId === id) return [...next, id];
  const at = next.indexOf(beforeId);
  if (at < 0) return [...next, id];
  next.splice(at, 0, id);
  return next;
}

export function moveGroupBefore(
  layout: GroupLayout,
  id: string,
  beforeId: string | null,
): GroupLayout {
  const ids = moveInOrder(
    layout.groups.map((group) => group.id),
    id,
    beforeId,
  );
  const byId = new Map(layout.groups.map((group) => [group.id, group]));
  return {
    ...layout,
    groups: ids
      .map((groupId) => byId.get(groupId))
      .filter((group): group is ConversationGroup => Boolean(group)),
  };
}

export function placeConversation(
  layout: GroupLayout,
  conversationId: string,
  groupId: string | null,
  beforeId: string | null,
): GroupLayout {
  const next = setMembership(layout, conversationId, groupId);
  return {
    ...next,
    order: moveInOrder(next.order, conversationId, beforeId),
  };
}

export function setCollapsed(
  layout: GroupLayout,
  id: string,
  collapsed: boolean,
): GroupLayout {
  return {
    ...layout,
    groups: layout.groups.map((group) =>
      group.id === id ? { ...group, collapsed } : group,
    ),
  };
}

export function setMembership(
  layout: GroupLayout,
  conversationId: string,
  groupId: string | null,
): GroupLayout {
  const membership = { ...layout.membership };
  if (!groupId || !layout.groups.some((group) => group.id === groupId)) {
    delete membership[conversationId];
  } else {
    membership[conversationId] = groupId;
  }
  return { ...layout, membership };
}

function byLayoutOrder<T extends { id: string }>(
  items: T[],
  order: string[],
): T[] {
  const rank = new Map(order.map((id, index) => [id, index]));
  return [...items].sort((left, right) => {
    const a = rank.get(left.id);
    const b = rank.get(right.id);
    if (a == null && b == null) return 0;
    if (a == null) return 1;
    if (b == null) return -1;
    return a - b;
  });
}

export function splitConversations<T extends { id: string }>(
  conversations: T[],
  layout: GroupLayout,
): {
  groups: Array<{ group: ConversationGroup; items: T[] }>;
  ungrouped: T[];
} {
  const buckets = new Map<string, T[]>(
    layout.groups.map((group) => [group.id, []]),
  );
  const ungrouped: T[] = [];
  for (const conversation of conversations) {
    const groupId = layout.membership[conversation.id];
    const bucket = groupId ? buckets.get(groupId) : undefined;
    if (bucket) bucket.push(conversation);
    else ungrouped.push(conversation);
  }
  return {
    groups: layout.groups.map((group) => ({
      group,
      items: byLayoutOrder(buckets.get(group.id) ?? [], layout.order),
    })),
    ungrouped: byLayoutOrder(ungrouped, layout.order),
  };
}

export type DropSlot =
  | {
      target: "conversation";
      id: string;
      groupId: string | null;
      edge: "before" | "after";
    }
  | { target: "group"; id: string; edge: "before" | "after" | "into" }
  | { target: "ungrouped" };

function nextAfter(order: string[], afterId: string, movingId: string): string | null {
  const without = order.filter((id) => id !== movingId);
  const at = without.indexOf(afterId);
  if (at < 0) return null;
  return without[at + 1] ?? null;
}

function lastIn(order: string[], ids: string[]): string | undefined {
  let last: string | undefined;
  let lastIndex = -1;
  for (const id of ids) {
    const index = order.indexOf(id);
    if (index > lastIndex) {
      lastIndex = index;
      last = id;
    }
  }
  return last;
}

export function layoutAfterDrop(
  layout: GroupLayout,
  item: { kind: "group" | "conversation"; id: string },
  slot: DropSlot,
): GroupLayout {
  if (item.kind === "group") {
    if (slot.target === "group") {
      if (slot.id === item.id || slot.edge === "into") return layout;
      if (slot.edge === "before") return moveGroupBefore(layout, item.id, slot.id);
      const ids = layout.groups.map((group) => group.id).filter((id) => id !== item.id);
      const at = ids.indexOf(slot.id);
      return moveGroupBefore(layout, item.id, at < 0 ? null : (ids[at + 1] ?? null));
    }
    if (slot.target === "ungrouped") return moveGroupBefore(layout, item.id, null);
    return layout;
  }
  if (slot.target === "ungrouped") {
    return placeConversation(layout, item.id, null, null);
  }
  if (slot.target === "group") {
    const members = Object.entries(layout.membership)
      .filter(([id, groupId]) => groupId === slot.id && id !== item.id)
      .map(([id]) => id);
    const last = lastIn(layout.order, members);
    return placeConversation(
      layout,
      item.id,
      slot.id,
      last ? nextAfter(layout.order, last, item.id) : null,
    );
  }
  if (slot.id === item.id) return layout;
  if (slot.edge === "before") {
    return placeConversation(layout, item.id, slot.groupId, slot.id);
  }
  return placeConversation(
    layout,
    item.id,
    slot.groupId,
    nextAfter(layout.order, slot.id, item.id),
  );
}


export function byRecent<T extends { updatedAt?: string; createdAt?: string }>(
  items: T[],
): T[] {
  return [...items].sort((left, right) => {
    const a = left.updatedAt || left.createdAt || "";
    const b = right.updatedAt || right.createdAt || "";
    return b.localeCompare(a);
  });
}

/** 新对话和用户新发起的运行置顶。流式刷新 updatedAt 不改相对顺序。 */
export function orderConversations<
  T extends { id: string; updatedAt?: string; createdAt?: string },
>(
  items: T[],
  previousOrder: string[],
  runningIds: Iterable<string> = [],
  previouslyRunning: Iterable<string> = [],
): T[] {
  const byId = new Map(items.map((item) => [item.id, item]));
  const present = new Set(byId.keys());
  const running = new Set(runningIds);
  const wasRunning = new Set(previouslyRunning);
  const known = new Set(previousOrder);
  const promoted = byRecent(
    items.filter(
      (item) =>
        !known.has(item.id) || (running.has(item.id) && !wasRunning.has(item.id)),
    ),
  );
  const promotedIds = new Set(promoted.map((item) => item.id));
  const rest = previousOrder
    .filter((id) => present.has(id) && !promotedIds.has(id))
    .map((id) => byId.get(id))
    .filter((item): item is T => Boolean(item));
  const leftover = items.filter(
    (item) => !promotedIds.has(item.id) && !previousOrder.includes(item.id),
  );
  return [...promoted, ...rest, ...leftover];
}
