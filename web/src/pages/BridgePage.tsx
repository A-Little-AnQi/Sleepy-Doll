import { useEffect, useState } from "react";
import { api } from "../api";
import {
  BridgeIcon,
  ChevronIcon,
  HistoryIcon,
  RefreshIcon,
} from "../components/icons";
import type { Bootstrap } from "../types";
import { BridgeApiExplorer } from "../components/BridgeApiExplorer";
import { BridgeRecovery } from "../components/BridgeRecovery";
import "./BridgePage.css";
export function BridgePage({
  bootstrap,
  reload,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [state, setState] = useState<unknown>();
  const [showCatalog, setShowCatalog] = useState(false);
  const [showRecovery, setShowRecovery] = useState(false);
  const bridge = bootstrap.bridge;
  useEffect(() => {
    if (!bridge.enabled || busy) return;
    const timer = setInterval(() => void reload(), 5000);
    return () => clearInterval(timer);
  }, [bridge.enabled, busy, reload]);
  const toggle = async (enabled: boolean) => {
    setBusy(true);
    setError("");
    setNotice("");
    setState(undefined);
    try {
      const result = await api.setBridgeEnabled(enabled);
      if (result.warning) setNotice(result.warning);
    } catch (reason) {
      setError(String(reason));
    } finally {
      await reload();
      setBusy(false);
    }
  };
  if (showCatalog)
    return <BridgeApiExplorer onBack={() => setShowCatalog(false)} />;
  if (showRecovery)
    return <BridgeRecovery onBack={() => setShowRecovery(false)} />;
  const connectionLabel = busy
    ? "连接中"
    : bridge.enabled && bridge.connected
      ? "已连接"
      : bridge.enabled
        ? "连接断开"
        : "未启用";
  return (
    <div className="page-sheet bridge-page">
      <header className="bridge-overview" data-motion="panel">
        <div>
          <span className="bridge-eyebrow">本地连接</span>
          <h2>BetterGI</h2>
          <p>管理宿主连接、接口契约和可恢复的配置变更。</p>
        </div>
        <span
          className={
            "bridge-connection-state" +
            (bridge.connected ? " is-connected" : "")
          }
        >
          <i aria-hidden="true" />
          {connectionLabel}
        </span>
      </header>
      {bridge.simulated && (
        <p className="notice">当前显示模拟数据，不代表真实 BetterGI 状态。</p>
      )}
      <section className="bridge-connection-card" data-motion="panel">
        <div className="bridge-connection-row">
          <div>
            <strong>连接 BetterGI</strong>
            <span>启动宿主后加载本地桥；关闭时拒绝新的操作。</span>
          </div>
          <button
            className={`switch ${bridge.enabled ? "on" : ""}`}
            role="switch"
            aria-label="启用 BetterGI 连接"
            aria-checked={bridge.enabled}
            disabled={busy}
            onClick={() => void toggle(!bridge.enabled)}
          />
        </div>
        <div className="bridge-endpoint">
          <span>本地端点</span>
          <code>{bridge.baseUrl}</code>
        </div>
      </section>
      <div className="bridge-feature-grid" data-motion="panel">
        <button className="bridge-feature" onClick={() => setShowCatalog(true)}>
          <BridgeIcon />
          <span>
            <strong>接口目录</strong>
            <small>按用途查阅参数、影响、验证与回退说明</small>
          </span>
          <ChevronIcon />
        </button>
        <button
          className="bridge-feature"
          onClick={() => setShowRecovery(true)}
        >
          <HistoryIcon />
          <span>
            <strong>配置恢复</strong>
            <small>查看事务记录，在宿主退出后恢复备份</small>
          </span>
          <ChevronIcon />
        </button>
      </div>
      {(error || notice) && (
        <div className="inline-error" role="alert">
          {error || notice}
        </div>
      )}
      {bridge.enabled && !bridge.connected && !busy && (
        <section className="page-block">
          <p className="muted">启动 BetterGI 后重新连接。</p>
          {bridge.error && (
            <details>
              <summary>连接详情</summary>
              <pre>{bridge.error}</pre>
            </details>
          )}
          <div>
            <button
              className="secondary-action"
              onClick={() => void toggle(true)}
            >
              <RefreshIcon className="button-icon" />
              重新连接
            </button>
          </div>
        </section>
      )}
      <section className="bridge-status-section" data-motion="panel">
        <div className="block-head">
          <div>
            <h2>运行状态</h2>
            <p>按需读取一次，不在后台持续打扰宿主。</p>
          </div>
          <button
            className="secondary-action"
            disabled={busy || !bridge.connected}
            onClick={() => {
              setBusy(true);
              setError("");
              void api
                .bridgeState()
                .then(setState)
                .catch((reason) => setError(String(reason)))
                .finally(() => {
                  setBusy(false);
                  void reload();
                });
            }}
          >
            <RefreshIcon className="button-icon" />
            刷新
          </button>
        </div>
        {state ? (
          <pre>{JSON.stringify(state, null, 2)}</pre>
        ) : (
          <div className="bridge-state-empty">
            <BridgeIcon />
            <span>{bridge.connected ? "尚未读取" : "当前未连接"}</span>
          </div>
        )}
      </section>
      <details className="bridge-lifecycle">
        <summary>组件生命周期</summary>
        <p className="field-help">
          关闭连接后拒绝新操作，已启动的 BetterGI 任务可能继续运行。组件随
          BetterGI 退出卸载。
        </p>
      </details>
    </div>
  );
}
