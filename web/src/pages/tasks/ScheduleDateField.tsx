import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { IconButton } from "@sleepy-doll/ui";
import { CalendarIcon, ChevronIcon } from "@sleepy-doll/ui/icons";
import { usePresence } from "@sleepy-doll/ui/motion";
import { useMessage } from "@sleepy-doll/ui/overlay/Message";
import { useSchedulePopover } from "./useSchedulePopover";

const dateText = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
export function validDate(value: string) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T12:00:00`);
  return Number.isFinite(date.getTime()) && dateText(date) === value;
}
export function normalizeDate(value: string) {
  const parts = /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/.exec(value.trim());
  if (!parts) return null;
  const normalized = `${parts[1]}-${parts[2]!.padStart(2, "0")}-${parts[3]!.padStart(2, "0")}`;
  return validDate(normalized) ? normalized : null;
}

export function ScheduleDateField({
  value,
  min,
  label = "执行日期",
  fallback,
  minimumMessage = "执行日期不能早于今天",
  onEditStart,
  onEditCancel,
  onChange,
}: {
  value: string;
  min: string;
  label?: string;
  fallback?: string;
  minimumMessage?: string;
  onEditStart?(): void;
  onEditCancel?(): void;
  onChange(value: string): void;
}) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const normalizedValue = normalizeDate(value);
  const message = useMessage();
  const input = useRef<HTMLInputElement>(null),
    beforeInput = useRef(value);
  const [month, setMonth] = useState(
    () => new Date(`${validDate(value) ? value : (fallback ?? min)}T12:00:00`),
  );
  const anchor = useRef<HTMLDivElement>(null),
    panel = useRef<HTMLDivElement>(null);
  const present = usePresence(open, panel, { y: 4 });
  useSchedulePopover(open, present, anchor, panel);
  useLayoutEffect(() => {
    if (!editing) return;
    input.current?.focus({ preventScroll: true });
    input.current?.select();
  }, [editing]);
  const beginInput = () => {
    if (editing) return;
    message.dismiss();
    beforeInput.current = value;
    onEditStart?.();
    setOpen(false);
    setEditing(true);
  };
  const finishInput = (cancel = false) => {
    message.dismiss();
    if (cancel) {
      onChange(beforeInput.current);
      onEditCancel?.();
    } else {
      const normalized = normalizeDate(value);
      if (normalized && normalized >= min) onChange(normalized);
      else {
        message.error(
          normalized ? minimumMessage : "请输入有效日期：YYYY-MM-DD",
        );
        return false;
      }
    }
    setEditing(false);
    return true;
  };
  const choose = () => {
    if (editing) {
      if (normalizedValue && normalizedValue >= min) onChange(normalizedValue);
      setEditing(false);
    }
    message.dismiss();
    if (!open)
      setMonth(
        new Date(
          `${normalizedValue && normalizedValue >= min ? normalizedValue : (fallback ?? min)}T12:00:00`,
        ),
      );
    setOpen(!open);
  };
  useEffect(() => {
    if (!open) return;
    const outside = (event: Event) => {
      if (
        !anchor.current?.contains(event.target as Node) &&
        !panel.current?.contains(event.target as Node)
      )
        setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      setOpen(false);
      anchor.current?.querySelector("button")?.focus({ preventScroll: true });
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("focusin", outside);
    document.addEventListener("keydown", escape, true);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("focusin", outside);
      document.removeEventListener("keydown", escape, true);
    };
  }, [open]);
  const first = new Date(month.getFullYear(), month.getMonth(), 1, 12);
  const start = new Date(first);
  start.setDate(1 - ((first.getDay() + 6) % 7));
  return (
    <div
      className="schedule-date-field"
      ref={anchor}
      onClick={(event) => {
        if (!event.currentTarget.contains(event.target as Node)) return;
        if (!editing && event.detail < 2) choose();
      }}
      onDoubleClick={(event) => {
        if (!event.currentTarget.contains(event.target as Node)) return;
        if (!editing) {
          event.preventDefault();
          beginInput();
        }
      }}
    >
      <IconButton
        aria-label={`选择${label}`}
        aria-expanded={open}
        onClick={(event) => {
          event.stopPropagation();
          if (event.detail < 2) choose();
        }}
      >
        <CalendarIcon className="button-icon" />
      </IconButton>
      <input
        ref={input}
        aria-label={label}
        aria-keyshortcuts="F2"
        inputMode="numeric"
        placeholder="YYYY-MM-DD"
        value={value}
        maxLength={10}
        readOnly={!editing}
        onMouseDown={(event) => {
          if (!editing) event.preventDefault();
        }}
        onBlur={(event) => {
          if (
            editing &&
            !anchor.current?.contains(event.relatedTarget as Node)
          ) {
            finishInput();
            setEditing(false);
          }
        }}
        onKeyDown={(event) => {
          if (editing && ["Enter", "Escape"].includes(event.key)) {
            event.preventDefault();
            event.stopPropagation();
            if (finishInput(event.key === "Escape"))
              anchor.current
                ?.querySelector("button")
                ?.focus({ preventScroll: true });
          } else if (
            !editing &&
            ["Enter", " ", "ArrowDown", "F2"].includes(event.key)
          ) {
            event.preventDefault();
            event.stopPropagation();
            if (event.key === "F2") beginInput();
            else choose();
          }
        }}
        aria-invalid={!normalizedValue || normalizedValue < min || undefined}
        onChange={(event) => {
          const next = event.target.value.replaceAll("/", "-");
          if (/^[\d-]*$/.test(next)) {
            message.dismiss();
            onChange(next);
          }
        }}
      />
      {present &&
        createPortal(
          <div
            ref={panel}
            className="schedule-date-popover"
            role="group"
            aria-label={`选择${label}日历`}
            inert={!open}
          >
            <header>
              <strong>
                {month.getFullYear()}年{month.getMonth() + 1}月
              </strong>
              <IconButton
                aria-label="上个月"
                onClick={() =>
                  setMonth(
                    new Date(month.getFullYear(), month.getMonth() - 1, 1, 12),
                  )
                }
              >
                <ChevronIcon className="button-icon" />
              </IconButton>
              <IconButton
                aria-label="下个月"
                onClick={() =>
                  setMonth(
                    new Date(month.getFullYear(), month.getMonth() + 1, 1, 12),
                  )
                }
              >
                <ChevronIcon className="button-icon schedule-calendar-next" />
              </IconButton>
            </header>
            <div className="schedule-calendar-week" aria-hidden="true">
              {["一", "二", "三", "四", "五", "六", "日"].map((day) => (
                <span key={day}>{day}</span>
              ))}
            </div>
            <div className="schedule-calendar-days">
              {Array.from({ length: 42 }, (_, index) => {
                const date = new Date(start);
                date.setDate(start.getDate() + index);
                const text = dateText(date);
                return (
                  <button
                    key={text}
                    type="button"
                    aria-label={text}
                    aria-pressed={text === value}
                    data-outside={
                      date.getMonth() !== month.getMonth() || undefined
                    }
                    disabled={text < min}
                    onClick={() => {
                      onChange(text);
                      setOpen(false);
                      anchor.current
                        ?.querySelector("button")
                        ?.focus({ preventScroll: true });
                    }}
                  >
                    {date.getDate()}
                  </button>
                );
              })}
            </div>
          </div>,
          anchor.current?.closest(".sd-dialog-layer") ?? document.body,
        )}
    </div>
  );
}
