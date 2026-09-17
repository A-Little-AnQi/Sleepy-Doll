import { useRef } from "react";
import { TextField } from "./TextField";
import "./InlineRename.css";

export function InlineRename({
  value,
  label,
  onChange,
  onSubmit,
  onCancel,
}: {
  value: string;
  label: string;
  onChange(value: string): void;
  onSubmit(): void;
  onCancel(): void;
}) {
  const skipBlur = useRef(false);
  return (
    <form
      className="sd-inline-rename"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <TextField
        aria-label={label}
        value={value}
        autoFocus
        onChange={(event) => onChange(event.target.value)}
        onBlur={() => {
          if (skipBlur.current) return;
          onSubmit();
        }}
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          skipBlur.current = true;
          onCancel();
        }}
      />
    </form>
  );
}
