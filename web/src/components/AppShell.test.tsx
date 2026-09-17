import { afterEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { AppShell } from "./AppShell";
import { GROUPS_KEY } from "../conversation-groups";
import { PREVIEW_PERMISSION, type Bootstrap, type ConversationInfo } from "../types";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const bootstrap: Bootstrap = {
  configPath: "/tmp/config.json",
  models: [],
  skills: [],
  plugins: [],
  tools: [],
  conversations: [],
  tasks: [],
  strategies: [],
  workflows: [],
  operations: [],
  resources: [],
  diagnostics: [],
  notifications: [],
  permission: PREVIEW_PERMISSION,
  bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1" },
};

function stubWide(wide = true) {
  window.matchMedia = ((query: string) => ({
    matches: query.includes("min-width: 960px")
      ? wide
      : query.includes("min-width: 1180px")
        ? wide
        : false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    onchange: null,
    dispatchEvent() {
      return false;
    },
  })) as typeof window.matchMedia;
}

function renderShell(collapsed = false) {
  stubWide(true);
  localStorage.setItem("sleepy-doll-sidebar-collapsed", String(collapsed));
  return render(
    <AppShell
      bootstrap={bootstrap}
      page="chat"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
}

it("shows a resize edge on the docked sidebar", () => {
  renderShell(false);
  expect(screen.getByRole("separator", { name: "调整侧栏宽度" })).toBeTruthy();
  expect(document.querySelector(".app-sidebar-peek")).toBeNull();
});

it("keeps the main pane wide when the sidebar is collapsed", () => {
  const { container } = renderShell(true);
  const shell = container.querySelector(".app-shell") as HTMLElement;
  expect(shell.getAttribute("data-collapsed")).toBe("true");
  expect(container.querySelector(".app-main")).toBeTruthy();
  expect(document.querySelector(".app-sidebar-peek")).toBeTruthy();
});

it("click collapse, hover peek, and click expand all toggle the same open state", () => {
  renderShell(false);
  const sidebar = document.querySelector(".app-sidebar") as HTMLElement;
  const shell = document.querySelector(".app-shell") as HTMLElement;
  expect(sidebar.getAttribute("aria-hidden")).toBe("false");

  fireEvent.click(screen.getByRole("button", { name: "收起侧栏" }));
  expect(shell.getAttribute("data-collapsed")).toBe("true");
  expect(shell.getAttribute("data-drawer")).toBe("false");
  expect(sidebar.getAttribute("aria-hidden")).toBe("true");

  fireEvent.pointerEnter(document.querySelector(".app-sidebar-peek") as Element);
  expect(shell.getAttribute("data-drawer")).toBe("true");
  expect(sidebar.getAttribute("aria-hidden")).toBe("false");

  fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
  expect(shell.getAttribute("data-collapsed")).toBe("false");
  expect(shell.getAttribute("data-drawer")).toBe("false");
  expect(sidebar.getAttribute("aria-hidden")).toBe("false");
  expect(document.querySelector(".app-sidebar-peek")).toBeNull();
});

it("peeks the collapsed sidebar from the left edge without a scrim", () => {
  renderShell(true);
  const peek = document.querySelector(".app-sidebar-peek");
  const sidebar = document.querySelector(".app-sidebar");
  expect(peek).toBeTruthy();
  expect(sidebar?.getAttribute("aria-hidden")).toBe("true");
  fireEvent.pointerEnter(peek as Element);
  expect(sidebar?.getAttribute("aria-hidden")).toBe("false");
  expect(document.querySelector(".app-scrim")).toBeNull();
});

it("does not dismiss peek when the pointer leaves the window", async () => {
  renderShell(true);
  const peek = document.querySelector(".app-sidebar-peek") as HTMLElement;
  const sidebar = document.querySelector(".app-sidebar") as HTMLElement;
  fireEvent.pointerEnter(peek);
  fireEvent.pointerLeave(sidebar, { relatedTarget: null });
  fireEvent.pointerLeave(peek, { relatedTarget: null });
  await waitFor(
    () => expect(sidebar.getAttribute("aria-hidden")).toBe("false"),
    { timeout: 500 },
  );
  fireEvent.pointerEnter(peek, { relatedTarget: null });
  expect(sidebar.getAttribute("aria-hidden")).toBe("false");
});

it("lets the user create a conversation group instead of toggling archived chats", () => {
  renderShell(false);
  expect(screen.queryByRole("button", { name: "显示已归档" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "新建分组" }));
  expect(screen.getByLabelText("分组名称")).toBeTruthy();
  expect((screen.getByLabelText("分组名称") as HTMLInputElement).value).toBe(
    "新分组",
  );
  expect(screen.getByRole("button", { name: "新对话" })).toBeTruthy();
});

it("does not animate an empty group body over the empty conversation hint", () => {
  stubWide(true);
  localStorage.setItem(
    GROUPS_KEY,
    JSON.stringify({
      groups: [{ id: "g1", name: "222", collapsed: true }],
      membership: {},
      order: [],
    }),
  );
  renderShell(false);
  expect(screen.getByRole("button", { name: "新对话" }).closest(".app-group")).toBeNull();
  expect(document.querySelector(".app-group-chats")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "222" }));
  expect(document.querySelector(".app-group-chats")).toBeNull();
});

