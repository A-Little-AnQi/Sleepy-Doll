import { useEffect, useRef, useState } from "react";
import { BrandIcon, SettingsIcon } from "./icons";
import {
  readReducedMotion,
  readTheme,
  writeReducedMotion,
  writeTheme,
  type ThemeId,
} from "../appearance";

export function SidebarAccount({
  settingsActive,
  onSettings,
}: {
  settingsActive: boolean;
  onSettings(): void;
}) {
  const [open, setOpen] = useState(false);
  const [theme, setTheme] = useState(readTheme);
  const [motion, setMotion] = useState(readReducedMotion);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const applyTheme = (next: ThemeId) => {
    setTheme(next);
    writeTheme(next);
  };

  return (
    <div className="app-account-row" ref={root}>
      <div className="app-account-slot">
        <button
          id="app-account-trigger"
          className="app-account"
          aria-label="本地用户"
          aria-haspopup="menu"
          aria-expanded={open}
          aria-controls="app-account-menu"
          onClick={() => setOpen((value) => !value)}
        >
          <span className="app-account-avatar">
            <BrandIcon />
          </span>
          <span className="app-account-copy">
            <strong>本地用户</strong>
            <small>本机</small>
          </span>
        </button>
        {open && (
          <div
            id="app-account-menu"
            className="app-account-menu"
            role="menu"
            aria-label="外观与快捷设置"
          >
            <p className="app-account-menu-label">外观</p>
            <div className="app-theme-toggle" role="group" aria-label="主题">
              <button
                type="button"
                role="menuitemradio"
                aria-checked={theme === "light"}
                className={theme === "light" ? "is-active" : ""}
                onClick={() => applyTheme("light")}
              >
                浅色
              </button>
              <button
                type="button"
                role="menuitemradio"
                aria-checked={theme === "dark"}
                className={theme === "dark" ? "is-active" : ""}
                onClick={() => applyTheme("dark")}
              >
                深色
              </button>
            </div>
            <button
              type="button"
              className="app-account-motion"
              role="menuitemcheckbox"
              aria-checked={motion}
              onClick={() => {
                const next = !motion;
                setMotion(next);
                writeReducedMotion(next);
              }}
            >
              减少动态效果
              <span className={`switch ${motion ? "on" : ""}`} />
            </button>
            <button
              type="button"
              className="app-account-more"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                onSettings();
              }}
            >
              全部设置
            </button>
          </div>
        )}
      </div>
      <button
        className={`app-nav-item app-settings-link${settingsActive ? " is-active" : ""}`}
        aria-current={settingsActive ? "page" : undefined}
        onClick={onSettings}
      >
        <SettingsIcon className="app-nav-icon" />
        <span>设置</span>
      </button>
    </div>
  );
}
