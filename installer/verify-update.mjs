import { chromium } from "playwright";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile, copyFile, open } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import assert from "node:assert/strict";

if (process.platform !== "win32" || process.env.GITHUB_ACTIONS !== "true")
  throw Error("此验收只在 GitHub Actions Windows 构建机执行");
const root = resolve("target/update-validation");
const installed = join(root, "installed");
const user = join(installed, "user");
const executable = join(installed, "sleepy-doll.exe");
const key =
  "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Sleepy Doll";
const debugPort = 9222;
let browser;
let child;
let registered = false;
const report = { baseline: "0.0.0", target: "0.0.1", publishedBaseline: false };
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

function powershell(code) {
  const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", code], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0)
    throw Error(result.stderr || result.stdout || "Windows 验收命令失败");
  return result.stdout.trim();
}
function quote(value) {
  return "'" + value.replaceAll("'", "''") + "'";
}
async function attach(timeout = 60000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const connection = await chromium.connectOverCDP(
        `http://127.0.0.1:${debugPort}`,
        { timeout: 3000 },
      );
      const page = connection
        .contexts()
        .flatMap((context) => context.pages())
        .find((page) => /sleepy/.test(page.url()));
      if (page) {
        await page.waitForFunction(
          () => typeof window.ipc?.postMessage === "function",
        );
        return { connection, page };
      }
    } catch {}
    await delay(1000);
  }
  throw Error("桌面 WebView2 未在时限内就绪");
}
async function rpc(page, method, params = {}) {
  return page.evaluate(
    ({ method, params }) =>
      new Promise((resolve, reject) => {
        const id = crypto.randomUUID();
        const receive = window.__sleepyDollReceive;
        const timer = setTimeout(() => {
          window.__sleepyDollReceive = receive;
          reject(Error("原生 IPC 超时"));
        }, 20000);
        window.__sleepyDollReceive = (message) => {
          receive?.(message);
          if (message.kind === "response" && message.id === id) {
            clearTimeout(timer);
            window.__sleepyDollReceive = receive;
            message.ok
              ? resolve(message.result)
              : reject(Error(message.error?.message));
          }
        };
        window.ipc.postMessage(JSON.stringify({ id, method, params }));
      }),
    { method, params },
  );
}
async function stopOwnedApplication() {
  const path = quote(executable);
  powershell(
    `Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'sleepy-doll.exe' -and $_.ExecutablePath -eq ${path} } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }`,
  );
}