it("does not expose pin or archive on conversation rows", () => {
  stubWide(true);
  render(
    <AppShell
      bootstrap={{
        ...bootstrap,
        conversations: [
          {
            id: "c1",
            title: "夜巡",
            createdAt: "t",
            updatedAt: "t",
          },
        ],
      }}
      page="chat"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(
    screen.queryByRole("button", { name: "置顶", hidden: true }),
  ).toBeNull();
  expect(
    screen.queryByRole("button", { name: "归档", hidden: true }),
  ).toBeNull();
  expect(screen.getByRole("button", { name: "重命名", hidden: true })).toBeTruthy();
  expect(screen.getByRole("button", { name: "删除对话", hidden: true })).toBeTruthy();
});

it("lets conversations be pointer-sorted instead of html5-dragged", () => {
  stubWide(true);
  render(
    <AppShell
      bootstrap={{
        ...bootstrap,
        conversations: [
          {
            id: "c1",
            title: "夜巡",
            createdAt: "t",
            updatedAt: "t",
          },
        ],
      }}
      page="chat"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(document.querySelector("[draggable='true']")).toBeNull();
  expect(
    document.querySelector("[data-sd-slot='conversation'][data-sd-id='c1']"),
  ).toBeTruthy();
});

it("opens theme choices and settings from the local user slot", async () => {
  stubWide(true);
  const seen: string[] = [];
  render(
    <AppShell
      bootstrap={bootstrap}
      page="chat"
      detailsOpen={false}
      onPage={(page) => seen.push(page)}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.queryByRole("button", { name: "设置" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /本地用户/ }));
  expect(screen.getByRole("menu", { name: "账户菜单" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "设置" })).toBeTruthy();
  expect(screen.getByText("主题")).toBeTruthy();
  expect(screen.getByRole("combobox", { name: "语言" })).toBeTruthy();
  const themeSwitch = screen.getByRole("switch", { name: "主题" });
  expect(themeSwitch.textContent).toContain("白昼");
  expect(themeSwitch.querySelector("svg")).toBeTruthy();
  fireEvent.click(themeSwitch);
  expect(document.documentElement.dataset.theme).toBe("dark");
  await waitFor(() => {
    expect(
      screen.getByRole("switch", { name: "主题" }),
    ).toBe(document.activeElement);
  });
  expect(screen.getByRole("switch", { name: "主题" }).textContent).toContain(
    "黑夜",
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "设置" }));
  expect(seen).toEqual(["settings"]);
});

it("keeps BetterGI above the account divider", () => {
  renderShell(false);
  const button = document.querySelector(".app-sidebar-status .app-connection");
  expect(button).toBeTruthy();
  expect(
    document.querySelector(".app-sidebar-foot .app-connection"),
  ).toBeNull();
  expect(document.querySelector(".app-sidebar-foot .app-account")).toBeTruthy();
  expect(button?.querySelector("path")?.getAttribute("d")).toBe(
    "M9 7V4M15 7V4M7 7h10v5a5 5 0 0 1-10 0zM12 17v4",
  );
});

const chat = (
  id: string,
  title: string,
): ConversationInfo => ({
  id,
  title,
  createdAt: "t",
  updatedAt: "t",
});

it("shows a draft chat in the group you were in, then assigns the created conversation", () => {
  stubWide(true);
  localStorage.setItem(
    GROUPS_KEY,
    JSON.stringify({
      groups: [{ id: "g1", name: "路线", collapsed: false }],
      membership: { c1: "g1" },
      order: ["c1"],
    }),
  );
  let setId!: (id: string | undefined) => void;
  let setList!: (items: ConversationInfo[]) => void;
  function Harness() {
    const [id, setConversation] = useState<string | undefined>("c1");
    const [list, setConversations] = useState([chat("c1", "夜巡")]);
    setId = setConversation;
    setList = setConversations;
    return (
      <AppShell
        bootstrap={{ ...bootstrap, conversations: list }}
        page="chat"
        conversationId={id}
        detailsOpen={false}
        onPage={() => undefined}
        onNew={() => setConversation(undefined)}
        onConversation={setConversation}
        onToggleDetails={() => undefined}
        reload={async () => undefined}
      />
    );
  }
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "新建对话" }));
  const draft = screen.getByRole("button", { name: "新对话" });
  expect(draft.closest(".app-group")).toBeTruthy();
  expect(draft.closest(".app-group")?.textContent).toContain("路线");
  act(() => {
    setList([chat("c1", "夜巡"), chat("c2", "新对话")]);
    setId("c2");
  });
  expect(JSON.parse(localStorage.getItem(GROUPS_KEY) ?? "{}").membership.c2).toBe(
    "g1",
  );
});

