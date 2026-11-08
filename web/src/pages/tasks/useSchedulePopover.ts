import { useLayoutEffect, type RefObject } from "react";

export function useSchedulePopover(
  open: boolean,
  present: boolean,
  anchor: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
) {
  useLayoutEffect(() => {
    const control = anchor.current,
      popup = panel.current;
    if (!open || !control || !popup) return;
    let frame = 0;
    const position = () => {
      const dialog = control.closest(".sd-dialog")!;
      const bounds = dialog.getBoundingClientRect();
      const head = dialog
        .querySelector(".sd-dialog-head")!
        .getBoundingClientRect();
      const foot = dialog
        .querySelector(".sd-dialog-foot")
        ?.getBoundingClientRect();
      const box = control.getBoundingClientRect();
      const size = { width: popup.offsetWidth, height: popup.offsetHeight };
      const bottom = (foot?.top ?? bounds.bottom) - 10;
      const top =
        box.bottom + 6 + size.height <= bottom
          ? box.bottom + 6
          : box.top - size.height - 6;
      const left = Math.max(
        bounds.left + 12,
        Math.min(box.left, bounds.right - size.width - 12),
      );
      const y = Math.max(head.bottom + 8, Math.min(top, bottom - size.height));
      popup.style.left = `${left}px`;
      popup.style.top = `${y}px`;
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(position);
    };
    position();
    const observer = new ResizeObserver(schedule);
    observer.observe(control);
    observer.observe(popup);
    window.addEventListener("resize", schedule);
    document.addEventListener("scroll", schedule, true);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", schedule);
      document.removeEventListener("scroll", schedule, true);
    };
  }, [open, present, anchor, panel]);
}
