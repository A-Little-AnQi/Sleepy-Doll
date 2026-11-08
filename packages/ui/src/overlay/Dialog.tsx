import { IconButton } from "../Button";
import {
  useEffect,
  useLayoutEffect,
  useId,
  useRef,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { CloseIcon } from "../icons";
import "./Dialog.css";
import { useT } from "../i18n";
import { usePresence } from "../motion";

export function Dialog({
  title,
  subtitle,
  open,
  onClose,
  children,
  footer,
  compact = false,
  className = "",
  initialFocus = "field",
}: {
  title: string;
  subtitle?: ReactNode;
  open: boolean;
  onClose(): void;
  children: ReactNode;
  footer?: ReactNode;
  compact?: boolean;
  className?: string;
  initialFocus?: "field" | "dialog";
}) {
  const t = useT();
  const titleId = useId();
  const layer = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const present = usePresence(open, layer, { content: panel, y: 6, scale: 1 });
  const snapshot = useRef({ title, subtitle, children, footer });
  useLayoutEffect(() => {
    if (open || !present)
      snapshot.current = { title, subtitle, children, footer };
  }, [open, present, title, subtitle, children, footer]);
  const content = open
    ? { title, subtitle, children, footer }
    : snapshot.current;

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const focusable = () =>
      [
        ...(layer.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]',
        ) ?? []),
      ].filter((node) => node.getClientRects().length > 0);
    const frame = requestAnimationFrame(() => {
      if (initialFocus === "dialog") {
        panel.current?.focus({ preventScroll: true });
        return;
      }
      const field =
        layer.current?.querySelector<HTMLElement>("input, textarea");
      (field ?? focusable()[0])?.focus({ preventScroll: true });
    });
    const onKey = (event: KeyboardEvent) => {
      if (
        layer.current !==
        [
          ...document.querySelectorAll('.sd-dialog-layer[data-visible="true"]'),
        ].at(-1)
      )
        return;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        close.current();
      }
      if (event.key === "Tab") {
        const items = focusable(),
          first = items[0],
          last = items.at(-1);
        if (
          event.shiftKey &&
          (document.activeElement === first ||
            document.activeElement === panel.current)
        ) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("keydown", onKey);
      if (previous?.isConnected) previous.focus({ preventScroll: true });
    };
  }, [open, initialFocus]);

  if (!present) return null;
  return createPortal(
    <div
      ref={layer}
      className="sd-dialog-layer"
      data-visible={open}
      inert={!open}
      aria-hidden={!open}
    >
      <div className="sd-dialog-scrim" onClick={onClose} />
      <div
        ref={panel}
        className={`sd-dialog${compact ? " is-compact" : ""}${className ? ` ${className}` : ""}`}
        role="dialog"
        tabIndex={-1}
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="sd-dialog-head">
          <div>
            <h2 id={titleId}>{content.title}</h2>
            {content.subtitle ? (
              <p className="sd-dialog-path">{content.subtitle}</p>
            ) : null}
          </div>
          <IconButton
            type="button"
            aria-label={t.common.close}
            title={t.common.close}
            onClick={onClose}
          >
            <CloseIcon className="button-icon" />
          </IconButton>
        </header>
        <div className="sd-dialog-body">{content.children}</div>
        {content.footer ? (
          <footer className="sd-dialog-foot">{content.footer}</footer>
        ) : null}
      </div>
    </div>,
    document.body,
  );
}
