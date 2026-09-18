import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { CloseIcon } from "../icons";
import "./Dialog.css";

export function Dialog({
  title,
  subtitle,
  open,
  onClose,
  children,
  footer,
  compact = false,
}: {
  title: string;
  subtitle?: ReactNode;
  open: boolean;
  onClose(): void;
  children: ReactNode;
  footer?: ReactNode;
  compact?: boolean;
}) {
  const layer = useRef<HTMLDivElement>(null);
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
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!present) return null;
  return createPortal(
    <div ref={layer} className="sd-dialog-layer">
      <div className="sd-dialog-scrim" onClick={onClose} />
      <div
        className={compact ? "sd-dialog is-compact" : "sd-dialog"}
        role="dialog"
        aria-modal="true"
        aria-labelledby="sd-dialog-title"
      >
        <header className="sd-dialog-head">
          <div>
            <h2 id="sd-dialog-title">{title}</h2>
            {subtitle ? <p className="sd-dialog-path">{subtitle}</p> : null}
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label="关闭"
            title="关闭"
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
