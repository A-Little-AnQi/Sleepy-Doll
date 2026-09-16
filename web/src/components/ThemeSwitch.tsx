import { useState } from "react";
import { MoonIcon, SunIcon } from "./icons";
import type { ThemeId, ThemeOrigin } from "../appearance";

export function ThemeSwitch({
  theme,
  onChange,
}: {
  theme: ThemeId;
  onChange(next: ThemeId, origin: ThemeOrigin): void | Promise<void>;
}) {
  const dark = theme === "dark";
  const next: ThemeId = dark ? "light" : "dark";
  const [hot, setHot] = useState(false);
  return (
    <button
      type="button"
      className={`app-theme-switch${dark ? " is-dark" : ""}${hot ? " is-hot" : ""}`}
      role="switch"
      aria-checked={dark}
      aria-label={dark ? "切换为白昼" : "切换为黑夜"}
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
      <span className="app-theme-switch-knob" aria-hidden="true" />
      <span className="app-theme-switch-copy">
        {dark ? <SunIcon /> : <MoonIcon />}
        {dark ? "白昼" : "黑夜"}
      </span>
    </button>
  );
}
