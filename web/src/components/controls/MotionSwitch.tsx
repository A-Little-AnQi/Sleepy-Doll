import type { ReactNode } from "react";

export type MotionKind = "page" | "panel";

/** 切页 / 切 Tab / 切对话时重挂子树，重播同一套入场。 */
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
