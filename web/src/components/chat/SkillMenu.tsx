import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { HistoryIcon, PlusIcon, ToolIcon } from "../icons";
import type { SkillInfo } from "../../ipc/types";
import "./SkillMenu.css";

/** 内建技能的用户可读名称；其余技能按短描述或名字回退，菜单里不再展示 `$命令号`。 */
const SKILL_LABELS: Record<string, string> = {
  "create-shortcut": "创建快捷任务",
};

export function skillLabel(skill: SkillInfo): string {
  const known = SKILL_LABELS[skill.name];
  if (known) return known;
  // 长机器描述截断会变成假标题：只有本身就很短的首句才当名称，否则回退技能名。
  const line = (skill.description.split(/[。；;：:\n（(]/, 1)[0] ?? "").trim();
  return line.length > 0 && line.length <= 24 ? line : skill.name;
}

/** `/` 菜单的筛选：显示名与内部名都可匹配。 */
export function filterSkills(skills: SkillInfo[], query: string): SkillInfo[] {
  const q = query.trim().toLowerCase();
  if (!q) return skills;
  return skills.filter(
    (skill) =>
      skillLabel(skill).toLowerCase().includes(q) ||
      skill.name.toLowerCase().includes(q),
  );
}

/** 面板显示与放置就绪前的过渡态；closed 时不挂 portal。 */
type Phase = "closed" | "opening" | "closing";

/**
 * 输入框左下角的技能入口：列出可手动调用的技能，点选把 `$技能名` 插进输入框。
 * 运行时把 `$技能名` 识别为显式装载指令。
 * 受控 slash 模式：用户在输入框里打 `/` 时由 ChatPage 传入查询与高亮下标，
 * 复用同一份 popover 渲染，焦点保持在输入框里。
 */
export function SkillMenu({
  skills,
  disabled,
  onPick,
  slash,
  onSlashPick,
  onSlashClose,
  anchorElement,
}: {
  skills: SkillInfo[];
  disabled?: boolean;
  onPick(name: string): void;
  slash?: { query: string; index: number } | null;
  onSlashPick?(name: string): void;
  onSlashClose?(): void;
  anchorElement?: HTMLElement | null;
}) {
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<Phase>("closed");
  const [placed, setPlaced] = useState(false);
  const [side, setSide] = useState<"top" | "bottom">("top");
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(0);
  const [top, setTop] = useState(0);
  // 退出阶段冻结最后展示的条目：slash 清空筛选时不能闪回全列表。
  const exitItems = useRef<SkillInfo[]>([]);
  const slashItems = slash ? filterSkills(skills, slash.query) : [];
  const slashOpen = !!slash && slashItems.length > 0;
  const activeIndex =
    slash && slashItems.length
      ? Math.min(slash.index, slashItems.length - 1)
      : -1;
  const popOpen = !disabled && skills.length > 0 && (open || slashOpen);
  const exiting = phase === "closing" || !popOpen;
  // 逻辑打开期间持续同步退出缓存；关闭后冻结，卸载渲染用最后一份。
  if (popOpen) exitItems.current = slashOpen ? slashItems : skills;
  // 手动打开与 slash 受控打开共用一条关闭路径，两种状态一次清干净。
  const close = () => {
    setOpen(false);
    onSlashClose?.();
  };
  useEffect(() => {
    if (disabled || !skills.length) setOpen(false);
  }, [disabled, skills.length]);

  // 入场/退场调度：打开先渲染 hidden 面板，关闭等动画播完再卸载；
  // 快速重开会由 cleanup 取消上一次的卸载定时器，不会卸掉新菜单。
  useEffect(() => {
    if (popOpen) {
      setPlaced(false);
      setPhase("opening");
      return;
    }
    setPhase((current) => {
      if (current === "closed") return current;
      return "closing";
    });
    if (
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      setPhase("closed");
      setPlaced(false);
      return;
    }
    const timer = window.setTimeout(() => {
      setPhase("closed");
      setPlaced(false);
    }, 130);
    return () => window.clearTimeout(timer);
    // 只跟随逻辑开关变化：phase 内部流转不能重触发定时器。
  }, [popOpen]);

  useLayoutEffect(() => {
    if (!popOpen || phase !== "opening") return;
    // slash 模式锚在输入框上；按钮模式锚在 + 按钮。
    const el = slashOpen && anchorElement ? anchorElement : trigger.current;
    const anchor = el?.getBoundingClientRect();
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
    // 输入区在窗口底部：优先向上展开并紧贴锚点上沿，
    // 只有上方放不下才向下；两种方向都不越出视口。
    const above = anchor.top - height - 6;
    if (above >= margin) {
      setTop(above);
      setSide("top");
    } else {
      setTop(
        Math.max(
          margin,
          Math.min(anchor.bottom + 6, window.innerHeight - height - margin),
        ),
      );
      setSide("bottom");
    }
    // placed 只在这里置位：动画必须等定位完成才播，避免 (0,0) 闪现。
    setPlaced(true);
  }, [popOpen, phase, slashOpen, slash?.query, skills, anchorElement]);

  useEffect(() => {
    if (!popOpen) return;
    const onDown = (event: Event) => {
      if (
        !menu.current?.contains(event.target as Node) &&
        !trigger.current?.contains(event.target as Node)
      )
        close();
    };
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // 只有按钮手动打开时把焦点还给 +；slash 模式焦点留在输入框，外点也不抢焦点。
      const manual = !slashOpen && open;
      close();
      if (manual) trigger.current?.focus();
    };
    // 菜单内部滚动（scrollIntoView 高亮项）与锚元素自身的滚动都不算脱锚；
    // 只有外部视口滚动或窗口变化才收起。
    const onScroll = (event: Event) => {
      const target = event.target as Node | null;
      if (menu.current?.contains(target)) return;
      if (
        anchorElement &&
        (target === anchorElement || anchorElement.contains(target))
      )
        return;
      close();
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    // slash 模式焦点必须留在输入框里打筛选词，只有按钮模式把焦点交给菜单。
    // 聚焦要等面板挂载并定位完成（placed + opening），首帧 menu.current 还是 null。
    if (!slashOpen && placed && phase === "opening")
      menu.current?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", close);
    };
  }, [popOpen, slashOpen, open, phase, placed, anchorElement]);

  useLayoutEffect(() => {
    if (!slashOpen || !placed || phase !== "opening") return;
    menu.current
      ?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [slashOpen, activeIndex, slash?.query, phase, placed]);

  const onMenuKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!popOpen || exiting) return;
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

  // 逻辑关闭的一瞬间就按退出处理：冻结条目、禁用按钮，等动画播完再卸载。
  const shownItems = exiting
    ? exitItems.current
    : slashOpen
      ? slashItems
      : skills;
  return (
    <div className="skill-menu" data-open={popOpen}>
      <button
        ref={trigger}
        type="button"
        className="icon-button skill-menu-trigger"
        aria-label="快捷命令"
        title="快捷命令"
        aria-haspopup="menu"
        aria-expanded={popOpen}
        disabled={disabled || !skills.length}
        onClick={() => (popOpen ? close() : setOpen(true))}
      >
        <PlusIcon className="button-icon" />
      </button>
      {phase !== "closed" &&
        createPortal(
          <div
            ref={menu}
            className="skill-menu-popover"
            role="menu"
            aria-label="快捷命令"
            data-state={exiting ? "closing" : placed ? "open" : "placing"}
            data-side={side}
            inert={exiting ? true : undefined}
            aria-hidden={exiting || undefined}
            style={{ left, top, visibility: placed ? undefined : "hidden" }}
            onKeyDown={onMenuKeyDown}
          >
            {shownItems.map((skill, index) => (
              <button
                key={skill.name}
                type="button"
                role="menuitem"
                data-active={slashOpen && index === activeIndex}
                disabled={exiting}
                onClick={() => {
                  if (!popOpen || exiting) return;
                  // 先统一关闭（清掉手动打开与 slash 状态）再插入。
                  close();
                  if (slashOpen) onSlashPick?.(skill.name);
                  else onPick(skill.name);
                }}
              >
                {skill.name === "create-shortcut" ? (
                  <HistoryIcon className="skill-menu-item-icon" />
                ) : (
                  <ToolIcon className="skill-menu-item-icon" />
                )}
                <strong>{skillLabel(skill)}</strong>
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
