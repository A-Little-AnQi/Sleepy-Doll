import {
  useEffect,
  useLayoutEffect,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { api } from "../api";
import "./title-bar.css";

/** 自绘的窗口标题栏。无边框窗口里系统不再画标题栏，这条替代它。
 * 只在桌面壳注入了 frameless 标记时由调用方挂上。 */
export function TitleBar({
  canMaximize = true,
  closeDisabled = false,
}: {
  canMaximize?: boolean;
  closeDisabled?: boolean;
} = {}) {
  const [maximized, setMaximized] = useState(false);

  useLayoutEffect(() => {
    // 布局前打标记：CSS 靠它把标题栏的高度让出来，晚一帧会看到内容跳一下。
    document.documentElement.dataset.frameless = "true";
    return () => {
      delete document.documentElement.dataset.frameless;
    };
  }, []);

  useEffect(() => {
    window.__sleepyDollWindow = (state) => setMaximized(state.maximized);
    void api.windowState();
    return () => {
      delete window.__sleepyDollWindow;
    };
  }, []);

  useLayoutEffect(() => {
    // 最大化后窗口铺满屏幕，原生侧不再切圆角，界面跟着方。
    if (maximized) document.documentElement.dataset.maximized = "true";
    else delete document.documentElement.dataset.maximized;
    return () => {
      delete document.documentElement.dataset.maximized;
    };
  }, [maximized]);

  const onDragPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    // 只响应主键，也只在标题栏本身上起拖：控制按钮自己处理点击。
    if (event.button !== 0) return;
    if (isControl(event.target)) return;
    void api.windowDrag();
  };

  const onDragDoubleClick = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (!canMaximize) return;
    if (isControl(event.target)) return;
    void api.windowToggleMaximize();
  };

  return (
    <div
      className="title-bar"
      onPointerDown={onDragPointerDown}
      onDoubleClick={onDragDoubleClick}
    >
      <div className="title-bar-controls">
        <button
          type="button"
          className="title-bar-button"
          title="最小化"
          aria-label="最小化"
          onClick={() => void api.windowMinimize()}
        >
          <Glyph>
            <path d="M1 6h10" />
          </Glyph>
        </button>
        {canMaximize ? (
          <button
            type="button"
            className="title-bar-button"
            title={maximized ? "向下还原" : "最大化"}
            aria-label={maximized ? "向下还原" : "最大化"}
            onClick={() => void api.windowToggleMaximize()}
          >
            {maximized ? (
              <Glyph>
                <path d="M3.5 3.5v-2h7v7h-2" />
                <rect x="1.5" y="3.5" width="7" height="7" />
              </Glyph>
            ) : (
              <Glyph>
                <rect x="1.5" y="1.5" width="9" height="9" />
              </Glyph>
            )}
          </button>
        ) : null}
        <button
          type="button"
          className="title-bar-button title-bar-close"
          title="关闭"
          aria-label="关闭"
          disabled={closeDisabled}
          onClick={() => void api.windowClose()}
        >
          <Glyph>
            <path d="m1.5 1.5 9 9M10.5 1.5l-9 9" />
          </Glyph>
        </button>
      </div>
    </div>
  );
}

function isControl(target: EventTarget | null) {
  return target instanceof Element && target.closest(".title-bar-controls");
}

function Glyph({ children }: { children: React.ReactNode }) {
  return (
    <svg
      className="title-bar-glyph"
      viewBox="0 0 12 12"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}
