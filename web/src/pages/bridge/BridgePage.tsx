import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import { BridgeIcon, ChevronIcon, HistoryIcon } from "../../components/icons";
import { readError } from "../../session";
import type { Bootstrap } from "../../ipc/types";
import { BridgeApiExplorer } from "../../components/bridge/BridgeApiExplorer";
import { BridgeRecovery } from "../../components/bridge/BridgeRecovery";
import { Toast } from "../../components/overlay/Toast";
import "./BridgePage.css";
import { useT } from "../../i18n";
export function BridgePage({
  bootstrap,
  reload,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}) {
  const t = useT();
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
  // 不暴露配置里的 enabled 开关：它默认就是开的，不代表连接状态。
  const connection = busy
    ? { title: t.bridge.connecting, detail: t.bridge.connectingNote }
    : bridge.connected
      ? { title: t.nav.connected, detail: t.bridge.bgRunning }
      : { title: t.nav.disconnected, detail: t.bridge.bgNotRunning };
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
            {busy ? t.bridge.connectingShort : bridge.connected ? t.bridge.reconnect : t.bridge.connectButton}
          </button>
        </div>
        <div className="bridge-endpoint">
          <span>{t.bridge.addrLabel}</span>
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
            <strong>{t.bridge.methodCatalog}</strong>
            <small>{bridge.connected ? t.bridge.viewMethods : t.bridge.connectToView}</small>
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
            <strong>{t.bridge.recoveryHeading}</strong>
            <small>{t.bridge.recoveryDesc}</small>
          </span>
          <ChevronIcon />
        </button>
      </div>
    </div>
  );
}
