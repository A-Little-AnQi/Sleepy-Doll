import { useState } from "react";
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
  const [hot, setHot] = useState(false);
  return (
    <button
      type="button"
      className={`app-theme-switch${dark ? " is-dark" : ""}${hot ? " is-hot" : ""}`}
      role="switch"
      aria-checked={dark}
      aria-label={t.settings.theme}
      onPointerEnter={() => setHot(true)}
      onPointerLeave={() => setHot(false)}
      onClick={(event) => {
        const button = event.currentTarget;
        const x = event.clientX;
        const y = event.clientY;
        void Promise.resolve(onChange(next, { x, y })).finally(() => {
          button.focus({ preventScroll: true });
          const node =
            typeof document.elementFromPoint === "function"
              ? document.elementFromPoint(x, y)
              : button;
          setHot(Boolean(node && (node === button || button.contains(node))));
        });
      }}
    >
      {/* 旋钮与文案不参与视图过渡。 */}
      <span className="app-theme-switch-knob" aria-hidden="true" />
      <span className="app-theme-switch-copy">
        {dark ? <MoonIcon /> : <SunIcon />}
        <span className="app-theme-switch-label">{dark ? t.settings.themeDark : t.settings.themeLight}</span>
      </span>
    </button>
  );
}