it("toggles a group by clicking the row, not only the chevron", () => {
  stubWide(true);
  localStorage.setItem(
    GROUPS_KEY,
    JSON.stringify({
      groups: [{ id: "g1", name: "路线", collapsed: true }],
      membership: { c1: "g1" },
      order: ["c1"],
    }),
  );
  render(
    <AppShell
      bootstrap={{ ...bootstrap, conversations: [chat("c1", "夜巡")] }}
      page="chat"
      conversationId="c1"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  const row = screen.getByRole("button", { name: "路线" });
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector(".app-group-chats.is-collapsed")).toBeTruthy();
  fireEvent.click(row);
  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelector(".app-group-chats.is-collapsed")).toBeNull();
  fireEvent.click(row);
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector(".app-group-chats.is-collapsed")).toBeTruthy();
});

it("shows only a new-chat control on a group and keeps rename and delete in the context menu", () => {
  stubWide(true);
  localStorage.setItem(
    GROUPS_KEY,
    JSON.stringify({
      groups: [{ id: "g1", name: "路线", collapsed: false }],
      membership: {},
      order: [],
    }),
  );
  render(
    <AppShell
      bootstrap={bootstrap}
      page="chat"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  const group = document.querySelector(".app-group") as HTMLElement;
  expect(within(group).queryByRole("button", { name: "删除分组" })).toBeNull();
  expect(within(group).queryByRole("button", { name: "重命名" })).toBeNull();
  expect(
    within(group).getByRole("button", {
      name: "在此分组新建对话",
      hidden: true,
    }),
  ).toBeTruthy();
  fireEvent.contextMenu(screen.getByRole("button", { name: "路线" }));
  expect(screen.getByRole("menu", { name: "分组" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "重命名" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "删除分组" })).toBeTruthy();
});

it("creates a draft chat from the group plus control", () => {
  stubWide(true);
  localStorage.setItem(
    GROUPS_KEY,
    JSON.stringify({
      groups: [{ id: "g1", name: "路线", collapsed: true }],
      membership: {},
      order: ["c1"],
    }),
  );
  function Harness() {
    const [id, setConversation] = useState<string | undefined>("c1");
    return (
      <AppShell
        bootstrap={{ ...bootstrap, conversations: [chat("c1", "夜巡")] }}
        page="chat"
        conversationId={id}
        detailsOpen={false}
        onPage={() => undefined}
        onNew={() => setConversation(undefined)}
        onConversation={setConversation}
        onToggleDetails={() => undefined}
        reload={async () => undefined}
      />
    );
  }
  render(<Harness />);
  fireEvent.click(
    screen.getByRole("button", { name: "在此分组新建对话", hidden: true }),
  );
  expect(screen.getByRole("button", { name: "新对话" }).closest(".app-group")).toBeTruthy();
  expect(screen.getByRole("button", { name: "路线" }).getAttribute("aria-expanded")).toBe(
    "true",
  );
});

it("does not put a chrome title or details toggle on pages that already have a heading", () => {
  stubWide(true);
  localStorage.setItem("sleepy-doll-sidebar-collapsed", "false");
  for (const page of ["tasks", "extensions", "settings"] as const) {
    cleanup();
    render(
      <AppShell
        bootstrap={bootstrap}
        page={page}
        detailsOpen={page === "tasks"}
        onPage={() => undefined}
        onNew={() => undefined}
        onConversation={() => undefined}
        onToggleDetails={() => undefined}
        reload={async () => undefined}
      />,
    );
    expect(document.querySelector(".app-header")).toBeNull();
    expect(screen.queryByRole("button", { name: "显示详情" })).toBeNull();
  }
});

it("keeps expand controls when the sidebar is collapsed, without a details toggle", () => {
  stubWide(true);
  localStorage.setItem("sleepy-doll-sidebar-collapsed", "true");
  render(
    <AppShell
      bootstrap={bootstrap}
      page="tasks"
      detailsOpen={false}
      onPage={() => undefined}
      onNew={() => undefined}
      onConversation={() => undefined}
      onToggleDetails={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByRole("button", { name: "展开侧栏" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "新建对话" })).toBeTruthy();
  expect(screen.queryByRole("heading", { level: 1 })).toBeNull();
  expect(screen.queryByRole("button", { name: "显示详情" })).toBeNull();
});


