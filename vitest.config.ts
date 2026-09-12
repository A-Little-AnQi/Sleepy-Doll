import { defineConfig } from "vitest/config";

export default defineConfig({
  esbuild: { jsx: "automatic" },
  test: {
    environment: "jsdom",
    include: ["web/src/**/*.test.{ts,tsx}"],
    clearMocks: true,
  },
});
