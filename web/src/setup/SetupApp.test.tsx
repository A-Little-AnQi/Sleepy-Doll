import { afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { SetupApp, installedDirectory, progressPercent } from "./SetupApp";
import type { SetupInfo, SetupState } from "./api";

const DEFAULT_DIRECTORY =
  "C:\\Users\\me\\AppData\\Local\\Programs\\Sleepy Doll";

interface Request {
  id: string;
  method: string;
  params: Record<string, unknown>;
}

let sent: Request[] = [];

/** 顶替原生侧：记下请求，并按方法给出回复。 */
function host(overrides: Partial<SetupInfo> = {}) {
  sent = [];
  const info: SetupInfo = {
    version: "0.1.0",
    directory: overrides.directory ?? DEFAULT_DIRECTORY,
    defaultDirectory: overrides.defaultDirectory ?? DEFAULT_DIRECTORY,
    installed: overrides.installed ?? false,
    installedVersion: overrides.installedVersion ?? null,
    uninstallMode: overrides.uninstallMode ?? false,
  };
  window.ipc = {
    postMessage(message: string) {
      const request = JSON.parse(message) as Request;
      sent.push(request);
      const result =
        request.method === "setup.info"
          ? info
          : request.method === "setup.browse"
            ? { directory: "D:\\Picked" }
            : {};
      window.__setupReceive?.({ id: request.id, result });
    },
  };
}

function lastRequest(method: string) {
  return sent.filter((item) => item.method === method).at(-1);
}

function push(state: SetupState) {
  act(() => window.__setupState?.(state));
}

function directoryField() {
  return screen.getByLabelText("安装位置") as HTMLInputElement;
}

/** 标题栏里也有关闭按钮，页脚的动作按页脚查。 */
function footer() {
  return within(document.querySelector(".setup-foot") as HTMLElement);
}

function uninstallHost() {
  host({
    uninstallMode: true,
    installed: true,
    installedVersion: "0.1.0",
    directory: "D:\\Games\\Sleepy Doll",
  });
}

function userDataBox() {
  return screen.getByRole("checkbox", { name: /user\\/ }) as HTMLInputElement;
}

afterEach(() => {
  cleanup();
  delete window.ipc;
  delete window.__SLEEPY_DOLL_FRAMELESS__;
  vi.clearAllMocks();
});

it("appends the product directory unless it is already there", () => {
  expect(installedDirectory("D:\\Games")).toBe("D:\\Games\\Sleepy Doll");
  expect(installedDirectory("D:\\Games\\")).toBe("D:\\Games\\Sleepy Doll");
  expect(installedDirectory("D:\\Games\\Sleepy Doll")).toBe(
    "D:\\Games\\Sleepy Doll",
  );
  expect(installedDirectory("D:\\Games\\Sleepy-Doll")).toBe(
    "D:\\Games\\Sleepy-Doll",
  );
  expect(installedDirectory("  ")).toBe("");
});

it("reads progress as either a fraction or a percentage", () => {
  expect(progressPercent(0.42)).toBe(42);
  expect(progressPercent(42)).toBe(42);
  expect(progressPercent(1)).toBe(100);
  expect(progressPercent(120)).toBe(100);
});

it("still renders the form when no native host answers", () => {
  render(<SetupApp />);
  expect(directoryField().value).toBe("D:\\Sleepy Doll");
  expect(screen.getByRole("button", { name: "安装" })).toBeTruthy();
  expect(screen.getByText("版本 0.1.0")).toBeTruthy();
  expect(document.querySelector(".title-bar")).toBeNull();
});

it("shows where the chosen directory ends up and installs into it", async () => {
  host();
  render(<SetupApp />);
  await waitFor(() => expect(directoryField().value).toBe(DEFAULT_DIRECTORY));

  fireEvent.change(directoryField(), { target: { value: "D:\\Games" } });
  expect(screen.getByText(/最终会装到/).textContent).toBe(
    "最终会装到 D:\\Games\\Sleepy Doll",
  );

  fireEvent.click(screen.getByRole("checkbox", { name: "创建桌面快捷方式" }));
  fireEvent.click(screen.getByRole("button", { name: "安装" }));

  await waitFor(() =>
    expect(lastRequest("setup.install")?.params).toEqual({
      directory: "D:\\Games",
      desktopShortcut: false,
    }),
  );
});

it("refuses to install into an empty path", async () => {
  host();
  render(<SetupApp />);
  await waitFor(() => expect(directoryField().value).toBe(DEFAULT_DIRECTORY));
  fireEvent.change(directoryField(), { target: { value: "   " } });
  expect(screen.queryByText(/最终会装到/)).toBeNull();
  expect(screen.getByRole("button", { name: "安装" })).toHaveProperty(
    "disabled",
    true,
  );
});

it("prefills the existing install so overwriting keeps its place", async () => {
  host({
    installed: true,
    installedVersion: "0.1.0",
    directory: "D:\\Games\\Sleepy Doll",
  });
  render(<SetupApp />);
  await waitFor(() =>
    expect(directoryField().value).toBe("D:\\Games\\Sleepy Doll"),
  );
  expect(
    screen.getByText("所有文件与数据均会保存在安装目录下"),
  ).toBeTruthy();
  expect(directoryField().disabled).toBe(true);
  expect(screen.getByRole("button", { name: "浏览" })).toHaveProperty(
    "disabled",
    true,
  );
  expect(screen.getByText(/最终会装到/).textContent).toBe(
    "最终会装到 D:\\Games\\Sleepy Doll",
  );
});

it("locks every control and moves the bar while running", async () => {
  host();
  render(<SetupApp />);
  fireEvent.click(screen.getByRole("button", { name: "安装" }));
  push({
    phase: "running",
    progress: 0.42,
    message: "正在复制程序文件…",
    error: null,
  });

  expect(screen.getByText("正在复制程序文件…")).toBeTruthy();
  expect(
    screen
      .getByRole("progressbar", { name: "安装进度" })
      .getAttribute("aria-valuenow"),
  ).toBe("42");
  expect(directoryField().disabled).toBe(true);
  expect(
    (
      screen.getByRole("checkbox", {
        name: "创建桌面快捷方式",
      }) as HTMLInputElement
    ).disabled,
  ).toBe(true);
  expect(
    screen.getByRole("button", { name: "安装" }).hasAttribute("disabled"),
  ).toBe(true);
  expect(
    screen.getByRole("button", { name: "取消" }).hasAttribute("disabled"),
  ).toBe(true);
  expect(screen.queryByLabelText("关闭")).toBeNull();
});

it("disables the window close button while writing files", async () => {
  window.__SLEEPY_DOLL_FRAMELESS__ = true;
  host();
  render(<SetupApp />);
  fireEvent.click(screen.getByRole("button", { name: "安装" }));
  push({
    phase: "running",
    progress: 0.42,
    message: "正在复制程序文件…",
    error: null,
  });
  expect(screen.getByLabelText("关闭")).toHaveProperty("disabled", true);
});

it("reports the failure and lets the user go back and retry", async () => {
  host();
  render(<SetupApp />);
  fireEvent.change(directoryField(), { target: { value: "D:\\Games" } });
  fireEvent.click(screen.getByRole("button", { name: "安装" }));
  push({
    phase: "failed",
    progress: 0,
    message: "",
    error: "无法写入目标目录，请换一个位置。",
  });

  expect(screen.getByText("安装失败")).toBeTruthy();
  expect(screen.getByText("无法写入目标目录，请换一个位置。")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "返回重试" }));
  expect(directoryField().value).toBe("D:\\Games");
  expect(
    screen.getByRole("button", { name: "安装" }).hasAttribute("disabled"),
  ).toBe(false);
});

