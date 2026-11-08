import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { IconButton } from "@sleepy-doll/ui";
import { CyclicNumberWheel } from "@sleepy-doll/ui/controls/CyclicNumberWheel";
import { usePresence } from "@sleepy-doll/ui/motion";
import { useMessage } from "@sleepy-doll/ui/overlay/Message";
import { ClockIcon } from "@sleepy-doll/ui/icons";
import { useSchedulePopover } from "./useSchedulePopover";

export function normalizeTime(value: string) {
  value = value.trim().replaceAll("：", ":");
  if (!/^\d{1,2}:\d{1,2}$/.test(value)) return null;
  const [h = 0, m = 0] = value.split(":").map(Number);
  return h < 24 && m < 60
    ? `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`
    : null;
}

export function ScheduleTimeField({
  value,
  onChange,
  index,
}: {
  value: string;
  onChange(value: string): void;
  index: number;
}) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const message = useMessage();
  const inputPart = useRef(0),
    beforeInput = useRef(value);
  const container = useRef<HTMLDivElement>(null),
    panel = useRef<HTMLDivElement>(null);
  const before = useRef(value);
  const present = usePresence(open, panel, { y: 4 });
  const [hour = "", minute = ""] = value.split(":");
  const valid = normalizeTime(value);
  useLayoutEffect(() => {
    if (!editing) return;
    const input =
      container.current?.querySelectorAll("input")[inputPart.current];
    input?.focus({ preventScroll: true });
    input?.select();
  }, [editing]);
  const beginInput = (part = 0) => {
    if (editing) return;
    message.dismiss();
    beforeInput.current = value;
    inputPart.current = part;
    setOpen(false);
    setEditing(true);
  };
  const finishInput = (cancel = false) => {
    message.dismiss();
    if (cancel) onChange(beforeInput.current);
    else {
      const normalized = normalizeTime(value);
      if (normalized) onChange(normalized);
      else {
        message.error("请输入有效时间：小时 00–23，分钟 00–59");
        return false;
      }
    }
    setEditing(false);
    return true;
  };
  const choose = () => {
    if (open) close();
    else {
      // 无效的手输值仍可通过拨轮纠正。
      if (editing) {
        if (valid) onChange(valid);
        setEditing(false);
      }
      message.dismiss();
      before.current = value;
      setOpen(true);
    }
  };
  const close = (cancel = false) => {
    if (cancel) onChange(before.current);
    setOpen(false);
  };
  useSchedulePopover(open, present, container, panel);
  useEffect(() => {
    if (!open) return;
    const pointer = (event: Event) => {
      if (
        !container.current?.contains(event.target as Node) &&
        !panel.current?.contains(event.target as Node)
      )
        close();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape" && event.key !== "Enter") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      close(event.key === "Escape");
      container.current
        ?.querySelector<HTMLButtonElement>("button")
        ?.focus({ preventScroll: true });
    };
    document.addEventListener("pointerdown", pointer);
    document.addEventListener("focusin", pointer);
    document.addEventListener("keydown", key, true);
    return () => {
      document.removeEventListener("pointerdown", pointer);
      document.removeEventListener("focusin", pointer);
      document.removeEventListener("keydown", key, true);
    };
  }, [open]);
  const setPart = (part: number, text: string) => {
    if (!/^\d{0,2}$/.test(text)) return;
    const parts = value.split(":");
    parts[part] = text;
    message.dismiss();
    onChange(parts.join(":"));
  };
  return (
    <div className="schedule-time-field" ref={container}>
      <div
        className="schedule-time-input"
        role="group"
        aria-label={`执行时刻 ${index + 1}`}
        onClick={(event) => {
          if (!editing && event.detail < 2) choose();
        }}
        onDoubleClick={(event) => {
          if (!editing) {
            event.preventDefault();
            const inputs = container.current?.querySelectorAll("input");
            beginInput(event.target === inputs?.[1] ? 1 : 0);
          }
        }}
        onBlur={(event) => {
          if (
            editing &&
            !event.currentTarget.contains(event.relatedTarget as Node)
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
              container.current
                ?.querySelector("button")
                ?.focus({ preventScroll: true });
          } else if (
            !editing &&
            event.target instanceof HTMLInputElement &&
            ["Enter", " ", "ArrowDown", "F2"].includes(event.key)
          ) {
            event.preventDefault();
            event.stopPropagation();
            if (event.key === "F2")
              beginInput(
                event.target === container.current?.querySelectorAll("input")[1]
                  ? 1
                  : 0,
              );
            else choose();
          }
        }}
        onPaste={(event) => {
          if (!editing || !/[:：]/.test(event.clipboardData.getData("text")))
            return;
          event.preventDefault();
          const normalized = normalizeTime(event.clipboardData.getData("text"));
          if (normalized) {
            message.dismiss();
            onChange(normalized);
          } else message.error("请输入有效时间：小时 00–23，分钟 00–59");
        }}
      >
        <IconButton
          aria-label={`选择时刻 ${index + 1}`}
          aria-expanded={open}
          onClick={(event) => {
            event.stopPropagation();
            if (event.detail < 2) choose();
          }}
        >
          <ClockIcon className="button-icon" />
        </IconButton>
        <input
          aria-label={`时刻 ${index + 1} 小时`}
          inputMode="numeric"
          value={hour}
          maxLength={2}
          readOnly={!editing}
          aria-keyshortcuts="F2"
          aria-invalid={!valid || undefined}
          onFocus={(event) => {
            if (editing) event.target.select();
          }}
          onMouseDown={(event) => {
            if (!editing) event.preventDefault();
          }}
          onChange={(event) => setPart(0, event.target.value)}
        />
        <span>:</span>
        <input
          aria-label={`时刻 ${index + 1} 分钟`}
          inputMode="numeric"
          value={minute}
          maxLength={2}
          readOnly={!editing}
          aria-keyshortcuts="F2"
          aria-invalid={!valid || undefined}
          onFocus={(event) => {
            if (editing) event.target.select();
          }}
          onMouseDown={(event) => {
            if (!editing) event.preventDefault();
          }}
          onChange={(event) => setPart(1, event.target.value)}
        />
      </div>
      {present &&
        createPortal(
          <div
            className="schedule-wheel"
            ref={panel}
            role="group"
            aria-label="循环选择时间"
            data-visible={open}
            inert={!open}
          >
            <div>
              <span>时</span>
              <span>分</span>
            </div>
            <div>
              <CyclicNumberWheel
                label="小时"
                active={open}
                count={24}
                value={
                  Number(hour) >= 0 && Number(hour) < 24 ? Number(hour) : 0
                }
                onChange={(next) =>
                  onChange(
                    `${String(next).padStart(2, "0")}:${String(Number(minute) >= 0 && Number(minute) < 60 ? Number(minute) : 0).padStart(2, "0")}`,
                  )
                }
              />
              <CyclicNumberWheel
                label="分钟"
                active={open}
                count={60}
                value={
                  Number(minute) >= 0 && Number(minute) < 60
                    ? Number(minute)
                    : 0
                }
                onChange={(next) =>
                  onChange(
                    `${String(Number(hour) >= 0 && Number(hour) < 24 ? Number(hour) : 0).padStart(2, "0")}:${String(next).padStart(2, "0")}`,
                  )
                }
              />
            </div>
          </div>,
          container.current?.closest(".sd-dialog-layer") ?? document.body,
        )}
    </div>
  );
}
