import { createContext, useContext, type ReactNode } from "react";

export const defaultUiText = {
  common: { close: "关闭", cancel: "取消" },
  controls: { selectPlaceholder: "请选择" },
  settings: { theme: "主题", themeDark: "黑夜", themeLight: "白昼" },
  taskCard: {
    pinned: "已置顶",
    unpin: "取消置顶",
    pin: "置顶",
    restore: "恢复",
    archive: "归档",
    noDescription: "暂无说明",
    offlineNote: "运行时不会向模型发送请求",
    offlineShort: "运行不调用模型",
    open: "打开",
    sourceDeleted: "来源对话已删除",
  },
};
export type UiText = typeof defaultUiText;
const UiTextContext = createContext<UiText>(defaultUiText);
export function UiProvider({
  text = defaultUiText,
  children,
}: {
  text?: UiText;
  children: ReactNode;
}) {
  return (
    <UiTextContext.Provider value={text}>{children}</UiTextContext.Provider>
  );
}
export const useT = () => useContext(UiTextContext);
