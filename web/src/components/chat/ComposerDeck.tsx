import type { ReactNode } from "react";
import "./ComposerDeck.css";

export function ComposerDeck({
  children,
  className,
}: {
  children: ReactNode;
  className?: string | undefined;
}) {
  return (
    <div className={["sd-composer-deck", className].filter(Boolean).join(" ")}>
      {children}
    </div>
  );
}
