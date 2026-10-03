import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { useT } from "../../i18n";
import "./context-menu.css";

/** 距视口边缘的最小距离。 */
const MARGIN = 8;

type MenuAction = "cut" | "copy" | "paste" | "selectAll" | "copyLink";

interface TargetState {
  x: number;
  y: number;
  /** 右键命中的可编辑元素；编辑动作要作用在它上面。 */
  editable: HTMLElement | null;
  canCopy: boolean;
  canModify: boolean;
  hasSelection: boolean;
  linkHref: string | null;
}

interface MenuItem {
  id: MenuAction;
  label: string;
  disabled: boolean;
}

function editableElement(node: EventTarget | null): HTMLElement | null {
  if (!(node instanceof HTMLElement)) {
    return null;
  }
  if (node.isContentEditable) {
    return node;
  }
  if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
    return node;
  }
  return null;
}

function selectionExists(): boolean {
  const selection = window.getSelection();
  return (
    selection != null &&
    !selection.isCollapsed &&
    selection.toString().length > 0
  );
}

/** 输入框的内部选区不进 window.getSelection()，要从元素自身取。 */
function editableSelectionExists(el: HTMLElement): boolean {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    return el.selectionStart !== el.selectionEnd;
  }
  const selection = window.getSelection();
  return (
    selection != null &&
    !selection.isCollapsed &&
    selection.rangeCount > 0 &&
    el.contains(selection.anchorNode)
  );
}

/**
 * 自绘右键菜单。
 *
 * 桌面壳已关闭 WebView2 的原生菜单（那会把“刷新／检查”等浏览器项露给用户），
 * 这里补回桌面应用期望的动作：文本编辑（剪切/复制/粘贴/全选）、选区复制和
 * 链接复制。空白处右键不弹菜单，只保持屏蔽，与原生应用一致。
 */
