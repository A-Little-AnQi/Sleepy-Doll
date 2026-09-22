import { MoonIcon, SunIcon } from "../icons";
import { useT } from "../../i18n";
import type { ThemeId, ThemeOrigin } from "../../appearance";

export function ThemeSwitch({
  theme,
  onChange,
}: {
  theme: ThemeId;
  onChange(next: ThemeId, origin: ThemeOrigin): void | Promise<void>;
}) {
  const t = useT();
  const dark = theme === "dark";
  const next: ThemeId = dark ? "light" : "dark";
  return (
    <button
      type="button"
      className={`app-theme-switch${dark ? " is-dark" : ""}`}
      role="switch"
      aria-checked={dark}
      aria-label={t.settings.theme}
      onClick={(event) => {
        const button = event.currentTarget;
        button.focus({ preventScroll: true });
        const bounds = button.getBoundingClientRect();
        const x =
          event.detail === 0 ? bounds.left + bounds.width / 2 : event.clientX;
        const y =
          event.detail === 0 ? bounds.top + bounds.height / 2 : event.clientY;
        void onChange(next, { x, y });
      }}
    >
      {/* 旋钮与文案不参与视图过渡。 */}
      <span className="app-theme-switch-knob" aria-hidden="true" />
      <span className="app-theme-switch-copy">
        {dark ? <MoonIcon /> : <SunIcon />}
        <span className="app-theme-switch-label">
          {dark ? t.settings.themeDark : t.settings.themeLight}
        </span>
      </span>
    </button>
  );
}
