import type { LabelHTMLAttributes, ReactNode } from "react";

export function FormField({
  label,
  children,
  ...props
}: LabelHTMLAttributes<HTMLLabelElement> & { label: ReactNode }) {
  return (
    <label {...props}>
      <span>{label}</span>
      {children}
    </label>
  );
}
