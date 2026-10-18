import type { ReactNode } from "react";
import "./SettingRow.css";

export function SettingRow({
  label,
  hint,
  children,
  compact = false,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
  compact?: boolean;
}) {
  return (
    <div className={`setting-row${compact ? " is-compact" : ""}`}>
      <div className="setting-row-copy">
        <strong>{label}</strong>
        {hint ? <span className="setting-row-hint">{hint}</span> : null}
      </div>
      <div className="setting-row-control">{children}</div>
    </div>
  );
}
