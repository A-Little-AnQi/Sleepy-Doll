import type { ReactNode } from "react";

export type MotionKind = "page" | "panel";

/** viewKey 变化时重建子树，重放入场动画。 */
export function MotionSwitch({
  viewKey,
  kind = "page",
  children,
  className,
}: {
  viewKey: string;
  kind?: MotionKind;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      key={viewKey}
      className={["motion-switch", className].filter(Boolean).join(" ")}
      data-motion={kind}
    >
      {children}
    </div>
  );
}
