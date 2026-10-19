import assert from "node:assert/strict";
import { createHash, createHmac } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { parseListing, checkStorageBudget } from "./r2-budget.mjs";
import { releaseConfig } from "./release-version.mjs";

export const sha256 = (bytes) =>
  createHash("sha256").update(bytes).digest("hex");

export async function replaceRelease(
  snapshot,
  replacement,
  expected,
  effects,
  note = "",
) {
  const previous = snapshot.manifest;
  assert.match(expected, /^[a-f0-9]{64}$/);
  assert.equal(previous.channel, "stable");
  assert.equal(
    previous.sha256,
    expected,
    "Stable channel changed before replacement",
  );
  assert.equal(
    sha256(snapshot.package),
    expected,
    "Original package hash mismatch",
  );
  assert.equal(
    snapshot.package.length,
    previous.size,
    "Original package size mismatch",
  );
  assert.equal(
    JSON.parse(snapshot.archive).sha256,
    expected,
    "Archived manifest mismatch",
  );
  assert.equal(
    snapshot.checksum.toString().split(" ")[0],
    expected,
    "Original checksum mismatch",
  );
  assert.equal(replacement.subarray(0, 2).toString(), "MZ");
  assert.ok(replacement.length <= 256 * 1024 * 1024);
  const digest = sha256(replacement);
  assert.notEqual(digest, expected, "Replacement package is unchanged");
  const manifest = {
    ...previous,
    size: replacement.length,
    sha256: digest,
    publishedAt: new Date().toISOString(),
  };
  if (note) manifest.notes = `${previous.notes ?? ""}\n\n${note}`;
  const next = {
    manifest,
    package: replacement,
    checksum: Buffer.from(
      `${digest}  Sleepy-Doll-${previous.version}-setup.exe\n`,
    ),
    archive: Buffer.from(JSON.stringify(manifest)),
  };
  await effects.prepare(next);
  let mutated = false;
  let tagAttempted = false;
  try {
    await effects.gateway(digest, true);
    await effects.assertCurrent();
    mutated = true;
    await effects.writeFiles(next);
    await effects.publishGithub(next);
    await effects.writeChannel(next.manifest);
    await effects.gateway(digest, false);
    await effects.verifyPublic(next);
    tagAttempted = true;
    await effects.retag();
    return manifest;
  } catch (error) {
    const failures = [];
    const attempt = async (action) => {
      try {
        await action();
      } catch (failure) {
        failures.push(failure);
      }
    };
    if (mutated) {
      await attempt(() => effects.gateway(digest, true));
      await attempt(() => effects.writeFiles(snapshot));
      await attempt(() => effects.publishGithub(snapshot));
      await attempt(() => effects.writeChannel(previous));
      if (tagAttempted) await attempt(() => effects.restoreTag());
    }
    if (!failures.length) await attempt(() => effects.gateway(expected, false));
    if (failures.length)
      throw new AggregateError(
        [error, ...failures],
        "Replacement failed; rollback is incomplete and downloads must be checked",
      );
    throw error;
  }
}

function run(command, args, capture = false) {
  const result = spawnSync(command, args, {
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
    encoding: "utf8",
    windowsHide: true,
    shell: process.platform === "win32" && command.endsWith(".cmd"),
  });
  if (result.status !== 0)
    throw new Error(
      `${command} failed: ${result.status}${capture ? ` ${result.stderr}` : ""}`,
    );
  return result.stdout?.trim();
}

