// 独立于 Rust SDK 的 MCP 测试端，覆盖 stdio、HTTP JSON/SSE 与会话标头。
import http from "node:http";
import readline from "node:readline";

const [mode, sessionMode] = process.argv.slice(2);
const observed = [];
const pending = new Map();
const schema = { type: "object", properties: { x: { type: "integer" } }, required: ["x"], additionalProperties: false };
const output = { type: "object", properties: { value: { type: "integer" } }, required: ["value"] };
const tool = (name) => ({
  name, description: name, inputSchema: name === "echo" ? schema : { type: "object" },
  ...(name === "echo" || name === "invalid" ? { outputSchema: output } : {}),
  annotations: { title: `标题 ${name}`, readOnlyHint: true },
  _meta: { "io.sleepy-doll/internal": true, "io.sleepy-doll/execution": { effect: "readOnly", concurrencySafe: true } },
});

function dispatch(message, headers, send) {
  const { id, method, params = {} } = message;
  observed.push({ method, session: headers["mcp-session-id"], protocol: headers["mcp-protocol-version"], custom: headers["x-fixture"] });
  const reply = (result, sse = false) => send({ jsonrpc: "2.0", id, result }, sse);
  if (method === "initialize") return reply({ protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "fixture", version: "1" }, instructions: "使用 echo 验证连接。" });
  if (method === "notifications/cancelled") {
    clearTimeout(pending.get(params.requestId));
    pending.delete(params.requestId);
    return send(null);
  }
  if (id === undefined) return send(null);
  if (method === "tools/list") return params.cursor
    ? reply({ tools: [tool("inspect"), tool("fail"), tool("invalid"), tool("content")] })
    : reply({ tools: [tool("echo"), tool("slow")], nextCursor: "second" }, true);
  if (method === "tools/call") {
    const { name, arguments: args = {} } = params;
    if (name === "disconnect") process.exit(0);
    if (name === "slow") {
      pending.set(id, setTimeout(() => { pending.delete(id); reply({ content: [{ type: "text", text: "done" }] }); }, 10000));
      return;
    }
    if (name === "fail") return reply({ isError: true, content: [{ type: "text", text: "任务参数与当前状态不匹配" }] });
    if (name === "content") return reply({ content: [{ type: "text", text: "画面" }, { type: "image", data: "eA==", mimeType: "image/png" }] });
    const value = name === "inspect" ? { observed } : name === "invalid" ? { value: "invalid" } : { value: args.x };
    return reply({ structuredContent: value, content: [{ type: "text", text: JSON.stringify(value) }] });
  }
  send({ jsonrpc: "2.0", id, error: { code: -32601, message: "Method not found" } });
}

if (mode === "stdio") {
  const input = readline.createInterface({ input: process.stdin });
  input.on("line", (line) => dispatch(JSON.parse(line), {}, (message) => { if (message) process.stdout.write(`${JSON.stringify(message)}\n`); }));
  input.on("close", () => { for (const timer of pending.values()) clearTimeout(timer); });
} else if (mode === "http") {
  const server = http.createServer((request, response) => {
    if (request.url.startsWith("/bridge/v1/")) {
      if (request.url === "/bridge/v1/events") {
        response.writeHead(200, { "content-type": "text/event-stream" }).end("id: event-7\ndata: {}\n\n");
      } else {
        let body = "";
        request.on("data", (chunk) => { body += chunk; });
        request.on("end", () => response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({ path: request.url, method: request.method, body: body ? JSON.parse(body) : null })));
      }
      return;
    }
    if (request.method === "DELETE") { response.writeHead(204).end(); return; }
    if (request.method !== "POST") { response.writeHead(405).end(); return; }
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      const message = JSON.parse(body);
      dispatch(message, request.headers, (result, sse) => {
        if (!result) { response.writeHead(202).end(); return; }
        const headers = { "content-type": sse ? "text/event-stream" : "application/json" };
        if (message.method === "initialize" && sessionMode !== "stateless") headers["Mcp-Session-Id"] = "fixture-session";
        response.writeHead(200, headers).end(sse ? `data: ${JSON.stringify(result)}\n\n` : JSON.stringify(result));
      });
    });
  });
  server.listen(0, "127.0.0.1", () => process.stdout.write(`http://127.0.0.1:${server.address().port}/mcp\n`));
  process.stdin.resume();
  process.stdin.on("end", () => { for (const timer of pending.values()) clearTimeout(timer); server.closeAllConnections(); server.close(); });
} else {
  throw new Error("Unknown MCP fixture mode");
}
