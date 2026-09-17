import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CheckIcon } from "./icons";
import { DisclosureChevron } from "./DisclosureChevron";
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
  placeholder?: string;
  disabled?: boolean;
}

/** 弹层与触发器之间留的空隙。 */
const GAP = 6;
/** 距视口边缘至少留这么多，避免贴边。 */
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
 * 弹层挂到 `document.body` 上用 fixed 定位：只要留在原地，它就会被祖先元素
 * 的层叠上下文困住（面板的入场动画、`overflow: hidden` 的容器都会），再高的
 * z-index 也救不回来，选项会被后面的内容盖住。挂出去之后层级只由视口决定，
 * 顺便也能在贴边时翻转和收窄。
 */
export function Select({
  value,
  options,
  onChange,
  label,
  placeholder = "请选择",
  disabled,
}: Props) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);
  const [placement, setPlacement] = useState<Placement>();
  /** 键盘改焦点时才把选项滚进视口；指针划过不能拽列表，否则一滚就和滚动对着干。 */
  const fromKey = useRef(false);
  /** 打开时定下上下方向，滚动跟随不再翻转，避免贴阈值时整层对跳。 */
  const side = useRef<"above" | "below">(undefined);
  /** 当前这次打开是否已经把选中项滚进视口；placement 后续更新不能再拽列表。 */
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

  // 有说明的选项需要比触发器更宽；语言这类短选项跟触发器对齐，
  // 否则侧栏菜单里会出现 128px 控件配 240px 弹层。
  const roomy = options.some((option) => option.description);

  // 摆位在绘制前算好，避免弹层先出现在错误位置再跳一下。
  useLayoutEffect(() => {
    if (!open) {
      setPlacement(undefined);
      side.current = undefined;
      return;
    }
    const place = (event?: Event) => {
      // 列表自己滚时不要重算定位：会改 maxHeight、还会和滚轮抢 scrollTop。
      if (event?.target instanceof Node && menu.current?.contains(event.target)) {
        return;
      }
      const anchor = trigger.current?.getBoundingClientRect();
      if (!anchor) return;
      const width = Math.min(
        Math.max(anchor.width, roomy ? 240 : 0),
        window.innerWidth - MARGIN * 2,
      );
      const left = Math.min(
        Math.max(MARGIN, anchor.left),
        window.innerWidth - width - MARGIN,
      );
      const below = window.innerHeight - anchor.bottom - GAP - MARGIN;
      const above = anchor.top - GAP - MARGIN;
      if (side.current == null) {
        side.current = below < 160 && above > below ? "above" : "below";
      }
      const flip = side.current === "above";
      const available = flip ? above : below;
      const next: Placement = {
        left,
        ...(flip
          ? { bottom: window.innerHeight - anchor.top + GAP }
          : { top: anchor.bottom + GAP }),
        width,
        maxHeight: Math.max(120, Math.min(300, available)),
      };
      setPlacement((current) => (samePlacement(current, next) ? current : next));
    };
    place();
    window.addEventListener("resize", place);
    // 页面滚动时跟着走，否则弹层会停在原地和触发器脱开。
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
        <span>{selected?.label ?? placeholder}</span>
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
          </div>,
          document.body,
        )}
    </div>
  );
}