export function ContextMenu() {
  const t = useT();
  const [target, setTarget] = useState<TargetState | null>(null);
  const [active, setActive] = useState(0);
  const menu = useRef<HTMLDivElement>(null);
  const [placement, setPlacement] = useState<{ left: number; top: number }>();

  const close = useCallback(() => {
    setTarget(null);
  }, []);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      // 无论最终是否弹菜单，原生菜单都不能出现。
      event.preventDefault();
      const editable = editableElement(event.target);
      const link =
        event.target instanceof HTMLElement && !editable
          ? event.target.closest("a[href]")
          : null;
      const input = editable instanceof HTMLInputElement ? editable : null;
      const locked =
        editable != null &&
        ((input != null && input.type === "password") ||
          (editable as HTMLInputElement | HTMLTextAreaElement).readOnly);
      setTarget({
        x: event.clientX,
        y: event.clientY,
        editable,
        canCopy: !locked,
        canModify: editable != null && !locked,
        hasSelection:
          !locked &&
          (editable != null
            ? editableSelectionExists(editable)
            : selectionExists()),
        linkHref:
          link instanceof HTMLAnchorElement
            ? new URL(link.href, location.href).href
            : null,
      });
    };
    window.addEventListener("contextmenu", onContextMenu, true);
    return () => window.removeEventListener("contextmenu", onContextMenu, true);
  }, []);

  const items: MenuItem[] = [];
  if (target != null) {
    if (target.editable != null) {
      items.push(
        {
          id: "cut",
          label: t.contextMenu.cut,
          disabled: !target.canModify || !target.hasSelection,
        },
        {
          id: "copy",
          label: t.contextMenu.copy,
          disabled: !target.canCopy || !target.hasSelection,
        },
        {
          id: "paste",
          label: t.contextMenu.paste,
          disabled: !target.canModify,
        },
        { id: "selectAll", label: t.contextMenu.selectAll, disabled: false },
      );
    } else {
      if (target.hasSelection) {
        items.push({ id: "copy", label: t.contextMenu.copy, disabled: false });
      }
      if (target.linkHref != null) {
        items.push({
          id: "copyLink",
          label: t.contextMenu.copyLink,
          disabled: false,
        });
      }
    }
  }
  const hasItems = items.length > 0;

  useEffect(() => {
    if (target == null || !hasItems) {
      return;
    }
    const onPointerDown = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node)) {
        close();
      }
    };
    // 滚动（含 transcript 这类内部容器）与窗口失焦都应收起菜单。
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("scroll", close, true);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("blur", close);
    };
  }, [target, hasItems, close]);

  const run = useCallback(
    (item: MenuItem) => {
      if (item.disabled || target == null) {
        return;
      }
      close();
      switch (item.id) {
        case "cut":
        case "copy":
        case "selectAll":
          // execCommand 是唯一能同步作用于当前选区/焦点的路径；右键与菜单
          // 点击都是用户手势，Chromium 下可用。失败再退回异步剪贴板写入。
          if (!document.execCommand(item.id)) {
            const el = target.editable;
            let text = window.getSelection()?.toString() ?? "";
            if (
              !text &&
              (el instanceof HTMLInputElement ||
                el instanceof HTMLTextAreaElement)
            ) {
              text = el.value.slice(
                el.selectionStart ?? 0,
                el.selectionEnd ?? 0,
              );
            }
            if (item.id === "copy" && text) {
              void navigator.clipboard?.writeText(text);
            }
          }
          break;
        case "paste":
          void navigator.clipboard?.readText().then((text) => {
            if (!text) {
              return;
            }
            target.editable?.focus();
            document.execCommand("insertText", false, text);
          });
          break;
        case "copyLink":
          void navigator.clipboard?.writeText(target.linkHref ?? "");
          break;
      }
    },
    [close, target],
  );

  // 键盘支持放在 window 层：菜单不抢焦点，右键命中的输入框保持焦点，
  // 粘贴动作才不需要先恢复焦点。
  useEffect(() => {
    if (target == null || !hasItems) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        return;
      }
      if (
        event.key !== "ArrowDown" &&
        event.key !== "ArrowUp" &&
        event.key !== "Enter"
      ) {
        return;
      }
      event.preventDefault();
      if (event.key === "Enter") {
        const item = items[active];
        if (item != null) {
          run(item);
        }
        return;
      }
      const step = event.key === "ArrowDown" ? 1 : -1;
      let index = active;
      for (let moved = 0; moved < items.length; moved += 1) {
        index = (index + step + items.length) % items.length;
        if (!items[index]?.disabled) {
          break;
        }
      }
      setActive(index);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [target, hasItems, items, active, run, close]);

  // 换目标后高亮回到第一个可用项。
  useEffect(() => {
    setActive(
      Math.max(
        0,
        items.findIndex((item) => !item.disabled),
      ),
    );
    // items 由 target 派生，重置跟随 target 即可。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  // 先按光标位置渲染，再按实际尺寸贴边翻转；都在提交前完成，不闪。
  useLayoutEffect(() => {
    if (target == null || !hasItems) {
      return;
    }
    const node = menu.current;
    if (node == null) {
      return;
    }
    const left = Math.max(
      MARGIN,
      Math.min(target.x, window.innerWidth - node.offsetWidth - MARGIN),
    );
    const top =
      target.y + node.offsetHeight > window.innerHeight - MARGIN
        ? Math.max(MARGIN, target.y - node.offsetHeight)
        : target.y;
    setPlacement({ left, top });
  }, [target, hasItems]);

  if (target == null || !hasItems) {
    return null;
  }

  return createPortal(
    <div
      ref={menu}
      className="context-menu"
      style={placement}
      role="menu"
      onPointerDown={(event) => event.stopPropagation()}
    >
      {items.map((item, index) => (
        <Fragment key={item.id}>
          {item.id === "copyLink" && index > 0 && (
            <div
              className="context-menu-rule"
              role="separator"
              aria-hidden="true"
            />
          )}
          <button
            type="button"
            role="menuitem"
            className="context-menu-item"
            data-active={index === active || undefined}
            disabled={item.disabled}
            onMouseEnter={() => setActive(index)}
            onClick={() => run(item)}
          >
            {item.label}
          </button>
        </Fragment>
      ))}
    </div>,
    document.body,
  );
}
