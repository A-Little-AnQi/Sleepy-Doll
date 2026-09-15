import { afterEach, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SettingsPage } from "./SettingsPage";
import { PREVIEW_PERMISSION, type Bootstrap } from "../types";

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
