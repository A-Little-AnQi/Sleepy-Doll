import type { HTMLAttributes, ReactNode } from "react";
import { PluginIcon } from "./icons";
import { Switch } from "./controls/Switch";
import "./ExtensionRow.css";

export function ExtensionRow({
  name,
  meta,
  description,
  error,
  flag,
  enabled,
  disabled,
  onOpen,
  onChange,
  icon = <PluginIcon />,
}: {
  name: string;
  meta: string;
  description?: string;
  error?: string;
  flag?: string;
  enabled: boolean;
  disabled?: boolean | undefined;
  onOpen(): void;
  onChange(enabled: boolean): void;
  icon?: ReactNode;
}) {
  return (
    <div className="extension-row">
      <div className="extension-glyph">{icon}</div>
      <button className="extension-summary" onClick={onOpen}>
        <strong>{name}</strong>
        <span className="extension-meta">{meta}</span>
        {description && <p>{description}</p>}
        {error && <p>{error}</p>}
        {flag && <p className="extension-flag">{flag}</p>}
      </button>
      <Switch
        label={name}
        checked={enabled}
        disabled={disabled ?? false}
        onChange={onChange}
      />
    </div>
  );
}

export function ExtensionList(props: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className={["extension-list", props.className].filter(Boolean).join(" ")}
    />
  );
}
