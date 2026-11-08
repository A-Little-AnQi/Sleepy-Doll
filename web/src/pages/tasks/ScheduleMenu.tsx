import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IconButton } from "@sleepy-doll/ui";
import { MoreIcon } from "@sleepy-doll/ui/icons";
import { usePresence } from "@sleepy-doll/ui/motion";

export function ScheduleMenu({
  open,
  point,
  label,
  onToggle,
  onClose,
  children,
}: {
  open: boolean;
  point: { x: number; y: number } | null;
  label: string;
  onToggle(): void;
  onClose(): void;
  children: ReactNode;
}) {
  const anchor = useRef<HTMLDivElement>(null),
    panel = useRef<HTMLDivElement>(null);
  const present = usePresence(open, panel, { y: 4, scale: 0.985 });
  useLayoutEffect(() => {
    const node = panel.current,
      trigger = anchor.current?.querySelector("button");
    if (!open || !node || !trigger) return;
    const box = trigger.getBoundingClientRect();
    const width = node.offsetWidth,
      height = node.offsetHeight;
    node.style.left = `${Math.max(8, Math.min(point?.x ?? box.right - width, innerWidth - width - 8))}px`;
    node.style.top = `${Math.max(8, Math.min(point?.y ?? box.bottom + 6, innerHeight - height - 8))}px`;
    node.style.transformOrigin = point ? "top left" : "top right";
  }, [open, present, point]);
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() =>
      panel.current
        ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
        ?.focus({ preventScroll: true }),
    );
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !anchor.current?.contains(target))
        onClose();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
      anchor.current?.querySelector("button")?.focus({ preventScroll: true });
    };
    const scroll = (event: Event) => {
      if (!panel.current?.contains(event.target as Node)) onClose();
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape, true);
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", onClose);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape, true);
      document.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", onClose);
    };
  }, [open, onClose]);
  return (
    <div ref={anchor} className="schedule-row-menu">
      <IconButton
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
      >
        <MoreIcon className="button-icon" />
      </IconButton>
      {present &&
        createPortal(
          <div
            ref={panel}
            className="task-menu schedule-action-menu"
            role="menu"
            aria-label={label}
            aria-hidden={!open}
            data-context-menu-owner="schedule"
            onClickCapture={(event) => {
              if (open && (event.target as Element).closest("button"))
                anchor.current
                  ?.querySelector("button")
                  ?.focus({ preventScroll: true });
            }}
            onKeyDown={(event) => {
              if (event.key === "Tab") {
                onClose();
                anchor.current
                  ?.querySelector("button")
                  ?.focus({ preventScroll: true });
              } else if (
                ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)
              ) {
                event.preventDefault();
                event.stopPropagation();
                const buttons = [
                  ...event.currentTarget.querySelectorAll<HTMLButtonElement>(
                    "button:not(:disabled)",
                  ),
                ];
                const index = buttons.indexOf(
                  document.activeElement as HTMLButtonElement,
                );
                const next =
                  event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? buttons.length - 1
                      : (index +
                          (event.key === "ArrowDown" ? 1 : -1) +
                          buttons.length) %
                        buttons.length;
                buttons[next]?.focus({ preventScroll: true });
              }
            }}
            onPointerDown={(event) => {
              event.stopPropagation();
              if (!open) event.preventDefault();
            }}
            onClick={(event) => {
              event.stopPropagation();
              if (!open) event.preventDefault();
            }}
            onContextMenu={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
          >
            <div inert={!open}>{children}</div>
          </div>,
          document.body,
        )}
    </div>
  );
}
