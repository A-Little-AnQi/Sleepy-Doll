import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import "./ComposerDeck.css";

export function ComposerDeck({
  children,
  className,
}: {
  children: ReactNode;
  className?: string | undefined;
}) {
  // textarea 高度即行高，卡片上下留白是 deck 的 padding；点空白处也要能进输入。
  const focusComposer = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.defaultPrevented) {
      return;
    }
    if (
      event.target instanceof HTMLElement &&
      event.target.closest(
        "button, a, input, textarea, select, [role=combobox]",
      )
    ) {
      return;
    }
    event.preventDefault();
    event.currentTarget.querySelector<HTMLTextAreaElement>("textarea")?.focus();
  };

  return (
    <div
      className={["sd-composer-deck", className].filter(Boolean).join(" ")}
      onPointerDown={focusComposer}
    >
      {children}
    </div>
  );
}
