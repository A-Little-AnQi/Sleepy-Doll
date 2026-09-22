import { defineConfig } from "vitest/config";

export default defineConfig({
  esbuild: { jsx: "automatic" },
  // 与 vite.config.ts 的 define 保持一致：版本号来源是 package.json。
  define: {
    __APP_VERSION__: JSON.stringify("0.0.0-test"),
  },
  test: {
    environment: "jsdom",
    include: ["web/src/**/*.test.{ts,tsx}"],
    clearMocks: true,
  },
});
