import { spawnSync } from "node:child_process";
const packageVersion = "4.147.0";
const command = process.platform === "win32" ? "npm.cmd" : "npm";
function run(args, input) {
  const result = spawnSync(
    command,
    [
      "exec",
      "--yes",
      `--package=wrangler@${packageVersion}`,
      "--",
      "wrangler",
      ...args,
      "--config",
      "cloudflare/wrangler.jsonc",
    ],
    {
      stdio: ["pipe", "inherit", "inherit"],
      input,
      shell: process.platform === "win32",
    },
  );
  if (result.status !== 0) throw Error(`Cloudflare 部署失败：${result.status}`);
}
if (!process.env.CLOUDFLARE_API_TOKEN)
  throw Error("缺少 Cloudflare 部署 token");
if (!process.env.GA_MEASUREMENT_ID || !process.env.GA_API_SECRET)
  throw Error("缺少 GA4 配置");
run(["deploy"]);
run(
  ["secret", "bulk"],
  JSON.stringify({
    GA_MEASUREMENT_ID: process.env.GA_MEASUREMENT_ID,
    GA_API_SECRET: process.env.GA_API_SECRET,
  }),
);
