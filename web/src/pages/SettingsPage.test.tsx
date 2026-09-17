import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsPage } from "./SettingsPage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../types";

vi.mock("../api", () => ({
  api: {
    configRead: vi.fn(async () => ({
      path: "/tmp/config.json",
      content: '{"version":3}\n',
    })),
    configWrite: vi.fn(async () => ({ saved: true })),
  },
}));

afterEach(cleanup);

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

it("keeps sponsor behind its own last settings tab", () => {
  const onSection = () => undefined;
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="settings"
      onSection={onSection}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByRole("button", { name: "赞助作者" })).toBeTruthy();
  expect(screen.queryByRole("heading", { name: "赞助作者" })).toBeNull();
  expect(screen.queryByLabelText("收款二维码")).toBeNull();
});

it("does not show the sponsor page on the model key path", () => {
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="models"
      onSection={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.queryByRole("heading", { name: "赞助作者" })).toBeNull();
  expect(screen.queryByLabelText("收款二维码")).toBeNull();
});

it("opens the QR slot from the last settings tab", () => {
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="sponsor"
      onSection={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByRole("heading", { name: "赞助作者" })).toBeTruthy();
  expect(screen.getByLabelText("收款二维码")).toBeTruthy();
});

it("switches to the sponsor tab from the nav", () => {
  const seen: string[] = [];
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="settings"
      onSection={(section) => seen.push(section)}
      reload={async () => undefined}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "赞助作者" }));
  expect(seen).toEqual(["sponsor"]);
});

it("applies the selected theme from the general settings", async () => {
  document.documentElement.dataset.theme = "light";
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="settings"
      onSection={() => undefined}
      reload={async () => undefined}
    />,
  );
  const themeSwitch = screen.getByRole("switch", { name: "主题" });
  expect(themeSwitch.textContent).toContain("白昼");
  fireEvent.click(themeSwitch);
  expect(document.documentElement.dataset.theme).toBe("dark");
  await waitFor(() => {
    expect(document.activeElement).toBe(
      screen.getByRole("switch", { name: "主题" }),
    );
  });
  expect(screen.getByRole("switch", { name: "主题" }).textContent).toContain(
    "黑夜",
  );
});

it("opens the config editor from the general settings", async () => {
  render(
    <SettingsPage
      bootstrap={bootstrap}
      section="settings"
      onSection={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.getByText("/tmp/config.json")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "编辑" }));
  const dialog = await screen.findByRole("dialog");
  expect(screen.getByRole("heading", { name: "编辑配置" })).toBeTruthy();
  expect(document.querySelector(".sd-dialog-layer")?.parentElement).toBe(
    document.body,
  );
});

it("hides the BetterGI settings tab when the host plugin is off", () => {
  render(
    <SettingsPage
      bootstrap={{
        ...bootstrap,
        plugins: [
          {
            manifest: {
              id: "bgi",
              name: "BetterGI",
              version: "1",
              description: "游戏自动化宿主",
            },
            status: "disabled",
            configuredEnabled: false,
            host: true,
          },
        ],
      }}
      section="settings"
      onSection={() => undefined}
      reload={async () => undefined}
    />,
  );
  expect(screen.queryByRole("button", { name: "BetterGI" })).toBeNull();
});
