import type { ReactNode } from "react";
import { Dialog } from "./Dialog";

export function ConfirmDialog({
  open,
  title,
  children,
  confirmLabel,
  busy = false,
  busyLabel,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  busy?: boolean;
  busyLabel?: string;
  onConfirm(): void;
  onClose(): void;
}) {
  return (
    <Dialog
      compact
      open={open}
      onClose={() => {
        if (!busy) onClose();
      }}
      title={title}
      footer={
        <>
          <button
            type="button"
            className="subtle-action"
            disabled={busy}
            onClick={onClose}
          >
            取消
          </button>
          <button
            type="button"
            className="primary-action"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? (busyLabel ?? confirmLabel) : confirmLabel}
          </button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}
