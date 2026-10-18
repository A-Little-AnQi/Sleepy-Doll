import type { HTMLAttributes, ReactNode } from "react";

export function EmptyState({
  icon,
  title,
  description,
  children,
  className,
  ...props
}: Omit<HTMLAttributes<HTMLDivElement>, "title"> & {
  icon: ReactNode;
  title: ReactNode;
  description?: ReactNode;
}) {
  return (
    <div
      {...props}
      className={["empty-state", className].filter(Boolean).join(" ")}
    >
      {icon}
      <h3>{title}</h3>
      {description != null && <p>{description}</p>}
      {children}
    </div>
  );
}
