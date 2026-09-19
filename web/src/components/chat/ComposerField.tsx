import {
  forwardRef,
  useEffect,
  useRef,
  type TextareaHTMLAttributes,
} from "react";
import { TextField } from "../controls/TextField";

export const ComposerField = forwardRef<
  HTMLTextAreaElement,
  Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "rows">
>(function ComposerField(props, ref) {
  const inner = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const node = inner.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${Math.min(node.scrollHeight, 180)}px`;
  }, [props.value]);

  return (
    <TextField
      {...props}
      multiline
      bare
      rows={2}
      ref={(node) => {
        const area = node as HTMLTextAreaElement | null;
        inner.current = area;
        if (typeof ref === "function") ref(area);
        else if (ref) ref.current = area;
      }}
    />
  );
});
