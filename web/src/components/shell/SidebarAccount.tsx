import { useEffect, useRef, useState } from "react";
import { BrandIcon, HelpIcon, SettingsIcon } from "@sleepy-doll/ui/icons";
import { ThemeSwitch } from "@sleepy-doll/ui/controls/ThemeSwitch";
import { SettingRow } from "@sleepy-doll/ui/controls/SettingRow";
import { useTheme, writeTheme } from "../../appearance";
import { useT } from "../../i18n";
import { usePresence } from "@sleepy-doll/ui/motion";

export function SidebarAccount({
  onHelp,
  onSettings,
}: {
  onHelp(): void;
  onSettings(): void;
}) {
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const present = usePresence(open, menu);
  const t = useT();
  const theme = useTheme();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const keyboardOpen = useRef(false);

  useEffect(() => {
    if (!open || !present) return;
    if (keyboardOpen.current) {
      root.current
        ?.querySelector<HTMLButtonElement>(".app-account-menu button")
        ?.focus({ preventScroll: true });
      keyboardOpen.current = false;
    }
    const close = (event: MouseEvent) => {
      if (!(event.target instanceof Node)) return;
      if (root.current?.contains(event.target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      trigger.current?.focus({ preventScroll: true });
    };
    document.addEventListener("mousedown", close);
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
          data-theme-control-group=""
          ref={menu}
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
