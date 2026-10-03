import test from "node:test";
import assert from "node:assert/strict";
import worker, { validateEvent } from "./worker.mjs";
const event = {
  clientId: "12345678-1234-1234-1234-123456789abc",
  sessionId: 1,
  event: "app_start",
  version: "0.0.1",
  channel: "test",
  platform: "windows",
};
test("事件白名单只转发基础字段", () => {
  const output = validateEvent({
    ...event,
    prompt: "private",
    apiKey: "secret",
  });
  assert.equal(output.events[0].params.app_version, "0.0.1");
  assert(!JSON.stringify(output).includes("private"));
  assert.equal(validateEvent({ ...event, event: "conversation" }), null);
});
test("统计拒绝其他来源与无效事件", async () => {
  const response = await worker.fetch(
    new Request("https://sleepy-doll.restless-nh3.com/api/events", {
      method: "POST",
      headers: {
        Origin: "https://other.example",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(event),
    }),
    {},
  );
  assert.equal(response.status, 403);
});
test("没有版本时返回 404", async () => {
  const response = await worker.fetch(
    new Request("https://sleepy-doll.restless-nh3.com/api/releases/stable"),
    { RELEASES: { get: async () => null } },
  );
  assert.equal(response.status, 404);
});

test("官网仅记录允许的文档路由，不转发查询参数", () => {
  const web = { ...event, platform: "web", event: "page_view" };
  assert.equal(validateEvent({ ...web, pagePath: "/docs/guide" }).events[0].params.page_location, "https://sleepy-doll.restless-nh3.com/#/docs/guide");
  assert.equal(validateEvent({ ...web, pagePath: "/docs/guide?secret=private" }).events[0].params.page_location, "https://sleepy-doll.restless-nh3.com/");
});

const downloadUrl = "https://sleepy-doll-download.restless-nh3.com/releases/0.0.1/Sleepy-Doll-0.0.1-setup.exe";
function downloadEnv({ found = true, allowed = true, fail = false, ranged = false } = {}) {
  let reads = 0;
  const object = () => ({
    size: 9, storageClass: "Standard", httpEtag: '"fixture"',
    writeHttpMetadata(headers) { headers.set("Content-Type", "application/octet-stream"); },
    ...(ranged ? { range: { offset: 0, length: 3 } } : {}),
  });
  return {
    get reads() { return reads; },
    env: {
      DOWNLOAD_LIMIT: { limit: async () => ({ success: allowed }) },
      RELEASES: {
        async get() { reads++; if (fail) throw Error("storage"); return found ? { ...object(), body: new Response(ranged ? "ins" : "installer").body } : null; },
        async head() { reads++; return found ? object() : null; },
      },
    },
  };
}
test("下载和 HEAD 每次最多读取一次 R2，响应可供客户端校验", async () => {
  for (const method of ["GET", "HEAD"]) {
    const fixture = downloadEnv();
    const response = await worker.fetch(new Request(downloadUrl, { method }), fixture.env);
    assert.equal(response.status, 200); assert.equal(fixture.reads, 1);
    assert.equal(response.headers.get("X-Sleepy-Doll-Gateway"), "guarded-r2");
    assert.equal(await response.text(), method === "HEAD" ? "" : "installer");
  }
});
test("不合法路径、查询串、方法和多段 Range 在访问 R2 前拒绝", async () => {
  const requests = [
    new Request(downloadUrl + "?random=1"),
    new Request(downloadUrl.replace("Sleepy-Doll-0.0.1", "Sleepy-Doll-0.0.2")),
    new Request(downloadUrl + "/anything"),
    new Request(downloadUrl, { method: "POST" }),
    new Request(downloadUrl, { headers: { Range: "bytes=0-1,5-7" } }),
  ];
  for (const request of requests) {
    const fixture = downloadEnv();
    assert((await worker.fetch(request, fixture.env)).status >= 400); assert.equal(fixture.reads, 0);
  }
});
test("限流、缺失保护绑定和手动暂停均不访问 R2", async () => {
  const blocked = downloadEnv({ allowed: false });
  assert.equal((await worker.fetch(new Request(downloadUrl), blocked.env)).status, 429);
  assert.equal(blocked.reads, 0);
  const paused = downloadEnv(); paused.env.DOWNLOADS_DISABLED = "true";
  assert.equal((await worker.fetch(new Request(downloadUrl), paused.env)).status, 503); assert.equal(paused.reads, 0);
  const absent = downloadEnv(); delete absent.env.DOWNLOAD_LIMIT;
  assert.equal((await worker.fetch(new Request(downloadUrl), absent.env)).status, 503); assert.equal(absent.reads, 0);
});
test("缺失对象与 R2 故障不发生重试，Range 正常返回部分内容", async () => {
  for (const options of [{ found: false }, { fail: true }]) {
    const fixture = downloadEnv(options);
    const response = await worker.fetch(new Request(downloadUrl), fixture.env);
    assert.equal(response.status, options.fail ? 502 : 404); assert.equal(fixture.reads, 1);
  }
  const fixture = downloadEnv({ ranged: true });
  const response = await worker.fetch(new Request(downloadUrl, { headers: { Range: "bytes=0-2" } }), fixture.env);
  assert.equal(response.status, 206); assert.equal(response.headers.get("Content-Range"), "bytes 0-2/9");
  assert.equal(await response.text(), "ins"); assert.equal(fixture.reads, 1);
});

test("旧 CDN 缓存命中仍经过保护入口，且不会读取 R2", async () => {
  const original = globalThis.caches;
  try {
    globalThis.caches = { default: { match: async () => new Response("installer", { headers: { "Content-Length": "9" } }) } };
    const fixture = downloadEnv();
    const response = await worker.fetch(new Request(downloadUrl), fixture.env);
    assert.equal(response.headers.get("X-Sleepy-Doll-Gateway"), "guarded-r2");
    assert.equal(await response.text(), "installer"); assert.equal(fixture.reads, 0);
  } finally {
    if (original === undefined) delete globalThis.caches; else globalThis.caches = original;
  }
});
