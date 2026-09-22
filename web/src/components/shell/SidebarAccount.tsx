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
  onHelp,
  onSettings,
}: {
  onHelp(): void;
  onSettings(): void;
}) {
  const [open, setOpen] = useState(false);
  const [present, setPresent] = useState(false);
  const t = useT();
  const theme = useTheme();
  const locale = useLocale();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const keyboardOpen = useRef(false);

  useEffect(() => {
    if (open) {
      setPresent(true);
      return;
    }
    if (!present) return;
    const delay = window.matchMedia?.("(prefers-reduced-motion: reduce)")
      .matches
      ? 0
      : 140;
    const timer = window.setTimeout(() => setPresent(false), delay);
    return () => window.clearTimeout(timer);
  }, [open, present]);

  useEffect(() => {
    if (!open || !present) return;
    if (keyboardOpen.current) {
      root.current
        ?.querySelector<HTMLButtonElement>(".app-account-menu button")
        ?.focus();
      keyboardOpen.current = false;
    }
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
      if (event.key !== "Escape") return;
      // Escape first dismisses the language list, then the account popover.
      if (
        root.current?.querySelector('[data-ui="select-root"][data-open="true"]')
      )
        return;
      setOpen(false);
      trigger.current?.focus({ preventScroll: true });
    };
    document.addEventListener("mousedown", close);
    // Inspect the nested list before its target handler closes it.
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open, present]);

  return (
    <div
      className="app-account-row"
      ref={root}
      onBlur={(event) => {
        const next = event.relatedTarget;
        if (!(next instanceof Node) || event.currentTarget.contains(next))
          return;
        if (next instanceof Element && next.closest('[data-ui="select-menu"]'))
          return;
        setOpen(false);
      }}
    >
      <div className="app-account-slot">
        <button
          id="app-account-trigger"
          ref={trigger}
          type="button"
          className="app-account"
          aria-label={t.account.localUser}
          aria-haspopup="menu"
          aria-expanded={open}
          aria-controls="app-account-menu"
          onClick={() => setOpen((value) => !value)}
          onKeyDown={(event) => {
            if (["ArrowUp", "ArrowDown"].includes(event.key)) {
              event.preventDefault();
              keyboardOpen.current = true;
              setOpen(true);
            } else if (event.key === "Enter" || event.key === " ") {
              keyboardOpen.current = !open;
            }
          }}
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
      {present && (
        <div
          id="app-account-menu"
          className="app-account-menu"
          data-open={open}
          inert={!open}
          aria-hidden={!open}
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
                trigger.current?.focus({ preventScroll: true });
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
                trigger.current?.focus({ preventScroll: true });
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
