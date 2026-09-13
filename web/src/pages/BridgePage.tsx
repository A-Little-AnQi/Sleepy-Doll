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
import { Toast } from "../components/Toast";
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
  // 状态和失败原因写在同一个地方。这里不暴露配置里的 enabled 开关 ——
  // 它默认就是开的，拿它当连接状态会让「没连接」显示成「已启用」。
  const connection = busy
    ? { title: "正在连接", detail: "正在注入本地桥并等待握手。" }
    : bridge.connected
      ? {
          title: "已连接",
          detail: "桥在 BetterGI 进程内提供接口；关掉 BetterGI 后桥就没了，重新打开后点一次连接。",
        }
      : {
          title: "未连接",
          detail: "启动 BetterGI 后点「连接 BetterGI」。桥只在本机回环上监听。",
        };
  return (
    <div className="page-sheet bridge-page">
      <header className="bridge-overview" data-motion="panel">
        <div>
          <span className="bridge-eyebrow">本地连接</span>
          <h2>BetterGI</h2>
          <p>管理宿主连接、接口契约和可恢复的配置变更。</p>
        </div>
      </header>
      {bridge.simulated && (
        <p className="notice">当前显示模拟数据，不代表真实 BetterGI 状态。</p>
      )}
      {(error || notice) && (
        <Toast
          message={error || notice}
          onDismiss={() => {
            setError("");
            setNotice("");
          }}
        />
      )}
      <section className="bridge-connection-card" data-motion="panel">
        <div className="bridge-connection-row">
          <div>
            <strong
              className={bridge.connected ? "is-connected" : undefined}
            >
              {connection.title}
            </strong>
            <span>{connection.detail}</span>
          </div>
          <button
            className="primary-action"
            disabled={busy}
            onClick={() => void toggle(true)}
          >
            <RefreshIcon className="button-icon" />
            {busy ? "连接中…" : bridge.connected ? "重新连接" : "连接 BetterGI"}
          </button>
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
