import { readFileSync } from "node:fs";
import type { Theme } from "loom-frame";

type NativeTokens = {
  design: { radius: number; fontSize: number; borderWidth: number; motionMillis: number; shadow: boolean };
  light: Record<string, string>;
  dark: Record<string, string>;
};

export function readAndroidDesign(): Record<"light" | "dark", Theme> {
  const tokens = JSON.parse(readFileSync(new URL("../packages/ui-android/theme.json", import.meta.url), "utf8")) as NativeTokens;
  const theme = (mode: "light" | "dark"): Theme => ({
    mode, accent: tokens[mode].accent!, surface: tokens[mode].surface!, text: tokens[mode].text!, muted: tokens[mode].muted!, border: tokens[mode].border!,
    radius: tokens.design.radius, fontSize: tokens.design.fontSize, borderWidth: tokens.design.borderWidth, motion: tokens.design.motionMillis, shadow: tokens.design.shadow ? "soft" : "none",
  });
  return { light: theme("light"), dark: theme("dark") };
}
