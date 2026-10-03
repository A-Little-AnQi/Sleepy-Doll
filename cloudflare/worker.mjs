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
  async fetch(request, env) {
    const url = new URL(request.url);
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
