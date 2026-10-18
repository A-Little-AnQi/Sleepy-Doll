import type { Theme } from "loom-frame";

/** 默认值始终读取组件包的正式 CSS，不维护另一份配色。 */
export function readDesign(css: string): Record<"light" | "dark", Theme> {
  const variables = (block: string) =>
    Object.fromEntries(
      [...block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [
        m[1]!,
        m[2]!.trim(),
      ]),
    );
  const light = variables(css.match(/:root\s*\{([^}]+)\}/)?.[1] ?? "");
  const dark = {
    ...light,
    ...variables(
      css.match(/:root\[data-theme="dark"\]\s*\{([^}]+)\}/)?.[1] ?? "",
    ),
  };
  const color = (value: string | undefined) => {
    if (!value) throw new Error("组件包缺少默认颜色");
    return /^#[\da-f]{3}$/i.test(value)
      ? "#" + [...value.slice(1)].map((c) => c + c).join("")
      : value;
  };
  const theme = (mode: Theme["mode"], vars: Record<string, string>): Theme => ({
    mode,
    accent: color(vars["--gold"]),
    surface: color(vars["--surface"]),
    text: color(vars["--text"]),
    muted: color(vars["--muted"]),
    border: color(vars["--border"]),
    radius: parseFloat(vars["--radius"]!),
    fontSize: parseFloat(vars["--font-body"]!),
    motion: 240,
    borderWidth: 1,
    shadow: "soft",
  });
  return { light: theme("light", light), dark: theme("dark", dark) };
}
