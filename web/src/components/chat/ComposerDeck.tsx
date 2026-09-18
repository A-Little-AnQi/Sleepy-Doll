import type { ReactNode } from "react";
import "./ComposerDeck.css";

export function ComposerDeck({ children }: { children: ReactNode }) {
  return <div className="sd-composer-deck">{children}</div>;
}
