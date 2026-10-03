import { createHash, createHmac } from "node:crypto";
import { readFile, writeFile, stat } from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";

const hash = (value) => createHash("sha256").update(value).digest("hex");
const hmac = (key, value) => createHmac("sha256", key).update(value).digest();
const version = JSON.parse(await readFile("package.json", "utf8")).version;
const cargo = /^version\s*=\s*"([^"]+)"/m.exec(
  await readFile("Cargo.toml", "utf8"),
)?.[1];
if (!/^\d+\.\d+\.\d+$/.test(version) || cargo !== version)
  throw Error("前端和 Cargo 版本号必须一致");
const channel = version.startsWith("0.0.") ? "test" : "stable";
const name = `Sleepy-Doll-${version}-setup.exe`;
const file = `dist/${name}`;
const key = `releases/${version}/${name}`;
const account = process.env.CLOUDFLARE_ACCOUNT_ID;
const bucket = process.env.R2_BUCKET;
const access = process.env.R2_ACCESS_KEY_ID;
const secret = process.env.R2_SECRET_ACCESS_KEY;
if (![account, bucket, access, secret].every(Boolean))
  throw Error("缺少 R2 发布凭据或桶配置");
const host = `${account}.r2.cloudflarestorage.com`;

async function s3(method, objectKey, body = Buffer.alloc(0), extra = {}) {
  const path =
    "/" + [bucket, ...objectKey.split("/")].map(encodeURIComponent).join("/");
  const timestamp = new Date().toISOString().replace(/[:-]|\.\d{3}/g, "");
  const date = timestamp.slice(0, 8);
  const headers = {
    host,
    "x-amz-date": timestamp,
    "x-amz-content-sha256": hash(body),
    ...extra,
  };
  const names = Object.keys(headers).sort();
  const signed = names.join(";");
  const canonical = [
    method,
    path,
    "",
    names.map((name) => `${name}:${String(headers[name]).trim()}\n`).join(""),
    signed,
    hash(body),
  ].join("\n");
  const scope = `${date}/auto/s3/aws4_request`;
  const signingKey = hmac(
    hmac(hmac(hmac("AWS4" + secret, date), "auto"), "s3"),
    "aws4_request",
  );
  const signature = createHmac("sha256", signingKey)
    .update(`AWS4-HMAC-SHA256\n${timestamp}\n${scope}\n${hash(canonical)}`)
    .digest("hex");
  headers.Authorization = `AWS4-HMAC-SHA256 Credential=${access}/${scope}, SignedHeaders=${signed}, Signature=${signature}`;
  return fetch(`https://${host}${path}`, {
    method,
    headers,
    ...(method === "PUT" ? { body } : {}),
    signal: AbortSignal.timeout(120000),
  });
}

async function requireSuccess(response, operation) {
  if (!response.ok) throw Error(`${operation}失败：HTTP ${response.status}`);
  return response;
}

const bytes = await readFile(file);
const digest = hash(bytes);
const size = (await stat(file)).size;
const notes = await readFile(
  `web/src/app/release-notes/changelog-${version}.md`,
  "utf8",
);
const manifest = {
  version,
  channel,
  url: `https://download.sleepy-doll.restless-nh3.com/${key}`,
  size,
  sha256: digest,
  notes,
  publishedAt: new Date().toISOString(),
};
await writeFile(`${file}.sha256`, `${digest}  ${name}\n`);
await writeFile(
  `dist/Sleepy-Doll-${version}-release.json`,
  JSON.stringify(manifest, null, 2) + "\n",
);

if (process.argv.includes("--promote")) {
  const head = await requireSuccess(await s3("HEAD", key), "读取安装包");
  if (head.headers.get("x-amz-meta-sha256") !== digest)
    throw Error("R2 安装包摘要与本次构建不一致");
  const current = await s3("GET", `channels/${channel}.json`);
  if (current.ok) {
    const previous = await current.json();
    const compare = (a, b) => {
      const x = a.split(".").map(Number),
        y = b.split(".").map(Number);
      for (let i = 0; i < 3; i++) {
        if (x[i] !== y[i]) return x[i] - y[i];
      }
      return 0;
    };
    if (compare(previous.version, version) > 0)
      throw Error("拒绝用旧版本覆盖更新通道");
  } else if (current.status !== 404)
    throw Error(`读取通道失败：HTTP ${current.status}`);
  await requireSuccess(
    await s3(
      "PUT",
      `channels/${channel}.json`,
      Buffer.from(JSON.stringify(manifest)),
      { "content-type": "application/json", "cache-control": "no-store" },
    ),
    "切换更新通道",
  );
  console.log(`更新通道已切换：${channel} ${version}`);
} else {
  const existing = await s3("HEAD", key);
  if (existing.ok) {
    if (existing.headers.get("x-amz-meta-sha256") !== digest)
      throw Error("此版本已发布不同内容，请提升版本号");
  } else if (existing.status === 404) {
    await requireSuccess(
      await s3("PUT", key, bytes, {
        "content-type": "application/octet-stream",
        "content-disposition": `attachment; filename="${name}"`,
        "cache-control": "public, max-age=31536000, immutable",
        "x-amz-meta-sha256": digest,
      }),
      "上传安装包",
    );
  } else throw Error(`读取安装包失败：HTTP ${existing.status}`);
  await requireSuccess(
    await s3("PUT", `${key}.sha256`, Buffer.from(`${digest}  ${name}\n`), {
      "content-type": "text/plain",
      "cache-control": "public, max-age=31536000, immutable",
    }),
    "上传校验文件",
  );
  let verified = false;
  for (let attempt = 0; attempt < 6; attempt++) {
    try {
      const response = await fetch(manifest.url, {
        signal: AbortSignal.timeout(60000),
      });
      if (
        response.ok &&
        hash(Buffer.from(await response.arrayBuffer())) === digest
      ) {
        verified = true;
        break;
      }
    } catch {}
    if (attempt < 5) await delay(8000);
  }
  if (!verified) throw Error("公开下载地址未通过 SHA-256 验证，通道保持原版本");
  await requireSuccess(
    await s3(
      "PUT",
      `releases/${version}/release.json`,
      Buffer.from(JSON.stringify(manifest)),
      {
        "content-type": "application/json",
        "cache-control": "public, max-age=31536000, immutable",
      },
    ),
    "上传版本清单",
  );
  console.log(
    `安装包上传并校验完成：${version}，${size} 字节，SHA-256 ${digest}`,
  );
}
