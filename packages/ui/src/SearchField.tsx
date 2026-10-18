import type { InputHTMLAttributes } from "react";
import { SearchIcon } from "./icons";

/** 输入框保持原来的原生样式；label 是搜索框定位容器。 */
export function SearchField({
  compact = false,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { compact?: boolean }) {
  return (
    <label className={compact ? "search-field is-compact" : "search-field"}>
      <SearchIcon />
      <input {...props} />
    </label>
  );
}
