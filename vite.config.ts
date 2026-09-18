import fs from "node:fs";
import http from "node:http";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const IPC_PORT_FILE = fileURLToPath(
  new URL("./.sleepy-doll/dev/ipc.port", import.meta.url),
);

function ipcPort() {
  try {
    const port = Number(fs.readFileSync(IPC_PORT_FILE, "utf8").trim());
    if (Number.isInteger(port) && port >= 1 && port <= 65535) return port;
  } catch {
    // sleepy-doll-dev 还没写出端口文件时，沿用默认。
  }
  return 47124;
}

function ipcProxy(): Plugin {
  return {
    name: "sleepy-doll:ipc-proxy",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url ?? "";
        if (!url.startsWith("/ipc")) {
          next();
          return;
        }
        const port = ipcPort();
        const proxy = http.request(
          {
            hostname: "127.0.0.1",
            port,
            path: url,
            method: req.method,
            headers: { ...req.headers, host: `127.0.0.1:${port}` },
            timeout: 120_000,
          },
          (incoming) => {
            res.writeHead(incoming.statusCode ?? 502, incoming.headers);
            incoming.pipe(res);
          },
        );
        proxy.on("timeout", () => {
          proxy.destroy();
          if (!res.headersSent) {
            res.statusCode = 504;
            res.end();
          }
        });
        proxy.on("error", () => {
          if (!res.headersSent) {
            res.statusCode = 502;
            res.end();
          }
        });
        req.pipe(proxy);
      });
    },
  };
}

/**
 * 主程序的组件样式是按「product.css 排在最后」写的，同权重时由它盖过组件样式。
 * 两个窗口共用这份样式后它落进共享 chunk，先于入口 chunk 加载，覆盖关系会反过来。
 * 这里把主窗口的共享样式移回入口样式之后；安装窗口不用动，它的样式本来就压在
 * product.css 上面。
 */
function designCssLast(): Plugin {
  const own = (chunk: unknown) =>
    (chunk as { viteMetadata?: { importedCss: Set<string> } } | undefined)
      ?.viteMetadata?.importedCss;
  return {
    name: "sleepy-doll:design-css-last",
    apply: "build",
    enforce: "post",
    transformIndexHtml: {
      order: "post",
      handler(html, ctx) {
        if (!ctx.filename.endsWith("index.html")) return html;
        const css = own(ctx.chunk);
        const stylesheet = /<link[^>]*rel="stylesheet"[^>]*>/g;
        const tags = html.match(stylesheet);
        if (!css?.size || !tags || tags.length < 2) return html;
        const isOwn = (tag: string) =>
          [...css].some((file) => tag.includes(file));
        const ordered = [
          ...tags.filter(isOwn),
          ...tags.filter((tag) => !isOwn(tag)),
        ];
        let next = 0;
        return html.replace(stylesheet, (tag) => ordered[next++] ?? tag);
      },
    },
  };
}

export default defineConfig({
  plugins: [react(), designCssLast(), ipcProxy()],
  root: "web",
  build: {
    outDir: "../target/ui",
    emptyOutDir: true,
    rollupOptions: {
      // 两个窗口各一份 HTML：主程序加载 index.html，安装程序加载 setup.html。
      input: {
        index: fileURLToPath(new URL("./web/index.html", import.meta.url)),
        setup: fileURLToPath(new URL("./web/setup.html", import.meta.url)),
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: false,
    watch: process.env.WSL_DISTRO_NAME
      ? {
          usePolling: true,
          interval: 250,
          awaitWriteFinish: { stabilityThreshold: 400, pollInterval: 100 },
        }
      : undefined,
  },
});
