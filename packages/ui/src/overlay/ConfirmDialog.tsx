import { Button } from "../Button";
import type { ReactNode } from "react";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

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
  const t = useT();
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
          <Button
            type="button"
            variant="subtle"
            disabled={busy}
            onClick={onClose}
          >
            {t.common.cancel}
          </Button>
          <Button
            type="button"
            variant="primary"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? (busyLabel ?? confirmLabel) : confirmLabel}
          </Button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}