try {
  await mkdir(user, { recursive: true });
  for (const name of ["catalog", "skills", "plugins", ".sleepy-doll/updates"]) {
    await mkdir(join(user, name), { recursive: true });
  }
  await mkdir(join(installed, "bridge"), { recursive: true });
  await copyFile("target/release/sleepy-doll.exe", executable);
  const config = JSON.parse(
    await readFile("sleepy-doll.config.example.json", "utf8"),
  );
  config.bridge.enabled = false;
  config.bridge.token = "";
  config.plugins.directories = [];
  config.agent.skillDirectories = [];
  config.tray.enabled = false;
  await writeFile(join(user, "config.json"), JSON.stringify(config, null, 2));
  await writeFile(
    join(user, "preservation-marker.json"),
    JSON.stringify({ marker: "update-validation", content: "keep" }),
  );
  const bridgePath = join(installed, "bridge/bridge.config.json");
  await writeFile(
    bridgePath,
    JSON.stringify({ token: "validation-token", userDirectory: user }, null, 2),
  );
  await writeFile(
    join(user, ".sleepy-doll/updates/preferences.json"),
    JSON.stringify({
      clientId: randomUUID(),
      analyticsEnabled: true,
      channel: "test",
      pendingVersion: null,
    }),
  );
  powershell(
    `if(Test-Path -LiteralPath ${quote(key)}){throw '构建机已有安装登记，拒绝覆盖'}; New-Item -Path ${quote(key)} -Force | Out-Null; New-ItemProperty -LiteralPath ${quote(key)} -Name InstallLocation -Value ${quote(installed)} -PropertyType String | Out-Null; New-ItemProperty -LiteralPath ${quote(key)} -Name DisplayVersion -Value '0.0.0' -PropertyType String | Out-Null`,
  );
  registered = true;
  const stdout = await open(join(root, "application.stdout"), "w");
  const stderr = await open(join(root, "application.stderr"), "w");
  child = spawn(executable, [join(user, "config.json")], {
    windowsHide: true,
    stdio: ["ignore", stdout.fd, stderr.fd],
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
    },
  });
  report.baselinePid = child.pid;
  const initial = await attach();
  browser = initial.connection;
  const page = initial.page;
  const before = await rpc(page, "release.state");
  assert.equal(before.currentVersion, "0.0.0");
  await rpc(page, "release.check");
  await page
    .getByRole("button", { name: "下载更新", exact: true })
    .waitFor({ state: "visible", timeout: 30000 });
  await page.screenshot({ path: join(root, "before-update.png") });
  const configBefore = digest(await readFile(join(user, "config.json")));
  const markerBefore = digest(
    await readFile(join(user, "preservation-marker.json")),
  );
  const bridgeToken = JSON.parse(await readFile(bridgePath, "utf8")).token;
  const database = join(user, ".sleepy-doll/sleepy-doll.db");
  const python = spawnSync(
    "python",
    [
      "-B",
      "-c",
      "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('CREATE TABLE IF NOT EXISTS update_validation(marker TEXT)'); c.execute(\"INSERT INTO update_validation VALUES('keep')\"); c.commit(); c.close()",
      database,
    ],
    { encoding: "utf8", windowsHide: true },
  );
  assert.equal(python.status, 0, python.stderr);
  await page.getByRole("button", { name: "下载更新", exact: true }).click();
  await page
    .getByRole("button", { name: "安装并重启", exact: true })
    .waitFor({ state: "visible", timeout: 200000 });
  report.downloaded = (await rpc(page, "release.state")).downloaded;
  assert(report.downloaded);
  await page.getByRole("button", { name: "安装并重启", exact: true }).click();
  await new Promise((resolve, reject) => {
    if (child.exitCode !== null) {
      resolve();
      return;
    }
    const timer = setTimeout(() => reject(Error("旧程序未退出")), 70000);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
  await delay(3000);
  const updated = await attach(90000);
  browser = updated.connection;
  const after = await rpc(updated.page, "release.state");
  assert.equal(after.currentVersion, "0.0.1");
  await updated.page.screenshot({ path: join(root, "after-update.png") });
  assert.equal(digest(await readFile(join(user, "config.json"))), configBefore);
  assert.equal(
    digest(await readFile(join(user, "preservation-marker.json"))),
    markerBefore,
  );
  assert.equal(
    JSON.parse(await readFile(bridgePath, "utf8")).token,
    bridgeToken,
  );
  const dataCheck = spawnSync(
    "python",
    [
      "-B",
      "-c",
      "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); assert c.execute('SELECT marker FROM update_validation').fetchone()[0]=='keep'; c.close()",
      database,
    ],
    { encoding: "utf8", windowsHide: true },
  );
  assert.equal(dataCheck.status, 0, dataCheck.stderr);
  const deadline = Date.now() + 15000;
  let preferences;
  while (Date.now() < deadline) {
    preferences = JSON.parse(
      await readFile(
        join(user, ".sleepy-doll/updates/preferences.json"),
        "utf8",
      ),
    );
    if (preferences.pendingVersion === null) break;
    await delay(500);
  }
  assert.equal(preferences.pendingVersion, null);
  report.configPreserved = true;
  report.bridgeTokenPreserved = true;
  report.databasePreserved = true;
  report.actualVersion = after.currentVersion;
  report.success = true;
  console.log(JSON.stringify(report));
} catch (error) {
  report.success = false;
  report.error = String(error);
  throw error;
} finally {
  await writeFile(join(root, "result.json"), JSON.stringify(report, null, 2));
  await stopOwnedApplication();
  if (registered) {
    powershell(
      `[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree('Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Sleepy Doll',$false)`,
    );
  }
}
