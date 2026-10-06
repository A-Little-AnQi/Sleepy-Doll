import type { ReactNode } from "react";
import { CheckIcon } from "../icons";
import "./Checkbox.css";

/**
 * 复选框：真实 input 保留读屏语义与 Space 键操作，
 * 原生外观隐藏后用统一的自定义框绘制选中态。
 */
export function Checkbox({
  checked,
  disabled,
  onChange,
  label,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange(checked: boolean): void;
  label: ReactNode;
}) {
  return (
    <label className={`app-checkbox${disabled ? " is-disabled" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="app-checkbox-box" aria-hidden="true">
        {checked ? <CheckIcon className="app-checkbox-mark" /> : null}
      </span>
      <span className="app-checkbox-text">{label}</span>
    </label>
  );
}
