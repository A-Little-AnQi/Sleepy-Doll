import { useEffect, useRef, useState } from "react";
import { BrandIcon } from "./icons";
import { ThemeSwitch } from "./ThemeSwitch";
import { SettingRow } from "./SettingRow";
import { Select } from "./Select";
import {
  readTheme,
  writeTheme,
  type ThemeId,
  type ThemeOrigin,
} from "../appearance";
import {
  LOCALE_OPTIONS,
  readLocale,
  writeLocale,
  type LocaleId,
} from "../locale";

export function SidebarAccount({
  onSettings,
}: {
  onSettings(): void;
}) {
  const [open, setOpen] = useState(false);
  const [theme, setTheme] = useState(readTheme);
  const [locale, setLocale] = useState(readLocale);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!(event.target instanceof Node)) return;
      if (root.current?.contains(event.target)) return;
      // 语言下拉挂在 body 上，点选项不能当成点了菜单外面。
      if (
        event.target instanceof Element &&
        event.target.closest("[data-ui='select-menu']")
      ) {
        return;
      }
      setOpen(false);
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

  const applyTheme = (next: ThemeId, origin: ThemeOrigin) => {
    setTheme(next);
    return writeTheme(next, origin);
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
          onClick={() => {
            setOpen((value) => {
              const next = !value;
              if (next) {
                setTheme(readTheme());
                setLocale(readLocale());
              }
              return next;
            });
          }}
        >
          <span className="app-account-avatar">
            <BrandIcon />
          </span>
          <span className="app-account-copy">
            <strong>本地用户</strong>
            <small>本机</small>
          </span>
        </button>
      </div>
      {open && (
        <div
          id="app-account-menu"
          className="app-account-menu"
          role="menu"
          aria-label="账户菜单"
        >
          <SettingRow label="主题" compact>
            <ThemeSwitch theme={theme} onChange={applyTheme} />
          </SettingRow>
          <SettingRow label="语言" compact>
            <Select
              label="语言"
              value={locale}
              options={LOCALE_OPTIONS}
              onChange={(value) => {
                const next = value as LocaleId;
                setLocale(next);
                writeLocale(next);
              }}
            />
          </SettingRow>
          <button
            type="button"
            className="app-account-more"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSettings();
            }}
          >
            设置
          </button>
        </div>
      )}
    </div>
  );
}
