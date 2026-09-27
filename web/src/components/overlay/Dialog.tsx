import {
  useEffect,
  useLayoutEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { CloseIcon } from "../icons";
import "./Dialog.css";
import { useT } from "../../i18n";

export function Dialog({
  title,
  subtitle,
  open,
  onClose,
  children,
  footer,
  compact = false,
  className = "",
}: {
  title: string;
  subtitle?: ReactNode;
  open: boolean;
  onClose(): void;
  children: ReactNode;
  footer?: ReactNode;
  compact?: boolean;
  className?: string;
}) {
  const t = useT();
  const titleId = useId();
  const layer = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const [present, setPresent] = useState(open);

  useEffect(() => {
    if (open) setPresent(true);
  }, [open]);

  useEffect(() => {
    if (open || !present) return;
    const id = window.setTimeout(() => setPresent(false), 280);
    return () => window.clearTimeout(id);
  }, [open, present]);

  useLayoutEffect(() => {
    const node = layer.current;
    if (!node) return;
    if (open) {
      node.dataset.visible = "false";
      node.getBoundingClientRect();
      node.dataset.visible = "true";
      return;
    }
    node.dataset.visible = "false";
  }, [open, present]);

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
      const field =
        layer.current?.querySelector<HTMLElement>("input, textarea");
      (field ?? focusable()[0])?.focus();
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
        if (event.shiftKey && document.activeElement === first) {
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
      if (previous?.isConnected) previous.focus();
    };
  }, [open]);

  if (!present) return null;
  return createPortal(
    <div ref={layer} className="sd-dialog-layer">
      <div className="sd-dialog-scrim" onClick={onClose} />
      <div
        className={`sd-dialog${compact ? " is-compact" : ""}${className ? ` ${className}` : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="sd-dialog-head">
          <div>
            <h2 id={titleId}>{title}</h2>
            {subtitle ? <p className="sd-dialog-path">{subtitle}</p> : null}
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label={t.common.close}
            title={t.common.close}
            onClick={onClose}
          >
            <CloseIcon className="button-icon" />
          </button>
        </header>
        <div className="sd-dialog-body">{children}</div>
        {footer ? <footer className="sd-dialog-foot">{footer}</footer> : null}
      </div>
    </div>,
    document.body,
  );
}
