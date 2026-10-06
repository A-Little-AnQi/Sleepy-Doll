import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CheckIcon } from "../icons";
import { DisclosureChevron } from "./DisclosureChevron";
import styles from "./Select.module.css";
import { useT } from "../../i18n";

interface Props {
  value: string;
  options: ReadonlyArray<{
    value: string;
    label: string;
    description?: string;
  }>;
  onChange(value: string): void;
  label: string;
  placeholder?: string;
  disabled?: boolean;
  /** 菜单底部常驻的入口，与选项列表用分隔线隔开。 */
  footer?: { label: string; onClick(): void } | undefined;
}

/** 弹层与触发器之间留的空隙。 */
const GAP = 6;
/** 距视口边缘的最小距离。 */
const MARGIN = 8;

type Placement = {
  left: number;
  top?: number;
  bottom?: number;
  width: number;
  maxHeight: number;
};

function samePlacement(current: Placement | undefined, next: Placement) {
  return (
    current != null &&
    current.left === next.left &&
    current.top === next.top &&
    current.bottom === next.bottom &&
    current.width === next.width &&
    current.maxHeight === next.maxHeight
  );
}

function revealOption(list: HTMLElement, index: number) {
  const option = list.querySelector<HTMLElement>(`[data-index="${index}"]`);
  if (!option) return;
  if (option.offsetTop < list.scrollTop) {
    list.scrollTop = option.offsetTop;
    return;
  }
  const bottom = option.offsetTop + option.offsetHeight;
  if (bottom > list.scrollTop + list.clientHeight) {
    list.scrollTop = bottom - list.clientHeight;
  }
}

/**
 * 单选下拉。
 *
 * 弹层挂到 `document.body` 上用 fixed 定位。
 */
export function Select({
  value,
  options,
  onChange,
  label,
  placeholder,
  disabled,
  footer,
}: Props) {
  const t = useT();
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);
  const [placement, setPlacement] = useState<Placement>();
  /** 只有键盘改焦点时才把选项滚进视口。 */
  const fromKey = useRef(false);
  /** 打开时定下的上下方向。 */
  const side = useRef<"above" | "below">(undefined);
  /** 这次打开是否已经把选中项滚进视口。 */
  const revealed = useRef(false);
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

  // 有说明的选项用更宽的弹层，其余跟触发器对齐。
  const roomy = options.some((option) => option.description);

  // 绘制前算好摆位。
  useLayoutEffect(() => {
    if (!open) {
      setPlacement(undefined);
      side.current = undefined;
      return;
    }
    // 打开时记下边界的初始滚动值：迟到的 scroll 事件（如点击前
    // scrollIntoView 已完成的滚动）值未变，不应误关菜单。
    const boundaryEl = trigger.current?.closest("[data-select-boundary]");
    const boundaryScroll = boundaryEl
      ? { top: boundaryEl.scrollTop, left: boundaryEl.scrollLeft }
      : null;
    const place = (event?: Event) => {
      // 列表自身滚动时不重算定位。
      if (
        event?.target instanceof Node &&
        menu.current?.contains(event.target)
      ) {
        return;
      }
      // 有边界的 Select：resize 直接收起；边界自身的真实滚动（值变化）
      // 也收起，避免触发器滚出可见区后菜单越过边界；值未变的迟到
      // 事件忽略；边界以外的滚动按原逻辑重定位。无边界语义不变。
      if (boundaryEl && event) {
        if (event.type === "resize") {
          setOpen(false);
          return;
        }
        if (event.target instanceof Node && boundaryEl.contains(event.target)) {
          if (
            boundaryEl.scrollTop === boundaryScroll?.top &&
            boundaryEl.scrollLeft === boundaryScroll?.left
          ) {
            return;
          }
          setOpen(false);
          return;
        }
      }
      const anchor = trigger.current?.getBoundingClientRect();
      if (!anchor) return;
      const boundary = boundaryEl?.getBoundingClientRect();
      const limitTop = boundary ? Math.max(MARGIN, boundary.top) : MARGIN;
      const limitBottom = boundary
        ? Math.min(window.innerHeight - MARGIN, boundary.bottom)
        : window.innerHeight - MARGIN;
      const width = Math.min(
        Math.max(anchor.width, roomy ? 240 : 0),
        window.innerWidth - MARGIN * 2,
      );
      const left = Math.min(
        Math.max(MARGIN, anchor.left),
        window.innerWidth - width - MARGIN,
      );
      const below = limitBottom - anchor.bottom - GAP;
      const above = anchor.top - GAP - limitTop;
      if (side.current == null) {
        side.current = below < 160 && above > below ? "above" : "below";
      }
      const flip = side.current === "above";
      const available = flip ? above : below;
      const floor = boundary ? 0 : 120;
      const next: Placement = {
        left,
        ...(flip
          ? { bottom: window.innerHeight - anchor.top + GAP }
          : { top: anchor.bottom + GAP }),
        width,
        maxHeight: Math.max(floor, Math.min(300, available)),
      };
      setPlacement((current) =>
        samePlacement(current, next) ? current : next,
      );
    };
    place();
    window.addEventListener("resize", place);
    // 页面滚动时跟随重算。
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open, roomy]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: Event) => {
      if (
        !menu.current?.contains(event.target as Node) &&
        !trigger.current?.contains(event.target as Node)
      )
        setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  useLayoutEffect(() => {
    if (!open) {
      revealed.current = false;
      fromKey.current = false;
      return;
    }
    if (revealed.current || !menu.current) return;
    revealed.current = true;
    revealOption(menu.current, cursor);
  }, [open, placement]);

  useLayoutEffect(() => {
    if (!open || !fromKey.current || !menu.current) return;
    fromKey.current = false;
    revealOption(menu.current, cursor);
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
        data-empty={selected ? undefined : "true"}
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
            else {
              fromKey.current = true;
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
          }
        }}
      >
        <span>{selected?.label ?? placeholder ?? t.controls.selectPlaceholder}</span>
        <DisclosureChevron expanded={open} />
      </button>
      {open &&
        placement &&
        createPortal(
          <div
            ref={menu}
            id={id}
            className={styles["select-menu"]}
            data-ui="select-menu"
            role="listbox"
            aria-label={label}
            style={{
              left: placement.left,
              top: placement.top,
              bottom: placement.bottom,
              width: placement.width,
              maxHeight: placement.maxHeight,
            }}
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
                {value === option.value && (
                  <CheckIcon className="button-icon" />
                )}
              </div>
            ))}
            {footer && (
              <button
                type="button"
                className={styles["select-footer"]}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  setOpen(false);
                  footer.onClick();
                  trigger.current?.focus();
                }}
              >
                {footer.label}
              </button>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}
