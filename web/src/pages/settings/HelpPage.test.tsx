import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { HelpPage } from "./HelpPage";
import source from "./help.md?raw";

afterEach(cleanup);

it("is written as markdown and covers every product surface", () => {
  expect(source).toMatch(/^# 使用说明/m);
  expect(source).not.toMatch(/^\|/m);
  expect(source).toContain("[去添加模型](/app/models)");
  render(<HelpPage />);
  expect(screen.getByRole("heading", { name: "使用说明" })).toBeTruthy();
  const toc = screen.getByRole("navigation", { name: "目录" });
  expect(document.querySelector(".help-article")).toBeTruthy();
  expect(toc.previousElementSibling?.classList.contains("help-article")).toBe(
    true,
  );
  for (const title of [
    "添加模型",
    "发出第一条消息",
    "管理对话",
    "控制执行",
    "复用常用流程",
    "扩展能力",
    "连接 BetterGI",
    "调整设置",
    "解决问题",
  ]) {
    expect(screen.getByRole("heading", { name: title })).toBeTruthy();
    expect(toc.querySelector(`a[href="#${title}"]`)?.textContent).toBe(title);
  }
  expect(screen.getAllByText(/新建对话/).length).toBeGreaterThan(0);
  expect(screen.getAllByText(/工具与扩展/).length).toBeGreaterThan(0);
  expect(screen.getByText(/替我审批/)).toBeTruthy();
  expect(screen.getAllByText(/配置恢复/).length).toBeGreaterThan(0);
  expect(screen.getAllByText(/保存为快捷任务/).length).toBeGreaterThan(0);
  expect(screen.getByText(/展开侧栏/)).toBeTruthy();
  expect(screen.getByText(/设为默认/)).toBeTruthy();
  expect(screen.getByText(/显示详情/)).toBeTruthy();
  expect(screen.getByText(/导入技能/)).toBeTruthy();
  expect(screen.getAllByText(/接口目录/).length).toBeGreaterThan(0);
  expect(screen.getByText(/需要补充信息/)).toBeTruthy();
  expect(screen.getByText(/确认执行/)).toBeTruthy();
});

it("opens the matching product surface from in-text links", () => {
  const onOpen = vi.fn();
  render(<HelpPage onOpen={onOpen} />);
  fireEvent.click(screen.getByRole("button", { name: "去添加模型" }));
  expect(onOpen).toHaveBeenCalledWith("models");
  fireEvent.click(screen.getByRole("button", { name: "去连接" }));
  expect(onOpen).toHaveBeenCalledWith("bridge");
  fireEvent.click(screen.getByRole("button", { name: "去看快捷任务" }));
  expect(onOpen).toHaveBeenCalledWith("tasks");
  fireEvent.click(screen.getByRole("button", { name: "去看扩展" }));
  expect(onOpen).toHaveBeenCalledWith("extensions");
});

it("jumps from the table of contents to the matching section", () => {
  render(<HelpPage />);
  fireEvent.click(
    screen.getByRole("navigation", { name: "目录" }).querySelector("a")!,
  );
  expect(document.getElementById("添加模型")).toBeTruthy();
});
