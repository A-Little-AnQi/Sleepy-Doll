import { afterEach, expect, it } from "vitest";
import { readLocale, writeLocale } from "./locale";

afterEach(() => localStorage.clear());

it("stores the locale and maps it onto html lang", () => {
  expect(readLocale()).toBe("zh");
  writeLocale("en");
  expect(readLocale()).toBe("en");
  expect(document.documentElement.lang).toBe("en");
});
