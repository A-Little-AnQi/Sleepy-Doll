import type { HTMLAttributes } from "react";
import "./SettingsScope.css";

/** 设置区域的字号和控件尺寸上下文；布局由调用方决定。 */
export function SettingsScope({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className={["sd-settings-scope", className].filter(Boolean).join(" ")}
    />
  );
}
