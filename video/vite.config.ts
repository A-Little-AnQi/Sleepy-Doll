import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { CACHE, UI } from "./paths.mjs";

// 独立舞台构建，在页面解析前同步注入实测旁白提示点。
export default defineConfig({
  plugins: [
    react(),
    {
      name: "recorded-voice-cues",
      transformIndexHtml() {
        // 全片使用 qwen-ssh 旁白的 director-cues.json（12 段对齐）；旧 Edge cues.json 不再注入。
        const file = resolve(CACHE, "director-cues.json");
        let content: string;
        try {
          content = readFileSync(file, "utf8");
        } catch {
          // 导演段开发构建（VIDEO_DIRECTOR=1）允许缺音频：跳过注入，不伪造时序数据。
          if (process.env.VIDEO_DIRECTOR === "1") return [];
          throw new Error(
            "缺少 director-cues.json：请先运行 video/audio/director-cues.mjs 生成旁白对齐。",
          );
        }
        const cues = JSON.parse(content);
        if (
          cues.version !== 1 ||
          cues.provider !== "qwen-ssh" ||
          cues.segments?.length !== 12 ||
          cues.durationMs !== 60000
        )
          throw new Error(
            "director-cues.json 无效：需要 provider=qwen-ssh、12 段对齐、durationMs=60000。",
          );
        return [
          {
            tag: "script",
            attrs: { id: "director-cues", type: "application/json" },
            children: JSON.stringify(cues).replaceAll("<", "\\u003c"),
            injectTo: "head-prepend",
          },
        ];
      },
    },
  ],
  base: "./",
  root: "video",
  publicDir: false,
  build: {
    outDir: UI,
    emptyOutDir: true,
  },
  server: {
    host: "127.0.0.1",
    port: 5175,
    strictPort: false,
    hmr: { overlay: false },
  },
});
