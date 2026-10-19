import test from "node:test";
import assert from "node:assert/strict";
import { replaceRelease, sha256 } from "./replace-release.mjs";

function fixture(failAt) {
  const original = Buffer.from("MZ-original");
  const manifest = {
    version: "0.1.1",
    channel: "stable",
    size: original.length,
    sha256: sha256(original),
    notes: "original notes",
  };
  const snapshot = {
    manifest,
    package: original,
    archive: Buffer.from(JSON.stringify(manifest)),
    checksum: Buffer.from(`${manifest.sha256}  Sleepy-Doll-0.1.1-setup.exe\n`),
  };
  const replacement = Buffer.from("MZ-fixed-package");
  const calls = [];
  let failed = false;
  const state = {
    package: original,
    github: original,
    channel: manifest,
    paused: false,
    tag: "original",
  };
  const effect = async (name, action) => {
    calls.push(name);
    if (name === failAt && !failed) {
      failed = true;
      throw new Error(`Injected ${name}`);
    }
    action?.();
  };
  const effects = {
    prepare: () => effect("prepare"),
    assertCurrent: () => effect("assertCurrent"),
    gateway: (hash, paused) =>
      effect(paused ? "pause" : "resume", () => {
        state.paused = paused;
      }),
    writeFiles: (next) =>
      effect("files", () => {
        state.package = next.package;
      }),
    publishGithub: (next) =>
      effect("github", () => {
        state.github = next.package;
      }),
    writeChannel: (next) =>
      effect("channel", () => {
        state.channel = next;
      }),
    verifyPublic: () => effect("public"),
    retag: () =>
      effect("tag", () => {
        state.tag = "fixed";
      }),
    restoreTag: () =>
      effect("restoreTag", () => {
        state.tag = "original";
      }),
  };
  return { snapshot, replacement, state, effects, calls };
}

test("replacement verifies public bytes before moving the release tag", async () => {
  const f = fixture();
  const next = await replaceRelease(
    f.snapshot,
    f.replacement,
    f.snapshot.manifest.sha256,
    f.effects,
    "migration fix",
  );
  assert.equal(next.version, "0.1.1");
  assert.equal(next.sha256, sha256(f.replacement));
  assert.match(next.notes, /migration fix/);
  assert.equal(f.state.paused, false);
  assert.equal(f.state.tag, "fixed");
  assert.ok(f.calls.indexOf("public") < f.calls.indexOf("tag"));
});

test("unexpected original hash fails before external side effects", async () => {
  const f = fixture();
  await assert.rejects(
    replaceRelease(f.snapshot, f.replacement, "0".repeat(64), f.effects),
  );
  assert.deepEqual(f.calls, []);
});

for (const failure of [
  "prepare",
  "pause",
  "assertCurrent",
  "files",
  "github",
  "channel",
  "resume",
  "public",
  "tag",
]) {
  test(`failure at ${failure} restores original delivery and resumes the original gateway`, async () => {
    const f = fixture(failure);
    await assert.rejects(
      replaceRelease(
        f.snapshot,
        f.replacement,
        f.snapshot.manifest.sha256,
        f.effects,
      ),
      /Injected/,
    );
    assert.ok(f.state.package.equals(f.snapshot.package));
    assert.ok(f.state.github.equals(f.snapshot.package));
    assert.equal(f.state.channel.sha256, f.snapshot.manifest.sha256);
    assert.equal(f.state.tag, "original");
    assert.equal(f.state.paused, false);
  });
}

test("partially written package is restored after the upload reports failure", async () => {
  const f = fixture();
  let failed = false;
  f.effects.writeFiles = async (state) => {
    f.state.package = state.package;
    if (!failed) {
      failed = true;
      throw new Error("Partial write");
    }
  };
  await assert.rejects(
    replaceRelease(
      f.snapshot,
      f.replacement,
      f.snapshot.manifest.sha256,
      f.effects,
    ),
  );
  assert.ok(f.state.package.equals(f.snapshot.package));
  assert.equal(f.state.paused, false);
});

test("rollback failure does not reopen a partially restored release", async () => {
  const f = fixture("public");
  let writes = 0;
  f.effects.writeFiles = async () => {
    if (++writes > 1) throw new Error("Restore unavailable");
  };
  await assert.rejects(
    replaceRelease(
      f.snapshot,
      f.replacement,
      f.snapshot.manifest.sha256,
      f.effects,
    ),
    AggregateError,
  );
  assert.equal(f.state.paused, true);
});
