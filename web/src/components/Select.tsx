import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
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

/** 弹层与触发器之间留的空隙。 */
const GAP = 6;
/** 距视口边缘至少留这么多，避免贴边。 */
const MARGIN = 8;

/**
 * 单选下拉。
 *
 * 弹层挂到 `document.body` 上用 fixed 定位：只要留在原地，它就会被祖先元素
 * 的层叠上下文困住（面板的入场动画、`overflow: hidden` 的容器都会），再高的
 * z-index 也救不回来，选项会被后面的内容盖住。挂出去之后层级只由视口决定，
 * 顺便也能在贴边时翻转和收窄。
 */
export function Select({ value, options, onChange, label, disabled }: Props) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);
  const [placement, setPlacement] = useState<{
    left: number;
    /** 向下展开时给 top，向上翻转时给 bottom —— 不用 transform 翻转，
        否则会和入场动画的 transform 打架。 */
    top?: number;
    bottom?: number;
    width: number;
    maxHeight: number;
  }>();
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

  // 摆位在绘制前算好，避免弹层先出现在错误位置再跳一下。
  useLayoutEffect(() => {
    if (!open) {
      setPlacement(undefined);
      return;
    }
    const place = () => {
      const anchor = trigger.current?.getBoundingClientRect();
      if (!anchor) return;
      const width = Math.min(
        Math.max(anchor.width, 240),
        window.innerWidth - MARGIN * 2,
      );
      // 触发器比菜单窄时从左缘往右长，避免窄工具条把 240px 菜单拽进侧栏。
      const left = Math.min(
        Math.max(MARGIN, anchor.width < 240 ? anchor.left : anchor.right - width),
        window.innerWidth - width - MARGIN,
      );
      const below = window.innerHeight - anchor.bottom - GAP - MARGIN;
      const above = anchor.top - GAP - MARGIN;
      // 下方放不下就翻到上面；上面也放不下时选空间更大的一侧并压缩高度。
      const flip = below < 160 && above > below;
      const available = flip ? above : below;
      setPlacement({
        left,
        ...(flip
          ? { bottom: window.innerHeight - anchor.top + GAP }
          : { top: anchor.bottom + GAP }),
        width,
        maxHeight: Math.max(120, Math.min(300, available)),
      });
    };
    place();
    window.addEventListener("resize", place);
    // 页面滚动时跟着走，否则弹层会停在原地和触发器脱开。
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

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
