const EVENTS = new Set([
  "page_view",
  "download_click",
  "app_start",
  "update_check",
  "update_available",
  "update_download_complete",
  "update_success",
  "update_failed",
]);
const VERSION = /^\d+\.\d+\.\d+$/;
const UUID = /^[a-f0-9-]{36}$/i;
const SITE = "https://sleepy-doll.restless-nh3.com";
const MAX_DOWNLOAD = 256 * 1024 * 1024;

function releaseKey(path) {
  const match = /^\/releases\/(\d+\.\d+\.\d+)\/(Sleepy-Doll-\d+\.\d+\.\d+-setup\.exe(?:\.sha256)?|release\.json)$/.exec(path);
  if (!match) return null;
  if (match[2] !== "release.json" && !match[2].startsWith(`Sleepy-Doll-${match[1]}-setup.exe`)) return null;
  return path.slice(1);
}

async function download(request, env, ctx, url) {
  const key = releaseKey(url.pathname);
  if (!key) return json({ error: "not-found" }, 404);
  if (url.search) return json({ error: "query-not-supported" }, 400);
  if (!["GET", "HEAD"].includes(request.method)) return json({ error: "method" }, 405);
  if (env.DOWNLOADS_DISABLED === "true") return json({ error: "downloads-paused" }, 503);
  const range = request.headers.get("Range");
  if (range && !/^bytes=(?:\d+-\d*|-\d+)$/.test(range)) return json({ error: "range" }, 416);
  if (!env.DOWNLOAD_LIMIT) return json({ error: "configuration" }, 503);
  const limited = await env.DOWNLOAD_LIMIT.limit({ key: request.headers.get("CF-Connecting-IP") ?? "unknown" });
  if (!limited.success) {
    const response = json({ error: "rate" }, 429);
    response.headers.set("Retry-After", "60");
    return response;
  }
  const cache = typeof caches === "undefined" ? null : caches.default;
  const cacheRequest = new Request(url, { method: "GET", headers: request.headers });
  const cached = cache && await cache.match(cacheRequest);
  if (cached) {
    if (request.method === "HEAD") { await cached.body?.cancel(); return new Response(null, cached); }
    return cached;
  }
  try {
    // 不读取目录、不探测元数据、不重试；每次进入此分支最多执行一次 R2 操作。
    const object = request.method === "HEAD"
      ? await env.RELEASES.head(key)
      : await env.RELEASES.get(key, { range: request.headers, onlyIf: request.headers });
    if (!object) return json({ error: "not-found" }, 404);
    if (object.size > MAX_DOWNLOAD || (object.storageClass && object.storageClass !== "Standard")) {
      await object.body?.cancel();
      return json({ error: "object-policy" }, 503);
    }
    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("ETag", object.httpEtag);
    headers.set("Accept-Ranges", "bytes");
    headers.set("Cache-Control", "public, max-age=31536000, immutable");
    headers.set("X-Content-Type-Options", "nosniff");
    headers.set("X-Sleepy-Doll-Gateway", "guarded-r2");
    if (request.method === "HEAD") return new Response(null, { headers: new Headers([...headers, ["Content-Length", String(object.size)]]) });
    if (!("body" in object)) return new Response(null, { status: request.headers.has("If-None-Match") ? 304 : 412, headers });
    let status = 200;
    if (object.range) {
      const offset = object.range.offset ?? Math.max(0, object.size - object.range.suffix);
      const length = object.range.length ?? (object.size - offset);
      headers.set("Content-Range", `bytes ${offset}-${offset + length - 1}/${object.size}`);
      headers.set("Content-Length", String(length));
      status = 206;
    } else headers.set("Content-Length", String(object.size));
    const response = new Response(object.body, { status, headers });
    if (cache && status === 200 && ctx?.waitUntil) ctx.waitUntil(cache.put(new Request(url), response.clone()).catch(() => {}));
    return response;
  } catch { return json({ error: "storage" }, 502); }
}

export function validateEvent(body) {
  if (
    !body ||
    !EVENTS.has(body.event) ||
    !UUID.test(body.clientId ?? "") ||
    !VERSION.test(body.version ?? "") ||
    !["test", "stable"].includes(body.channel) ||
    !["windows", "web"].includes(body.platform) ||
    !Number.isSafeInteger(body.sessionId) ||
    body.sessionId <= 0 ||
    (body.targetVersion != null && !VERSION.test(body.targetVersion))
  )
    return null;
  return {
    client_id: body.clientId,
    events: [
      {
        name: body.event,
        params: {
          app_version: body.version,
          release_channel: body.channel,
          platform: body.platform,
          session_id: body.sessionId,
          engagement_time_msec: 1,
          ...(body.targetVersion ? { target_version: body.targetVersion } : {}),
          ...(body.platform === "web"
            ? { page_location: SITE + ( /^\/docs(?:\/(?:guide|faq|changelog|developer))?$/.test(body.pagePath ?? "") ? "/#" + body.pagePath : "/" ), page_title: "Sleepy Doll" }
            : {}),
          ...(body.channel === "test" ? { debug_mode: true } : {}),
        },
      },
    ],
  };
}

const json = (body, status = 200) =>
  Response.json(body, {
    status,
    headers: {
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    },
  });

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/releases/")) return download(request, env, ctx, url);
    if (url.pathname === "/api/events") {
      if (request.method !== "POST") return json({ error: "method" }, 405);
      const origin = request.headers.get("Origin");
      if (origin && origin !== SITE) return json({ error: "origin" }, 403);
      if (
        !request.headers.get("Content-Type")?.startsWith("application/json") ||
        Number(request.headers.get("Content-Length") ?? 0) > 4096
      )
        return json({ error: "size" }, 400);
      const text = await request.text();
      if (text.length > 4096) return json({ error: "size" }, 400);
      let payload;
      try {
        payload = validateEvent(JSON.parse(text));
      } catch {
        return json({ error: "json" }, 400);
      }
      if (!payload) return json({ error: "event" }, 400);
      if (!env.GA_MEASUREMENT_ID || !env.GA_API_SECRET)
        return json({ error: "configuration" }, 503);
      const limited = await env.EVENT_LIMIT.limit({
        key: payload.client_id,
      });
      if (!limited.success) return json({ error: "rate" }, 429);
      const endpoint = new URL("https://www.google-analytics.com/mp/collect");
      endpoint.searchParams.set("measurement_id", env.GA_MEASUREMENT_ID);
      endpoint.searchParams.set("api_secret", env.GA_API_SECRET);
      try {
        const response = await fetch(endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
          signal: AbortSignal.timeout(5000),
        });
        return json({ accepted: response.ok }, response.ok ? 200 : 502);
      } catch {
        return json({ error: "upstream" }, 502);
      }
    }
    if (request.method !== "GET" && request.method !== "HEAD")
      return json({ error: "method" }, 405);
    const channel = /^\/api\/releases\/(test|stable)$/.exec(url.pathname)?.[1];
    if (channel) {
      const object = await env.RELEASES.get(`channels/${channel}.json`);
      if (!object) return json({ error: "no-release" }, 404);
      return new Response(request.method === "HEAD" ? null : object.body, {
        headers: {
          "Content-Type": "application/json",
          "Cache-Control": "no-store",
          "X-Content-Type-Options": "nosniff",
        },
      });
    }
    if (url.pathname === "/health")
      return json({ service: "sleepy-doll-distribution", status: "ok" });
    if (url.pathname !== "/") return json({ error: "not-found" }, 404);
    return Response.redirect(SITE + "/", 302);
  },
};
