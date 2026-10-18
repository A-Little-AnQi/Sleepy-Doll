import { existsSync } from "node:fs";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

export const frameToolsDirectory = fileURLToPath(
  new URL("../frame/", import.meta.url),
);
const packages = new Set(["loom-frame", "@loom-frame/android"]);
export function frameToolPackage(name) {
  if (!packages.has(name))
    throw new Error("Unknown preview tool package: " + name);
  const directory = resolve(frameToolsDirectory, "node_modules", name);
  if (!existsSync(resolve(directory, "package.json"))) {
    throw new Error(
      "本地预览工具尚未安装。运行 npm run frame:setup -- <loom-frame.tgz> <android-plugin.tgz>；产品构建无需安装它。",
    );
  }
  return directory;
}
export function frameToolFile(name, filename) {
  const directory = frameToolPackage(name);
  const file = resolve(directory, filename);
  if (!file.startsWith(directory + sep) || !existsSync(file))
    throw new Error("预览工具缺少文件：" + file);
  return file;
}
export const frameToolRequire = createRequire(
  new URL("../frame/package.json", import.meta.url),
);
