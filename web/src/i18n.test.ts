import { describe, expect, it } from "vitest";
import * as i18n from "./i18n";

/** en 与 zh 的键结构必须完全一致：翻译漏一个键都是运行期 undefined。 */
function keysOf(dict: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(dict).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof value === "object" && value !== null
      ? keysOf(value as Record<string, unknown>, path)
      : [path];
  });
}

describe("i18n", () => {
  const locales = ["zh", "en"] as const;

  it("exposes the same key tree in every locale", () => {
    const [base, ...rest] = locales.map((id) =>
      keysOf(i18n.dictOf(id) as unknown as Record<string, unknown>).sort(),
    );
    for (const other of rest) {
      expect(other).toEqual(base);
    }
  });

  it("has no empty strings", () => {
    for (const id of locales) {
      const dict = i18n.dictOf(id) as unknown as Record<string, unknown>;
      for (const key of keysOf(dict)) {
        const value = key
          .split(".")
          .reduce<unknown>(
            (acc, part) => (acc as Record<string, unknown>)[part],
            dict,
          );
        if (typeof value === "string") expect(value.length).toBeGreaterThan(0);
      }
    }
  });
});
