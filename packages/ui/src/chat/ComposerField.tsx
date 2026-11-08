import {
  forwardRef,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type TextareaHTMLAttributes,
} from "react";
import { TextField } from "../controls/TextField";
import "./ComposerField.css";

export const ComposerField = forwardRef<
  HTMLTextAreaElement,
  Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "rows"> & {
    highlightTerms?: readonly string[];
  }
>(function ComposerField(props, ref) {
  const { highlightTerms, ...fieldProps } = props;
  const inner = useRef<HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const [uncontrolled, setUncontrolled] = useState(
    String(props.defaultValue ?? ""),
  );
  const value = String(props.value ?? uncontrolled);
  const highlighted = useMemo(() => {
    const terms = [...new Set(highlightTerms ?? [])]
      .filter(Boolean)
      .sort((a, b) => b.length - a.length);
    if (!terms.length) return { found: false, parts: [] as ReactNode[] };
    const escaped = terms.map((term) =>
      term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    );
    const pattern = new RegExp(`(^|\\s)(${escaped.join("|")})(?=$|\\s)`, "gu");
    const parts: ReactNode[] = [];
    let offset = 0,
      found = false;
    for (const match of value.matchAll(pattern)) {
      const start = match.index + match[1]!.length;
      parts.push(value.slice(offset, start));
      parts.push(<mark key={start}>{match[2]}</mark>);
      offset = start + match[2]!.length;
      found = true;
    }
    parts.push(value.slice(offset), "\u200b");
    return { found, parts };
  }, [value, highlightTerms]);

  useLayoutEffect(() => {
    const node = inner.current;
    if (!node) return;
    if (CSS.supports("field-sizing", "content")) return;
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
  }, [value]);

  useLayoutEffect(() => {
    const area = inner.current,
      layer = mirror.current;
    if (!area || !layer) return;
    const sync = () => {
      const style = getComputedStyle(area);
      layer.style.font = style.font;
      layer.style.letterSpacing = style.letterSpacing;
      layer.style.wordSpacing = style.wordSpacing;
      layer.style.tabSize = style.tabSize;
      layer.style.textAlign = style.textAlign;
      layer.style.padding = style.padding;
      layer.style.width = `${area.getBoundingClientRect().width - (area.offsetWidth - area.clientWidth)}px`;
      layer.style.height = `${area.clientHeight}px`;
      layer.scrollTop = area.scrollTop;
      layer.scrollLeft = area.scrollLeft;
    };
    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(area);
    return () => observer.disconnect();
  }, [value, highlightTerms !== undefined]);

  const field = (
    <TextField
      {...fieldProps}
      onChange={(event) => {
        if (props.value === undefined && highlightTerms !== undefined)
          setUncontrolled(event.currentTarget.value);
        props.onChange?.(event);
      }}
      onScroll={(event) => {
        if (mirror.current) {
          mirror.current.scrollTop = event.currentTarget.scrollTop;
          mirror.current.scrollLeft = event.currentTarget.scrollLeft;
        }
        props.onScroll?.(event);
      }}
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
  return highlightTerms === undefined ? (
    field
  ) : (
    <div className="sd-composer-editor" data-highlighted={highlighted.found}>
      <div
        ref={mirror}
        className="sd-composer-highlights"
        aria-hidden="true"
        hidden={!highlighted.found}
      >
        {highlighted.parts}
      </div>
      {field}
    </div>
  );
});
