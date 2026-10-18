import type { HTMLAttributes } from "react";

export function Badge({
  active = false,
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement> & { active?: boolean }) {
  return (
    <span
      {...props}
      className={["tag", active && "tag-active", className]
        .filter(Boolean)
        .join(" ")}
    />
  );
}
