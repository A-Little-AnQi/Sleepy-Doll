import { useLayoutEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import {
  BrandIcon,
  BridgeIcon,
  FolderIcon,
  HelpIcon,
  SettingsIcon,
  SidebarIcon,
  StopIcon,
} from "../components/icons";
import { Switch } from "../components/controls/Switch";
import { ContextMenu } from "../components/shell/ContextMenu";
import "../product.css";
import "./tray.css";

interface TrayState {
  dark?: boolean;
  active?: boolean;
  bridge?: boolean;
  bridgeBusy?: boolean;
}
declare global {
  interface Window {
    __TRAY_STATE__?: TrayState;
    updateMenu?: (state: TrayState) => void;
    showMenu?: (state: TrayState, generation: number) => void;
  }
}

function send(action: string, detail: Record<string, unknown> = {}) {
  window.ipc?.postMessage(JSON.stringify({ action, ...detail }));
}

function TrayMenu() {
  const [state, setState] = useState<TrayState>(
    window.__TRAY_STATE__ ?? {
      dark: localStorage.getItem("sleepy-doll-theme") === "dark",
    },
  );
  const menu = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    document.documentElement.dataset.theme = state.dark ? "dark" : "light";
  }, [state.dark]);
  useLayoutEffect(() => {
    const update = (next: TrayState) => flushSync(() => setState(next));
    window.updateMenu = update;
    let cancelled = false;
    const painted = (action: string, generation?: number) => {
      void document.fonts.ready.then(() => {
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            if (!cancelled)
              send(action, {
                generation,
                height: Math.ceil(
                  menu.current?.getBoundingClientRect().height ?? 0,
                ),
              });
          }),
        );
      });
    };
    window.showMenu = (next, generation) => {
      update(next);
      if (document.activeElement instanceof HTMLElement)
        document.activeElement.blur();
      // 过两帧再让宿主收尾：首帧提交样式与布局，第二帧确认合成。
      painted("shown", generation);
    };
    painted("ready");
    return () => {
      cancelled = true;
      delete window.updateMenu;
      delete window.showMenu;
    };
  }, []);
  return (
    <div
      className="tray-menu"
      ref={menu}
      role="menu"
      aria-label="Sleepy Doll"
      data-running={Boolean(state.active)}
      onContextMenu={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          send("dismiss");
          return;
        }
        if (!["ArrowDown", "ArrowUp", "Home", "End", "Tab"].includes(event.key))
          return;
        event.preventDefault();
        const items = Array.from(
          menu.current?.querySelectorAll<HTMLButtonElement>(
            "button:not(:disabled)",
          ) ?? [],
        );
        const index = items.indexOf(
          document.activeElement as HTMLButtonElement,
        );
        const backwards =
          event.key === "ArrowUp" || (event.key === "Tab" && event.shiftKey);
        const next =
          event.key === "Home"
            ? 0
            : event.key === "End"
              ? items.length - 1
              : index < 0
                ? backwards
                  ? items.length - 1
                  : 0
                : (index + (backwards ? -1 : 1) + items.length) % items.length;
        items[next]?.focus();
      }}
    >
      <header className="tray-identity">
        <span className="tray-brand">
          <BrandIcon />
        </span>
        <div>
          <strong>Sleepy Doll</strong>
          <div className="tray-status" role="status">
            {state.active ? "任务正在运行" : "当前没有运行中的任务"}
          </div>
        </div>
      </header>
      <div className="tray-items">
        <button
          className="tray-row"
          role="menuitem"
          onClick={() => send("open")}
        >
          <SidebarIcon />
          <span>打开主窗口</span>
        </button>
        <button
          className="tray-row"
          role="menuitem"
          onClick={() => send("settings")}
        >
          <SettingsIcon />
          <span>设置</span>
        </button>
        <button
          className="tray-row"
          role="menuitem"
          onClick={() => send("help")}
        >
          <HelpIcon />
          <span>使用说明与更新</span>
        </button>
        <button
          className="tray-row"
          role="menuitem"
          onClick={() => send("folder")}
        >
          <FolderIcon />
          <span>打开数据文件夹</span>
        </button>
      </div>
      <div className="tray-rule" role="separator" />
      <div className="tray-items">
        <label className="tray-switch-row">
          <BridgeIcon />
          <span>
            {state.bridgeBusy
              ? state.bridge
                ? "正在断开…"
                : "正在连接…"
              : "BetterGI 连接"}
          </span>
          <Switch
            label="BetterGI 连接"
            role="menuitemcheckbox"
            checked={Boolean(state.bridge)}
            disabled={Boolean(state.bridgeBusy)}
            onChange={(bridge) => {
              if (window.ipc) send("bridge");
              else setState((previous) => ({ ...previous, bridge }));
            }}
          />
        </label>
        <button
          className="tray-row tray-stop"
          role="menuitem"
          disabled={!state.active}
          onClick={() => send("stop")}
        >
          <StopIcon />
          <span>停止所有任务</span>
          <kbd>Ctrl+Alt+Q</kbd>
        </button>
      </div>
      <div className="tray-rule" role="separator" />
      <button className="tray-row" role="menuitem" onClick={() => send("quit")}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 3v9M7 5a8 8 0 1 0 10 0" />
        </svg>
        <span>{state.active ? "停止任务并退出" : "退出 Sleepy Doll"}</span>
      </button>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <>
    <TrayMenu />
    <ContextMenu />
  </>,
);
