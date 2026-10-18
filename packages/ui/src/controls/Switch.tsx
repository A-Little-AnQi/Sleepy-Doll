import "./Switch.css";

export interface SwitchProps {
  checked: boolean;
  onChange(checked: boolean): void;
  label: string;
  disabled?: boolean;
  className?: string;
  role?: "switch" | "menuitemcheckbox";
}

/** 设置、扩展与托盘共用的开关。 */
export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
  className,
  role = "switch",
}: SwitchProps) {
  return (
    <button
      type="button"
      className={["switch", checked && "on", className]
        .filter(Boolean)
        .join(" ")}
      role={role}
      aria-label={label}
      aria-checked={checked}
      aria-disabled={disabled || undefined}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}
