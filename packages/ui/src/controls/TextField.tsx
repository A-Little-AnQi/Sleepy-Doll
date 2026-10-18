import {
  forwardRef,
  type InputHTMLAttributes,
  type Ref,
  type TextareaHTMLAttributes,
} from "react";
import "./TextField.css";

type InputProps = InputHTMLAttributes<HTMLInputElement> & {
  multiline?: false;
  bare?: boolean;
};

type AreaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  multiline: true;
  bare?: boolean;
};

export const TextField = forwardRef<
  HTMLInputElement | HTMLTextAreaElement,
  InputProps | AreaProps
>(function TextField(props, ref) {
  const { bare, className, multiline, ...rest } = props;
  const cls = ["sd-field", bare ? "is-bare" : "", className]
    .filter(Boolean)
    .join(" ");
  if (multiline) {
    return (
      <textarea
        {...(rest as TextareaHTMLAttributes<HTMLTextAreaElement>)}
        className={cls}
        ref={ref as Ref<HTMLTextAreaElement>}
      />
    );
  }
  return (
    <input
      {...(rest as InputHTMLAttributes<HTMLInputElement>)}
      className={cls}
      ref={ref as Ref<HTMLInputElement>}
    />
  );
});
