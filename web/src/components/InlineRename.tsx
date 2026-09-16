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
        onBlur={onCancel}
        onKeyDown={(event) => {
          if (event.key === "Escape") onCancel();
        }}
      />
    </form>
  );
}
