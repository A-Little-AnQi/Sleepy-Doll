import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import { BridgeIcon, ChevronIcon, HistoryIcon } from "../../components/icons";
import { readError } from "../../session";
import type { Bootstrap } from "../../ipc/types";
import { BridgeApiExplorer } from "../../components/bridge/BridgeApiExplorer";
import { BridgeRecovery } from "../../components/bridge/BridgeRecovery";
import { Toast } from "../../components/overlay/Toast";
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
  const [showCatalog, setShowCatalog] = useState(false);
  const [showRecovery, setShowRecovery] = useState(false);
  const bridge = bootstrap.bridge;
  useEffect(() => {
    void reload();
  }, [reload]);
  const toggle = async (enabled: boolean) => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await api.setBridgeEnabled(enabled);
      if (result.warning) setNotice(result.warning);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      await reload();
      setBusy(false);
    }
  };
  if (showCatalog)
    return <BridgeApiExplorer onBack={() => setShowCatalog(false)} />;
  if (showRecovery)
    return <BridgeRecovery onBack={() => setShowRecovery(false)} />;
  // 这里不暴露配置里的 enabled 开关 —— 它默认就是开的，拿它当连接状态会让
  // 「没连接」显示成「已启用」。
  const connection = busy
    ? { title: "正在连接", detail: "正在连接 BetterGI。" }
    : bridge.connected
      ? { title: "已连接", detail: "BetterGI 正在运行。" }
      : { title: "未连接", detail: "请先启动 BetterGI。" };
  return (
    <div className="page-sheet bridge-page">
      <header className="bridge-overview" data-motion="panel">
        <h2>BetterGI</h2>
      </header>
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
            <strong>{connection.title}</strong>
            <span>{connection.detail}</span>
          </div>
          <button
            className="primary-action"
            disabled={busy}
            onClick={() => void toggle(true)}
          >
            {busy ? "连接中…" : bridge.connected ? "重新连接" : "连接 BetterGI"}
          </button>
        </div>
        <div className="bridge-endpoint">
          <span>地址</span>
          <code>{bridge.baseUrl}</code>
        </div>
      </section>
      <div className="bridge-feature-grid" data-motion="panel">
        <button
          type="button"
          className="bridge-feature"
          disabled={!bridge.connected}
          onClick={() => setShowCatalog(true)}
        >
          <BridgeIcon />
          <span>
            <strong>接口目录</strong>
            <small>{bridge.connected ? "查看可用接口" : "连接后可查看"}</small>
          </span>
          <ChevronIcon />
        </button>
        <button
          type="button"
          className="bridge-feature"
          onClick={() => setShowRecovery(true)}
        >
          <HistoryIcon />
          <span>
            <strong>配置恢复</strong>
            <small>还原之前的配置</small>
          </span>
          <ChevronIcon />
        </button>
      </div>
    </div>
  );
}
