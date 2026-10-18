import type { ReactNode } from "react";
import { UiProvider } from "@sleepy-doll/ui";
import { useT } from "../i18n";

/** 产品提供文案；共享组件包不读取产品偏好或调用 IPC。 */
export function ProductUiProvider({ children }: { children: ReactNode }) {
  const text = useT();
  return <UiProvider text={text}>{children}</UiProvider>;
}
