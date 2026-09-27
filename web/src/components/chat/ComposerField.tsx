import {
  forwardRef,
  useLayoutEffect,
  useRef,
  type TextareaHTMLAttributes,
} from "react";
import { TextField } from "../controls/TextField";
import "./ComposerField.css";

export const ComposerField = forwardRef<
  HTMLTextAreaElement,
  Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "rows">
>(function ComposerField(props, ref) {
  const inner = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const node = inner.current;
    if (!node) return;
    const resize = () => {
      node.style.height = "auto";
      node.style.height = `${Math.min(node.scrollHeight, 180)}px`;
    };
    resize();
    // 换行也受窗口/侧栏宽度影响；仅观察宽度，避免高度调整触发循环。
    let width = node.clientWidth;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth === width) return;
      width = node.clientWidth;
      resize();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [props.value]);

  return (
    <TextField
      {...props}
      className={["sd-composer-field", props.className]
        .filter(Boolean)
        .join(" ")}
      multiline
      bare
      rows={1}
      ref={(node) => {
        const area = node as HTMLTextAreaElement | null;
        inner.current = area;
        if (typeof ref === "function") ref(area);
        else if (ref) ref.current = area;
      }}
    />
  );
});
