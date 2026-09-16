export type LocaleId = "zh" | "en";

const LOCALE_KEY = "sleepy-doll-locale";

export const LOCALE_OPTIONS: ReadonlyArray<{ value: LocaleId; label: string }> =
  [
    { value: "zh", label: "简体中文" },
    { value: "en", label: "English" },
  ];

export function readLocale(): LocaleId {
  return localStorage.getItem(LOCALE_KEY) === "en" ? "en" : "zh";
}

export function writeLocale(locale: LocaleId) {
  localStorage.setItem(LOCALE_KEY, locale);
  document.documentElement.lang = locale === "en" ? "en" : "zh-CN";
}
