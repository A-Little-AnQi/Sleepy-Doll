import type { ButtonHTMLAttributes } from "react";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "runtime" | "subtle";
};

/** 沿用产品的四种操作按钮，保留原生表单提交语义。 */
export function Button({
  variant = "secondary",
  className,
  ...props
}: ButtonProps) {
  return (
    <button
      {...props}
      className={[`${variant}-action`, className].filter(Boolean).join(" ")}
    />
  );
}

export function IconButton({
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...props}
      className={["icon-button", className].filter(Boolean).join(" ")}
    />
  );
}
