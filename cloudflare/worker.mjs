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
    return new Response(request.method === "HEAD" ? null : PAGE, {
      headers: {
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "public, max-age=60",
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "strict-origin-when-cross-origin",
      },
    });
  },
};

const PAGE = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Sleepy Doll · 下载</title>
<style>:root{color-scheme:light dark;font-family:system-ui,sans-serif}body{max-width:720px;margin:12vh auto;padding:24px;line-height:1.8}h1{font-size:40px;margin-bottom:8px}p{opacity:.8}section{border:1px solid #8885;border-radius:16px;padding:24px;margin:24px 0}a{color:inherit}a.download{display:inline-block;padding:10px 20px;background:#a0c4b5;color:#10211a;border-radius:8px;text-decoration:none}pre{white-space:pre-wrap;font:inherit}small{opacity:.7}</style>
<h1>Sleepy Doll</h1><p>通用本地桌面 Agent，当前已适配 BetterGI。Windows x64。</p>
<section><h2>正式版</h2><div id="stable">正在读取发布信息…</div></section>
<section><h2>测试版</h2><p>用于验证发布与更新链路。</p><div id="test">正在读取发布信息…</div></section>
<p><a href="https://github.com/A-Little-AnQi/Sleepy-Doll">源码与发布记录</a></p>
<section><h2>匿名基础统计</h2><p>允许后，将访问和下载点击事件发送至 Google Analytics；不发送姓名、对话或密钥。软件内统计可在设置中单独开启或关闭。</p><button id="allow">允许统计</button> <button id="decline">关闭统计</button><p id="consent"></p></section>
<script>const sessionId=Math.floor(Date.now()/1000);
function event(name,version,channel){if(localStorage.getItem('sleepy-analytics-consent')!=='yes')return;const clientId=localStorage.getItem('sleepy-analytics-id')||crypto.randomUUID();localStorage.setItem('sleepy-analytics-id',clientId);fetch('/api/events',{method:'POST',headers:{'Content-Type':'application/json'},keepalive:true,body:JSON.stringify({clientId,sessionId,event:name,version,channel,platform:'web'})}).catch(()=>{});}
function consent(value){localStorage.setItem('sleepy-analytics-consent',value);document.getElementById('consent').textContent=value==='yes'?'统计已开启':'统计已关闭';if(value==='yes')event('page_view','0.0.0','stable');}
document.getElementById('allow').onclick=()=>consent('yes');document.getElementById('decline').onclick=()=>consent('no');document.getElementById('consent').textContent=localStorage.getItem('sleepy-analytics-consent')==='yes'?'统计已开启':'统计已关闭';
event('page_view','0.0.0','stable');
for(const channel of ['stable','test'])fetch('/api/releases/'+channel).then(async r=>{const box=document.getElementById(channel);if(r.status===404){box.textContent='尚未发布';return}if(!r.ok)throw Error();const release=await r.json();const version=document.createElement('p');version.textContent='版本 '+release.version+' · '+(release.size/1048576).toFixed(2)+' MiB';const a=document.createElement('a');a.className='download';a.textContent='下载 Windows 安装包';const link=new URL(release.url);if(link.protocol!=='https:'||link.hostname!=='sleepy-doll-download.restless-nh3.com')throw Error();a.href=link.href;a.addEventListener('click',()=>event('download_click',release.version,channel));const notes=document.createElement('pre');notes.textContent=release.notes;box.replaceChildren(version,a,notes);}).catch(()=>{document.getElementById(channel).textContent='发布信息暂时无法读取，请稍后重试。'});
</script></html>`;
