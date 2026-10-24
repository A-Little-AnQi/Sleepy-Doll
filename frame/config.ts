import { existsSync, readFileSync } from "node:fs";
import { androidLibrary, androidPlugin } from "@loom-frame/android";
import type { FrameConfig } from "loom-frame";
import { readDesign } from "./design.ts";
import { readAndroidDesign } from "./android-design.ts";

const androidAssets = "./target/frame-android/assets";
const androidReady = ["preview.apk", "manifest.json", "scrcpy-server"].every(
  (file) =>
    existsSync(new URL("../" + androidAssets + "/" + file, import.meta.url)),
);

export default {
  name: "Sleepy Doll 组件与主题",
  outDir: "target/frame",
  storage: { namespace: "sleepy-doll-ui" },
  libraries: [
    {
      key: "sleepy-doll",
      name: "Sleepy Doll UI",
      description: "主程序和托盘共用的内部组件包。",
      catalog: "./frame/catalog.ts",
      renderer: "./frame/renderer.tsx",
      packages: ["@sleepy-doll/ui"],
      sourceRoots: ["packages/ui/src"],
      styles: ["@sleepy-doll/ui/styles.css", "@sleepy-doll/ui/motion.css"],
      defaults: readDesign(
        readFileSync(
          new URL("../packages/ui/src/styles.css", import.meta.url),
          "utf8",
        ),
      ),
      preview: { scene: true, states: true, highlight: true, language: "tsx" },
    },
    androidLibrary({
      key: "sleepy-doll-android",
      name: "Sleepy Doll Android UI",
      description: "安卓应用和原生预览宿主共用的 Compose 组件库。",
      catalog: "./frame/android-catalog.ts",
      packages: ["dev.sleepydoll.ui"],
      sourceRoots: ["packages/ui-android/src/main"],
      defaults: readAndroidDesign(),
      preview: { viewport: { width: 360, height: 480, minWidth: 280 } },
    }),
  ],
  plugins: [
    androidPlugin({
      enabled: androidReady,
      assets: androidAssets,
      dataDir: "./target/frame-android/runtime",
    }),
  ],
} satisfies FrameConfig;
