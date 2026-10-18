import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
// 路由直接返回的原生 ESM 夹具不会经过 Vite 的裸包名转换。
export const uiStylesheetUrl =
  "/@fs/" +
  fileURLToPath(import.meta.resolve("@sleepy-doll/ui/styles.css")).replaceAll(
    "\\",
    "/",
  );
export const uiMotionStylesheetUrl =
  "/@fs/" +
  fileURLToPath(import.meta.resolve("@sleepy-doll/ui/motion.css")).replaceAll(
    "\\",
    "/",
  );
const scratch = resolve(
  root,
  "target/.tmp/ui-runtime",
  basename(process.argv[1], ".mjs"),
);
if (existsSync(scratch) && readdirSync(scratch).length) {
  throw new Error(
    `Existing validation data must be inspected before reuse: ${scratch}`,
  );
}
mkdirSync(scratch, { recursive: true });
process.env.TEMP = process.env.TMP = process.env.TMPDIR = scratch;

process.on("exit", () => {
  const result = spawnSync(
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
  if (result.status !== 0 || existsSync(scratch)) {
    process.stderr.write(
      `Validation cleanup failed: ${scratch}\n${result.stderr || result.stdout}`,
    );
    process.exitCode = 1;
  }
});
