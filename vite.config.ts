import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  root: "web",
  build: {
    outDir: "../ui-dist",
    emptyOutDir: true,
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/ipc": {
        target: "http://127.0.0.1:47124",
        changeOrigin: true,
        timeout: 120_000,
        proxyTimeout: 120_000,
      },
    },
    watch: process.env.WSL_DISTRO_NAME
      ? {
          usePolling: true,
          interval: 250,
          awaitWriteFinish: { stabilityThreshold: 400, pollInterval: 100 },
        }
      : undefined,
  },
});
