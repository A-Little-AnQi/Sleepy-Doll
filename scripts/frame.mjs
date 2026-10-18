// Frame 与产品共用 workspace 组件包。运行时临时数据固定在项目 target 内。
import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { frameToolFile, frameToolsDirectory } from "./frame-tooling.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const command = process.argv[2] ?? "dev";
if (!["dev", "build", "serve", "setup", "typecheck"].includes(command))
  throw new Error("未知 Frame 命令：" + command);
let executable;
let args;
if (command === "setup") {
  const archives = process.argv.slice(3).map((file) => resolve(file));
  if (
    archives.length !== 2 ||
    archives.some((file) => !file.endsWith(".tgz") || !existsSync(file))
  )
    throw new Error(
      "用法：npm run frame:setup -- <loom-frame.tgz> <android-plugin.tgz>",
    );
  if (!process.env.npm_execpath)
    throw new Error("请通过 npm run frame:setup 调用安装入口。");
  executable = process.execPath;
  args = [
    process.env.npm_execpath,
    "install",
    "--prefix",
    frameToolsDirectory,
    "--workspaces=false",
    "--no-save",
    "--package-lock=false",
    "--ignore-scripts",
    "--prefer-offline",
    "--no-audit",
    "--no-fund",
    "--cache",
    resolve(root, "target/npm-cache"),
    ...archives,
  ];
} else {
  frameToolFile("@loom-frame/android", "dist/index.mjs");
  executable = process.execPath;
  args =
    command === "typecheck"
      ? [
          resolve(root, "node_modules/typescript/bin/tsc"),
          "-p",
          resolve(root, "frame/tsconfig.json"),
          "--noEmit",
        ]
      : [frameToolFile("loom-frame", "dist/cli.mjs"), ...process.argv.slice(2)];
}
const scratch = resolve(root, "target/tmp/frame", command);
if (existsSync(scratch) && readdirSync(scratch).length) {
  throw new Error(`Frame 临时目录尚有数据，请先检查：${scratch}`);
}
mkdirSync(scratch, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = scratch;
const androidSettings = resolve(root, "android/local.properties");
if (existsSync(androidSettings)) {
  const settings = Object.fromEntries(
    readFileSync(androidSettings, "utf8")
      .split(/\r?\n/)
      .filter((line) => line.includes("=") && !line.startsWith("#"))
      .map((line) => {
        const at = line.indexOf("=");
        return [line.slice(0, at).trim(), line.slice(at + 1).trim()];
      }),
  );
  if (settings["java.home"]) process.env.JAVA_HOME = settings["java.home"];
  if (settings["sdk.dir"])
    process.env.ANDROID_SDK_ROOT = process.env.ANDROID_HOME =
      settings["sdk.dir"];
  process.env.ANDROID_USER_HOME = resolve(
    root,
    "target/frame-android/preferences",
  );
  process.env.ANDROID_EMULATOR_HOME = process.env.ANDROID_USER_HOME;
  const adbPort = Number(
    settings["adb.port"] ?? process.env.SLEEPY_FRAME_ADB_PORT ?? 5038,
  );
  if (!Number.isInteger(adbPort) || adbPort < 1024 || adbPort > 65535)
    throw new Error("Invalid Frame ADB port");
  process.env.ANDROID_ADB_SERVER_PORT = process.env.ADB_SERVER_PORT =
    String(adbPort);
}
const child = spawn(executable, args, {
  cwd: root,
  stdio: "inherit",
  windowsHide: true,
  env: { ...process.env, TEMP: scratch, TMP: scratch, TMPDIR: scratch },
});
let stopped = false;
const stop = () => {
  if (!stopped) {
    stopped = true;
    child.kill();
  }
};
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
child.on("error", (error) => {
  console.error(error);
  process.exitCode = 1;
});
child.on("close", (code) => {
  const cleanup = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "E:\\tools\\Remove-Directory.ps1",
      "-Path",
      scratch,
      "-Json",
    ],
    { encoding: "utf8", windowsHide: true },
  );
  if (cleanup.status !== 0 || existsSync(scratch)) {
    console.error(
      `Frame 临时目录清理失败：${scratch}\n${cleanup.stderr || cleanup.stdout}`,
    );
    process.exitCode = 1;
  } else process.exitCode = stopped ? 0 : (code ?? 1);
});
