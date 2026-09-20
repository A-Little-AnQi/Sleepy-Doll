import { useEffect, useRef, useState } from "react";
import { BrandIcon, HelpIcon, SettingsIcon } from "../icons";
import { ThemeSwitch } from "../controls/ThemeSwitch";
import { SettingRow } from "../controls/SettingRow";
import { Select } from "../controls/Select";
import { useTheme, writeTheme } from "../../appearance";
import {
  LOCALE_OPTIONS,
  useLocale,
  writeLocale,
  type LocaleId,
} from "../../appearance/locale";
import { useT } from "../../i18n";

export function SidebarAccount({
  onSettings,
  onHelp,
}: {
  onSettings(): void;
  onHelp(): void;
}) {
  const [open, setOpen] = useState(false);
  const t = useT();
  const theme = useTheme();
  const locale = useLocale();
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

  return (
    <div className="app-account-row" ref={root}>
      <div className="app-account-slot">
        <button
          id="app-account-trigger"
          className="app-account"
          aria-label={t.account.localUser}
          aria-haspopup="menu"
          aria-expanded={open}
          aria-controls="app-account-menu"
          onClick={() => setOpen((value) => !value)}
        >
          <span className="app-account-avatar">
            <BrandIcon />
          </span>
          <span className="app-account-copy">
            <strong>{t.account.localUser}</strong>
            <small>{t.account.onThisMachine}</small>
          </span>
        </button>
      </div>
      {open && (
        <div
          id="app-account-menu"
          className="app-account-menu"
          role="menu"
          aria-label={t.account.menu}
        >
          <div className="app-account-menu-prefs">
            <SettingRow label={t.settings.theme} compact>
              <ThemeSwitch theme={theme} onChange={writeTheme} />
            </SettingRow>
            <SettingRow label={t.settings.language} compact>
              <Select
                label={t.settings.language}
                value={locale}
                options={LOCALE_OPTIONS}
                onChange={(value) => writeLocale(value as LocaleId)}
              />
            </SettingRow>
          </div>
          <div className="app-account-menu-links">
            <button
              type="button"
              className="app-account-more"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                onHelp();
              }}
            >
              <HelpIcon className="button-icon" />
              {t.account.help}
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
              <SettingsIcon className="button-icon" />
              {t.account.settings}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
