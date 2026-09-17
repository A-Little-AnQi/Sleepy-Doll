import { useId, useState } from "react";
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
  const vt = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  return (
    <button
      type="button"
      className={`app-theme-switch${dark ? " is-dark" : ""}${hot ? " is-hot" : ""}`}
      role="switch"
      aria-checked={dark}
      aria-label="主题"
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
      <span
        className="app-theme-switch-knob"
        style={{ viewTransitionName: `sd-theme-knob-${vt}` }}
        aria-hidden="true"
      />
      <span
        className="app-theme-switch-copy"
        style={{ viewTransitionName: `sd-theme-copy-${vt}` }}
      >
        {dark ? <MoonIcon /> : <SunIcon />}
        <span className="app-theme-switch-label">{dark ? "黑夜" : "白昼"}</span>
      </span>
    </button>
  );
}