async function main() {
  const { version, channel } = await releaseConfig();
  assert.equal(channel, "stable");
  const expected = process.env.REPLACE_EXPECTED_SHA256;
  assert.match(expected ?? "", /^[a-f0-9]{64}$/);
  for (const name of [
    "CLOUDFLARE_ACCOUNT_ID",
    "R2_BUCKET",
    "R2_ACCESS_KEY_ID",
    "R2_SECRET_ACCESS_KEY",
    "CLOUDFLARE_API_TOKEN",
    "GH_TOKEN",
    "GITHUB_REPOSITORY",
    "GITHUB_SHA",
  ])
    assert.ok(process.env[name], `Missing ${name}`);
  const build = run("git", ["rev-parse", "HEAD"], true);
  assert.equal(
    build,
    process.env.REPLACE_SOURCE_COMMIT,
    "Checkout differs from the requested hotfix source",
  );
  const host = `${process.env.CLOUDFLARE_ACCOUNT_ID}.r2.cloudflarestorage.com`;
  const hmac = (key, value) => createHmac("sha256", key).update(value).digest();
  const encode = (value) =>
    encodeURIComponent(value).replace(
      /[!'()*]/g,
      (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
    );
  async function s3(
    method,
    key,
    body = Buffer.alloc(0),
    extra = {},
    query = {},
  ) {
    const path =
      "/" +
      [process.env.R2_BUCKET, ...key.split("/")]
        .map(encodeURIComponent)
        .join("/");
    const timestamp = new Date().toISOString().replace(/[:-]|\.\d{3}/g, "");
    const date = timestamp.slice(0, 8),
      scope = `${date}/auto/s3/aws4_request`;
    const headers = {
      host,
      "x-amz-date": timestamp,
      "x-amz-content-sha256": sha256(body),
      ...extra,
    };
    const names = Object.keys(headers).sort(),
      signed = names.join(";");
    const qs = Object.entries(query)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([k, v]) => `${encode(k)}=${encode(v)}`)
      .join("&");
    const canonical = [
      method,
      path,
      qs,
      names.map((n) => `${n}:${String(headers[n]).trim()}\n`).join(""),
      signed,
      sha256(body),
    ].join("\n");
    const signing = hmac(
      hmac(
        hmac(hmac("AWS4" + process.env.R2_SECRET_ACCESS_KEY, date), "auto"),
        "s3",
      ),
      "aws4_request",
    );
    const signature = createHmac("sha256", signing)
      .update(`AWS4-HMAC-SHA256\n${timestamp}\n${scope}\n${sha256(canonical)}`)
      .digest("hex");
    headers.Authorization = `AWS4-HMAC-SHA256 Credential=${process.env.R2_ACCESS_KEY_ID}/${scope}, SignedHeaders=${signed}, Signature=${signature}`;
    const response = await fetch(
      `https://${host}${path}${qs ? `?${qs}` : ""}`,
      {
        method,
        headers,
        ...(method === "PUT" ? { body } : {}),
        signal: AbortSignal.timeout(120000),
      },
    );
    if (!response.ok)
      throw new Error(`R2 ${method} ${key}: HTTP ${response.status}`);
    return response;
  }
  const name = `Sleepy-Doll-${version}-setup.exe`,
    key = `releases/${version}/${name}`;
  const channelKey = "channels/stable.json",
    archiveKey = `releases/${version}/release.json`;
  const manifest = await (await s3("GET", channelKey)).json();
  assert.equal(
    manifest.version,
    version,
    "Only the active stable version may be replaced",
  );
  const snapshot = {
    manifest,
    package: Buffer.from(await (await s3("GET", key)).arrayBuffer()),
    archive: Buffer.from(await (await s3("GET", archiveKey)).arrayBuffer()),
    checksum: Buffer.from(
      await (await s3("GET", `${key}.sha256`)).arrayBuffer(),
    ),
  };
  const replacement = await readFile(`dist/${name}`);
  const originalRelease = JSON.parse(
    run(
      "gh",
      [
        "api",
        `repos/${process.env.GITHUB_REPOSITORY}/releases/tags/v${version}`,
      ],
      true,
    ),
  );
  assert.equal(originalRelease.draft, false);
  assert.equal(originalRelease.prerelease, false);
  assert.notEqual(
    originalRelease.immutable,
    true,
    "GitHub release is immutable",
  );
  const originalAsset = originalRelease.assets.find(
    (asset) => asset.name === name,
  );
  assert.ok(originalAsset);
  assert.equal(originalAsset.size, manifest.size);
  if (originalAsset.digest)
    assert.equal(originalAsset.digest, `sha256:${expected}`);
  const ref = `refs/tags/v${version}`;
  const remoteTag = () =>
    run("git", ["ls-remote", "origin", ref], true)?.split(/\s/)[0];
  const previousTag = remoteTag();
  assert.match(previousTag ?? "", /^[a-f0-9]{40}$/);
  run("git", ["fetch", "--no-tags", "origin", ref]);
  const workspace = resolve("target/.tmp/release-replacement");
  await mkdir(workspace, { recursive: true });
  const metadata = (type, digest) => ({
    "content-type": type,
    "cache-control": "no-store",
    "x-amz-storage-class": "STANDARD",
    ...(digest
      ? {
          "x-amz-meta-sha256": digest,
          "content-disposition": `attachment; filename="${name}"`,
        }
      : {}),
  });
  async function writeFiles(state) {
    await s3(
      "PUT",
      key,
      state.package,
      metadata("application/octet-stream", state.manifest.sha256),
    );
    assert.equal(
      sha256(Buffer.from(await (await s3("GET", key)).arrayBuffer())),
      state.manifest.sha256,
    );
    await s3("PUT", `${key}.sha256`, state.checksum, metadata("text/plain"));
    await s3("PUT", archiveKey, state.archive, metadata("application/json"));
  }
  const effects = {
    async prepare(next) {
      const objects = [];
      const seen = new Set();
      let token;
      do {
        const page = parseListing(
          await (
            await s3(
              "GET",
              "",
              Buffer.alloc(0),
              {},
              {
                "list-type": "2",
                "max-keys": "1000",
                ...(token ? { "continuation-token": token } : {}),
              },
            )
          ).text(),
        );
        objects.push(...page.objects);
        token = page.next;
        if (objects.length > 10000 || (token && seen.has(token)))
          throw new Error("Unsafe R2 inventory");
        if (token) seen.add(token);
      } while (token);
      checkStorageBudget(objects, [
        { key, size: next.package.length },
        { key: `${key}.sha256`, size: next.checksum.length },
        { key: archiveKey, size: next.archive.length },
        {
          key: channelKey,
          size: Buffer.byteLength(JSON.stringify(next.manifest)),
        },
      ]);
    },
    async assertCurrent() {
      assert.equal(
        (await (await s3("GET", channelKey)).json()).sha256,
        expected,
        "Stable channel changed while preparing replacement",
      );
      assert.equal(
        remoteTag(),
        previousTag,
        "Release tag changed while preparing replacement",
      );
    },
    gateway(digest, paused) {
      run(process.platform === "win32" ? "npm.cmd" : "npm", [
        "exec",
        "--yes",
        "--package=wrangler@4.147.0",
        "--",
        "wrangler",
        "deploy",
        "--config",
        "cloudflare/wrangler.jsonc",
        "--keep-vars",
        "--var",
        `CLIENT_DOWNLOAD_REVISION:${digest}`,
        "--var",
        `DOWNLOAD_REPLACE_VERSION:${paused ? version : "none"}`,
      ]);
    },
    writeFiles,
    async publishGithub(state) {
      const prefix = state === snapshot ? workspace : resolve("dist");
      await writeFile(`${prefix}/${name}`, state.package);
      await writeFile(`${prefix}/${name}.sha256`, state.checksum);
      await writeFile(
        `${prefix}/Sleepy-Doll-${version}-release.json`,
        state.archive,
      );
      run("gh", [
        "release",
        "upload",
        `v${version}`,
        `${prefix}/${name}`,
        `${prefix}/${name}.sha256`,
        `${prefix}/Sleepy-Doll-${version}-release.json`,
        "--clobber",
      ]);
      const notes = `${workspace}/release-notes.md`;
      await writeFile(
        notes,
        state === snapshot
          ? originalRelease.body
          : `${originalRelease.body ?? ""}\n\n${process.env.REPLACE_NOTE ?? ""}`,
      );
      run("gh", ["release", "edit", `v${version}`, "--notes-file", notes]);
    },
    async writeChannel(value) {
      await s3(
        "PUT",
        channelKey,
        Buffer.from(JSON.stringify(value)),
        metadata("application/json"),
      );
    },
    async verifyPublic(next) {
      const base = new URL(next.manifest.url);
      assert.equal(
        base.origin,
        "https://sleepy-doll-download.restless-nh3.com",
      );
      const url = `${base.origin}/releases/${version}/${next.manifest.sha256}/${name}`;
      for (let attempt = 0; attempt < 12; attempt++) {
        try {
          const response = await fetch(url, {
            signal: AbortSignal.timeout(60000),
          });
          if (response.ok) {
            assert.equal(
              sha256(Buffer.from(await response.arrayBuffer())),
              next.manifest.sha256,
              "Public replacement hash mismatch",
            );
            const channelResponse = await fetch(
              "https://sleepy-doll-api.restless-nh3.com/api/releases/stable",
              { signal: AbortSignal.timeout(30000) },
            );
            if (channelResponse.ok) {
              const publicManifest = await channelResponse.json();
              if (
                publicManifest.version === version &&
                publicManifest.sha256 === next.manifest.sha256 &&
                publicManifest.size === next.package.length
              )
                return;
            }
          }
          if (![404, 502, 503].includes(response.status))
            throw new Error(`Public download HTTP ${response.status}`);
        } catch (error) {
          if (
            error.code === "ERR_ASSERTION" ||
            /Public download HTTP/.test(error.message)
          )
            throw error;
          if (attempt === 11) throw error;
        }
        await delay(8000);
      }
      throw new Error("Public replacement verification timed out");
    },
    retag() {
      run("git", [
        "push",
        `--force-with-lease=${ref}:${previousTag}`,
        "origin",
        `HEAD:${ref}`,
      ]);
    },
    restoreTag() {
      const current = remoteTag();
      if (current === previousTag) return;
      assert.equal(
        current,
        build,
        "Another actor changed the release tag; preserve it",
      );
      run("git", [
        "push",
        `--force-with-lease=${ref}:${build}`,
        "origin",
        `${previousTag}:${ref}`,
      ]);
    },
  };
  const result = await replaceRelease(
    snapshot,
    replacement,
    expected,
    effects,
    process.env.REPLACE_NOTE ?? "",
  );
  console.log(
    JSON.stringify({
      version,
      channel,
      sourceCommit: build,
      size: result.size,
      previousSha256: expected,
      sha256: result.sha256,
      publicVerified: true,
    }),
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
)
  await main();
