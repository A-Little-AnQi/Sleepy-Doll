import { useState } from "react";

import { api } from "../api";
import { AlertIcon, CheckIcon, RefreshIcon } from "../components/icons";
import type { Bootstrap } from "../types";

function summarize(value: unknown) {
  const root =
    value && typeof value === "object"
      ? (value as Record<string, unknown>)
      : {};
  const position =
    root.position && typeof root.position === "object"
      ? (root.position as Record<string, unknown>)
      : {};
  const runtime =
    root.runtime && typeof root.runtime === "object"
      ? (root.runtime as Record<string, unknown>)
      : {};
  return {
    position:
      typeof position.value === "string" && position.value.trim()
        ? position.value
        : "",
    busy: Boolean(runtime.activeJobId),
  };
}

export function BridgePage({
  bootstrap,
  reload,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}) {
  const [state, setState] = useState<unknown>();
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const bridge = bootstrap.bridge;
  const summary = state ? summarize(state) : undefined;

  const inspect = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await api.bridgeState();
      setState(result);
      await reload();
    } catch (reason) {
      setState(undefined);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  // Every state explains what to do next. The previous version greyed the
  // button out and left the reason in `bridge.error`, unread.
  const status = !bridge.enabled
    ? {
        tone: "is-off",
        title: "还没有开启 BetterGI 连接",
        body: `Sleepy Doll 目前不会读取游戏，也不会执行任何操作。要开启它，在配置文件里把 bridge 段的 enabled 改成 true，并填上 BetterGI 那边的地址（当前填的是 ${bridge.baseUrl}）。`,
      }
    : bridge.connected
      ? {
          tone: "is-ok",
          title: "已经连上 BetterGI",
          body: "可以读取游戏状态，也可以在征得你同意后执行操作。",
        }
      : {
          tone: "is-bad",
          title: "配置里开了，但连不上 BetterGI",
          body: `Sleepy Doll 正在往 ${bridge.baseUrl} 发请求。请确认 BetterGI 已经启动、并且开启了对应的远程接口。`,
        };

  return (
    <div className="page-sheet bridge-page">
      <section className={`status-card ${status.tone}`}>
        <div className="status-card-head">
          {status.tone === "is-ok" ? (
            <CheckIcon className="status-icon" />
          ) : (
            <AlertIcon className="status-icon" />
          )}
          <h2>{status.title}</h2>
        </div>
        <p>{status.body}</p>
        {bridge.error ? (
          <p className="status-detail">
            BetterGI 返回的错误：<code>{bridge.error}</code>
          </p>
        ) : null}
      </section>

      <section className="page-block">
        <div className="block-head">
          <h2>读取当前画面</h2>
          <button
            type="button"
            className="primary-action"
            disabled={!bridge.enabled || loading}
            title={
              bridge.enabled ? undefined : "BetterGI 连接未开启，先在配置里启用"
            }
            onClick={() => void inspect()}
          >
            <RefreshIcon className="button-icon" />
            {loading ? "正在读取…" : "读取当前画面"}
          </button>
        </div>
        <p className="muted">
          向 BetterGI 要一次当前游戏状态。它不会做任何操作。
        </p>

        {error ? (
          <div className="inline-error" role="alert">
            <strong>读取失败</strong>
            <span>{error}</span>
            <span className="muted">
              确认游戏在前台、BetterGI 正在运行，然后重试。
            </span>
          </div>
        ) : null}

        {summary ? (
          <div className="state-view" aria-live="polite">
            <h3>读到的内容</h3>
            <p>
              {summary.position
                ? `当前位置：${summary.position}`
                : "没能识别出当前位置。"}
            </p>
            <p>
              {summary.busy
                ? "BetterGI 那边有一项操作正在运行。"
                : "当前没有正在运行的操作。"}
            </p>
            <details>
              <summary>查看完整返回</summary>
              <pre>{JSON.stringify(state, null, 2)}</pre>
            </details>
          </div>
        ) : null}
      </section>
    </div>
  );
}
