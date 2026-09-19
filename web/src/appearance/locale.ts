import { preference } from "./preference";

export type LocaleId = "zh" | "en";

const LOCALE_KEY = "sleepy-doll-locale";

export const LOCALE_OPTIONS: ReadonlyArray<{ value: LocaleId; label: string }> =
  [
    { value: "zh", label: "简体中文" },
    { value: "en", label: "English" },
  ];

const locale = preference<LocaleId>(
  LOCALE_KEY,
  (stored) => (stored === "en" ? "en" : "zh"),
  (value) => {
    document.documentElement.lang = value === "en" ? "en" : "zh-CN";
  },
);

export const readLocale = locale.read;
export const restoreLocale = locale.restore;

/** 当前界面语言。 */
export function useLocale(): LocaleId {
  return locale.use();
}

export function writeLocale(next: LocaleId) {
  locale.write(next);
}
