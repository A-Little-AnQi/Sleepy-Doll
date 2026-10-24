import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const allowedRoot = new Set([
  ".git",
  ".github",
  ".idea",
  ".vscode",
  ".cursor",
  ".gitignore",
  ".npmrc",
  ".prettierrc",
  ".zcodeignore",
  "AGENTS.md",
  "CLAUDE.md",
  "CONTRIBUTING.md",
  "SECURITY.md",
  "README.md",
  "LICENSE",
  "Cargo.toml",
  "Cargo.lock",
  "build.rs",
  "package.json",
  "package-lock.json",
  "release-channel.json",
  "tsconfig.json",
  "vite.config.ts",
  "frame.config.ts",
  "sleepy-doll.config.example.json",
  "make-video.bat",
  "src",
  "web",
  "packages",
  "bgi-bridge",
  "plugins",
  "android",
  "frame",
  "cloudflare",
  "assets",
  "scripts",
  "tests",
  "docs",
  "video",
  "target",
  "dist",
  "node_modules",
]);
const errors = [];
for (const name of readdirSync(root)) {
  if (!allowedRoot.has(name))
    errors.push(`Unexpected repository root item: ${name}`);
}
const listed = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  },
);
const files = [...new Set(listed.split("\0").filter(Boolean))].filter((file) =>
  existsSync(resolve(root, file)),
);
const sensitive =
  /(?:^|\/)(?:user|node_modules|dist|target|\.sleepy-doll)\/|\.(?:db(?:-shm|-wal)?|log|pem|key|pfx|p12)$/i;
const credentials = [
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
  /\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})\b/,
  /\bsk-[A-Za-z0-9_-]{24,}\b/,
];
let markdown = 0;
for (const file of files) {
  if (sensitive.test(file))
    errors.push(`Runtime data or credential file in source list: ${file}`);
  const bytes = readFileSync(resolve(root, file));
  if (bytes.includes(0)) continue;
  const source = bytes.toString("utf8");
  for (const pattern of credentials) {
    const match = pattern.exec(source);
    if (match)
      errors.push(
        `Credential candidate: ${file}:${source.slice(0, match.index).split("\n").length}`,
      );
  }
  if (!file.endsWith(".md")) continue;
  markdown++;
  const prose = source.replace(/^(```|~~~)[\s\S]*?^\1[^\n]*$/gm, "");
  for (const match of prose.matchAll(
    /\[[^\]\n]*\]\(<?([^\s)>]+)>?(?:\s+"[^"]*")?\)/g,
  )) {
    const link = match[1];
    if (/^(?:[a-z][a-z\d+.-]*:|#|\/)/i.test(link)) continue;
    const path = decodeURIComponent(link.split(/[?#]/)[0]);
    if (!path) continue;
    const target = resolve(root, dirname(file), path);
    if (relative(root, target).startsWith("..") || !existsSync(target)) {
      errors.push(`Broken local link in ${file}: ${link}`);
    }
  }
}
if (errors.length) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else
  console.log(
    `Repository layout, ${markdown} Markdown files and ${files.length} source files checked; no credential-pattern candidates.`,
  );
