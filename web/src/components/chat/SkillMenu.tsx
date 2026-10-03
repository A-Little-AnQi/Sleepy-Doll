import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { PlusIcon } from "../icons";
import type { SkillInfo } from "../../ipc/types";
import "./SkillMenu.css";

/**
 * 输入框左下角的技能入口：列出可手动调用的技能，点选把 `$技能名` 插进输入框。
 * 运行时把 `$技能名` 识别为显式装载指令。
 */
export function SkillMenu({
  skills,
  disabled,
  onPick,
}: {
  skills: SkillInfo[];
  disabled?: boolean;
  onPick(name: string): void;
}) {
  const [open, setOpen] = useState(false);
  const [placed, setPlaced] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(0);
  const [top, setTop] = useState(0);

  useLayoutEffect(() => {
    // 关闭时复位，让下一次打开先以 hidden 渲染、量完尺寸再显示，
    // 不会带着上一次的坐标闪现。重置必须留在 layout effect 里：
    // passive effect 跑在其后，会把同一次打开刚设置的 placed 又抹掉。
    if (!open) {
      setPlaced(false);
      return;
    }
    const anchor = trigger.current?.getBoundingClientRect();
    const pop = menu.current;
    if (!anchor || !pop) return;
    const margin = 8;
    // 面板尺寸按实际渲染取：条目数量不同高度不同，不能只在 open 时拍一次。
    const width = pop.offsetWidth;
    const height = pop.offsetHeight;
    setLeft(
      Math.max(
        margin,
        Math.min(anchor.left, window.innerWidth - width - margin),
      ),
    );
    // 触发按钮在窗口底部的输入区：优先向上展开并紧贴按钮上沿，
    // 只有上方放不下才向下；两种方向都不越出视口。
    const above = anchor.top - height - 6;
    const top1 =
      above >= margin
        ? above
        : Math.min(anchor.bottom + 6, window.innerHeight - height - margin);
    setTop(Math.max(margin, top1));
    setPlaced(true);
  }, [open, skills]);

  useEffect(() => {
    if (!open) return;
    const onDown = (event: Event) => {
      if (
        !menu.current?.contains(event.target as Node) &&
        !trigger.current?.contains(event.target as Node)
      )
        setOpen(false);
    };
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    // 定位锚在视口坐标上：页面滚动或窗口变化后面板会脱锚，直接收起。
    const close = () => setOpen(false);
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    menu.current?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  const onMenuKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const items = Array.from(
      menu.current?.querySelectorAll<HTMLButtonElement>("button") ?? [],
    );
    if (!items.length) return;
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) %
      items.length;
    items[next]?.focus();
  };

  return (
    <div className="skill-menu" data-open={open}>
      <button
        ref={trigger}
        type="button"
        className="icon-button skill-menu-trigger"
        aria-label="选择技能"
        title="选择技能"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled || !skills.length}
        onClick={() => setOpen(!open)}
      >
        <PlusIcon className="button-icon" />
      </button>
      {open &&
        createPortal(
          <div
            ref={menu}
            className="skill-menu-popover"
            role="menu"
            aria-label="技能"
            style={{ left, top, visibility: placed ? undefined : "hidden" }}
            onKeyDown={onMenuKeyDown}
          >
            {skills.map((skill) => (
              <button
                key={skill.name}
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  onPick(skill.name);
                }}
              >
                <strong>{skill.description}</strong>
                <small>${skill.name}</small>
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