it("says where the program went once it is installed", async () => {
  host();
  render(<SetupApp />);
  fireEvent.change(directoryField(), { target: { value: "D:\\Games" } });
  fireEvent.click(screen.getByRole("button", { name: "安装" }));
  push({ phase: "done", progress: 1, message: "", error: null });

  expect(screen.getByText("安装完成")).toBeTruthy();
  expect(screen.getByText("已安装到")).toBeTruthy();
  expect(document.querySelector(".setup-target")?.textContent).toBe(
    "D:\\Games\\Sleepy Doll",
  );
  expect(footer().getByRole("button", { name: "关闭" })).toBeTruthy();
});

it("keeps user\\ by default when uninstalling", async () => {
  uninstallHost();
  render(<SetupApp />);
  await screen.findByRole("checkbox", { name: /user\\/ });
  expect(userDataBox().checked).toBe(false);
  expect(screen.getByText("D:\\Games\\Sleepy Doll")).toBeTruthy();
  expect(
    screen.queryByRole("checkbox", { name: "创建桌面快捷方式" }),
  ).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "卸载" }));
  await waitFor(() =>
    expect(lastRequest("setup.uninstall")?.params).toEqual({
      removeUserData: false,
    }),
  );
});

it("deletes user\\ only after the box is ticked", async () => {
  uninstallHost();
  render(<SetupApp />);
  await screen.findByRole("checkbox", { name: /user\\/ });
  fireEvent.click(userDataBox());
  expect(userDataBox().checked).toBe(true);

  fireEvent.click(screen.getByRole("button", { name: "卸载" }));
  await waitFor(() =>
    expect(lastRequest("setup.uninstall")?.params).toEqual({
      removeUserData: true,
    }),
  );
});

it("says what happened to user\\ when the uninstall finishes", async () => {
  uninstallHost();
  render(<SetupApp />);
  fireEvent.click(await screen.findByRole("button", { name: "卸载" }));
  push({ phase: "done", progress: 1, message: "", error: null });

  expect(screen.getByText("卸载完成")).toBeTruthy();
  expect(document.querySelector(".setup-target")?.textContent).toBe(
    "D:\\Games\\Sleepy Doll",
  );
  expect(screen.getByText(/user\\ 目录保留在/)).toBeTruthy();
});
