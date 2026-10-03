import { useEffect, useState } from "react";
import { api, type ReleaseState } from "../ipc/api";
import { Select } from "../components/controls/Select";
import { SettingRow } from "../components/controls/SettingRow";
import { Dialog } from "../components/overlay/Dialog";
import { useLocale } from "../appearance/locale";

const zh = {
  title: "软件更新",
  check: "检查更新",
  checking: "正在检查…",
  current: "当前版本",
  channel: "更新通道",
  stable: "正式版",
  test: "测试版",
  analytics: "匿名基础统计",
  analyticsHint: "统计启动、检查更新和更新结果。",
  on: "开启",
  off: "关闭",
  latest: "当前已是最新版本，或所选通道尚未发布。",
  download: "下载更新",
  downloading: "正在下载并校验…",
  install: "安装并重启",
  installing: "正在准备重启…",
  available: "发现新版本",
};
const en: typeof zh = {
  title: "Software updates",
  check: "Check for updates",
  checking: "Checking…",
  current: "Current version",
  channel: "Update channel",
  stable: "Stable",
  test: "Test",
  analytics: "Anonymous basic analytics",
  analyticsHint: "Record launches, update checks and update results.",
  on: "On",
  off: "Off",
  latest: "Up to date, or no release in this channel.",
  download: "Download update",
  downloading: "Downloading and verifying…",
  install: "Install and restart",
  installing: "Preparing restart…",
  available: "New version available",
};

function ReleaseActions({
  state,
  onState,
}: {
  state: ReleaseState;
  onState(state: ReleaseState): void;
}) {
  const text = useLocale() === "en" ? en : zh;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  if (!state.release) return null;
  return (
    <div>
      <p>
        {text.available} · {state.release.version}
      </p>
      <p style={{ whiteSpace: "pre-wrap" }}>{state.release.notes}</p>
      <button
        type="button"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError("");
          try {
            if (state.downloaded) await api.releaseInstall();
            else onState(await api.releaseDownload());
          } catch (reason) {
            setError(String(reason));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy
          ? state.downloaded
            ? text.installing
            : text.downloading
          : state.downloaded
            ? text.install
            : text.download}
      </button>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

export function ReleaseSettings() {
  const text = useLocale() === "en" ? en : zh;
  const [state, setState] = useState<ReleaseState | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    void api
      .releaseState()
      .then(setState)
      .catch((reason) => setMessage(String(reason)));
  }, []);
  async function configure(enabled: boolean, channel: string) {
    try {
      setState(await api.releaseConfigure(enabled, channel));
    } catch (reason) {
      setMessage(String(reason));
    }
  }
  return (
    <section className="settings-group">
      <h3>{text.title}</h3>
      <p>
        {text.current} · {state?.currentVersion ?? __APP_VERSION__}
      </p>
      {state && (
        <>
          <SettingRow label={text.channel}>
            <Select
              label={text.channel}
              value={state.channel}
              options={[
                { value: "stable", label: text.stable },
                { value: "test", label: text.test },
              ]}
              onChange={(channel) =>
                void configure(state.analyticsEnabled, channel)
              }
            />
          </SettingRow>
          <SettingRow label={text.analytics} hint={text.analyticsHint}>
            <Select
              label={text.analytics}
              value={state.analyticsEnabled ? "on" : "off"}
              options={[
                { value: "on", label: text.on },
                { value: "off", label: text.off },
              ]}
              onChange={(value) =>
                void configure(value === "on", state.channel)
              }
            />
          </SettingRow>
        </>
      )}
      <button
        type="button"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setMessage("");
          try {
            const next = await api.releaseCheck();
            setState(next);
            if (!next.release) setMessage(text.latest);
          } catch (reason) {
            setMessage(String(reason));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? text.checking : text.check}
      </button>
      {message && <p role="status">{message}</p>}
      {state && <ReleaseActions state={state} onState={setState} />}
    </section>
  );
}

export function ReleaseNotifications() {
  const text = useLocale() === "en" ? en : zh;
  const [state, setState] = useState<ReleaseState | null>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!window.__SLEEPY_DOLL_DESKTOP__) return;
    let active = true;
    void api
      .releaseCheck()
      .then((next) => {
        if (active && next.release) {
          setState(next);
          setOpen(true);
        }
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);
  return (
    <Dialog title={text.title} open={open} onClose={() => setOpen(false)}>
      {state && <ReleaseActions state={state} onState={setState} />}
    </Dialog>
  );
}
