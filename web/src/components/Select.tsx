import { useEffect, useId, useRef, useState } from "react";
import { CheckIcon, ChevronIcon } from "./icons";
import styles from "./Select.module.css";

interface Props {
  value: string;
  options: ReadonlyArray<{
    value: string;
    label: string;
    description?: string;
  }>;
  onChange(value: string): void;
  label: string;
  disabled?: boolean;
}

export function Select({ value, options, onChange, label, disabled }: Props) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);
  const selected = options.find((option) => option.value === value);
  const show = () => {
    if (!trigger.current || !options.length) return;
    setCursor(
      Math.max(
        0,
        options.findIndex((option) => option.value === value),
      ),
    );
    setOpen(true);
  };
  const choose = (index: number) => {
    const option = options[index];
    if (option) onChange(option.value);
    setOpen(false);
    trigger.current?.focus();
  };
  useEffect(() => {
    if (!open) return;
    const dismiss = (event: Event) => {
      if (
        !menu.current?.contains(event.target as Node) &&
        !trigger.current?.contains(event.target as Node)
      )
        setOpen(false);
    };
    const close = () => setOpen(false);
    document.addEventListener("pointerdown", dismiss);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      window.removeEventListener("resize", close);
    };
  }, [open]);
  useEffect(() => {
    const list = menu.current;
    const option = list?.querySelector<HTMLElement>(`[data-index="${cursor}"]`);
    if (!list || !option) return;
    if (option.offsetTop < list.scrollTop) list.scrollTop = option.offsetTop;
    else if (
      option.offsetTop + option.offsetHeight >
      list.scrollTop + list.clientHeight
    )
      list.scrollTop =
        option.offsetTop + option.offsetHeight - list.clientHeight;
  }, [cursor, open]);
  return (
    <div
      className={styles["select-root"]}
      data-ui="select-root"
      data-open={open}
    >
      <button
        ref={trigger}
        type="button"
        className={styles["select-trigger"]}
        data-ui="select-trigger"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-haspopup="listbox"
        aria-activedescendant={open ? `${id}-${cursor}` : undefined}
        disabled={disabled || !options.length}
        onClick={() => (open ? setOpen(false) : show())}
        onKeyDown={(event) => {
          if (
            [
              "ArrowDown",
              "ArrowUp",
              "Home",
              "End",
              "Enter",
              " ",
              "Escape",
            ].includes(event.key)
          )
            event.preventDefault();
          if (event.key === "Escape" || event.key === "Tab") setOpen(false);
          else if (event.key === "Enter" || event.key === " ")
            open ? choose(cursor) : show();
          else if (
            ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)
          ) {
            if (!open) show();
            else
              setCursor(
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? options.length - 1
                    : (cursor +
                        (event.key === "ArrowDown" ? 1 : -1) +
                        options.length) %
                      options.length,
              );
          }
        }}
      >
        <span>{selected?.label ?? "选择模型"}</span>
        <ChevronIcon className={styles["select-chevron"] ?? ""} />
      </button>
      {open && (
        <div
          ref={menu}
          id={id}
          className={styles["select-menu"]}
          data-ui="select-menu"
          role="listbox"
          aria-label={label}
        >
          {options.map((option, index) => (
            <div
              key={option.value}
              id={`${id}-${index}`}
              data-index={index}
              role="option"
              data-ui="select-option"
              aria-selected={value === option.value}
              className={`${styles["select-option"]} ${cursor === index ? styles["is-focused"] : ""}`}
              onPointerMove={() => setCursor(index)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              <span>
                <strong>{option.label}</strong>
                {option.description && <small>{option.description}</small>}
              </span>
              {value === option.value && <CheckIcon className="button-icon" />}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
