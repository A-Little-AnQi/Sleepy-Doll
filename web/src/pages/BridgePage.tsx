import { useState } from "react";

import { api } from "../api";
import { StateScanButton } from "../components/actions/SleepyActionButtons";
import type { Bootstrap } from "../types";

function BridgeIllustration({ connected }: { connected: boolean }) {
  return (
    <svg
      className="bridge-illustration"
      viewBox="0 0 680 520"
      aria-hidden="true"
    >
      <path
        className="bridge-face left"
        d="M35 395c52-45 111-59 178-42 35 9 64 30 91 62H35Z"
      />
      <path
        className="bridge-face right"
        d="M376 415c31-39 67-62 108-68 62-9 116 7 161 48v20Z"
      />
      <path className="bridge-edge" d="M78 286Q340 50 602 286" />
      <path
        className="bridge-link"
        d="M92 306h496M151 226v80M214 169v137M277 131v175M340 118v188M403 131v175M466 169v137M529 226v80M151 306v109M529 306v109"
      />
      <path
        className="bridge-current"
        d="M96 291Q340 82 584 291"
      />
      <g className="bridge-signal bridge-screen">
        <path d="M74 332h76v50H74ZM94 397h36M112 382v15" />
        <path d="m87 347 11 10 20-20" />
      </g>
      <g className="bridge-signal bridge-controller">
        <path d="M535 349c6-18 18-25 35-25s29 7 35 25l10 31c4 13-10 22-19 13l-12-12h-28l-12 12c-9 9-23 0-19-13Z" />
        <path d="M545 352h18M554 343v18M588 346h.1M598 357h.1" />
      </g>
      {connected ? (
        <path className="bridge-confirm" d="m309 259 25 24 48-51" />
      ) : null}
    </svg>
  );
}

function summarizeBridgeState(value: unknown) {
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
        ? `当前位置：${position.value}`
        : "未能识别当前位置",
    activity: runtime.activeJobId
      ? "有一项操作正在运行"
      : "当前没有运行中的操作",
  };
}

export function BridgePage({ bootstrap }: { bootstrap: Bootstrap }) {
  const [state, setState] = useState<unknown>();
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const summary = state ? summarizeBridgeState(state) : undefined;

  const inspect = async () => {
    setLoading(true);
    setError("");
    try {
      setState(await api.bridgeState());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="bridge-scene">
      <BridgeIllustration connected={bootstrap.bridge.connected} />
      <header>
        <h1>
          {bootstrap.bridge.connected ? "BetterGI 在这里" : "连接 BetterGI"}
        </h1>
        <p>
          {bootstrap.bridge.connected
            ? "可以读取游戏状态，也可以调用已经授权的能力。"
            : "启动 Remote Bridge 后，Sleepy Doll 才能观察和执行。"}
        </p>
        <div className="bridge-actions">
          <StateScanButton
            label="读取当前画面"
            status={loading ? "working" : state ? "success" : "idle"}
            disabled={!bootstrap.bridge.enabled || loading}
            onClick={() => void inspect()}
          />
          <span>{loading ? "正在读取" : "读取当前画面"}</span>
        </div>
      </header>
      <code className="bridge-address">{bootstrap.bridge.baseUrl}</code>
      {error ? <div className="bridge-error">{error}</div> : null}
      {summary ? (
        <section className="state-view" aria-live="polite">
          <h2>刚刚读到</h2>
          <p>{summary.position}</p>
          <p>{summary.activity}</p>
        </section>
      ) : null}
    </section>
  );
}
