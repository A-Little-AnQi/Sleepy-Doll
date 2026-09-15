import { afterEach, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AppShell } from "./AppShell";
import { PREVIEW_PERMISSION, type Bootstrap } from "../types";

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
  expect(screen.getByRole("button", { name: /新分组/ })).toBeTruthy();
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

it("keeps settings on the right and opens theme choices from the local user slot", () => {
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
  const row = document.querySelector(".app-account-row") as HTMLElement;
  expect(row.lastElementChild?.textContent).toContain("设置");
  fireEvent.click(screen.getByRole("button", { name: /本地用户/ }));
  expect(screen.getByRole("menu", { name: "外观与快捷设置" })).toBeTruthy();
  fireEvent.click(screen.getByRole("menuitemradio", { name: "深色" }));
  expect(document.documentElement.dataset.theme).toBe("dark");
  fireEvent.click(screen.getByRole("menuitem", { name: "全部设置" }));
  expect(seen).toEqual(["settings"]);
});

