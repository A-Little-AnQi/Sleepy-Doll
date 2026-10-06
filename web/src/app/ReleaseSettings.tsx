import { useEffect, useState } from "react";
import { api, type ReleaseState } from "../ipc/api";
import { Select } from "../components/controls/Select";
import { SettingRow } from "../components/controls/SettingRow";
import { Dialog } from "../components/overlay/Dialog";
import { Toast } from "../components/overlay/Toast";
import { useLocale } from "../appearance/locale";
import { readError } from "../session";
import "./ReleaseSettings.css";

const zh = {
  title: "软件更新",
  check: "检查更新",
  checking: "正在检查…",
  current: "当前版本",
  channel: "更新通道",
  stable: "正式版",
  test: "测试版",
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
    <div className="release-actions">
      <div className="release-info">
        <strong>
          {text.available} · {state.release.version}
        </strong>
        <p className="release-notes">{state.release.notes}</p>
      </div>
      <button
        type="button"
        className="primary-action"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError("");
          try {
            if (state.downloaded) await api.releaseInstall();
            else onState(await api.releaseDownload());
          } catch (reason) {
            setError(readError(reason));
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
      {error && <p className="release-error" role="alert">{error}</p>}
    </div>
  );
}

/**
 * 「关于」页的软件更新区：release.state 是唯一数据源，
 * 检查结果与错误走 Toast 浮层，不插入设置列表。
 */
export function ReleaseSettings() {
  const text = useLocale() === "en" ? en : zh;
  const [state, setState] = useState<ReleaseState | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    void api
      .releaseState()
      .then(setState)
      .catch((reason) => setMessage(readError(reason)));
  }, []);
  // configure 只负责换通道：统计开关沿用服务端既有偏好，不得在此改写。
  async function configure(channel: string) {
    try {
      setState(
        await api.releaseConfigure(state?.analyticsEnabled ?? false, channel),
      );
    } catch (reason) {
      setMessage(readError(reason));
    }
  }
  return (
    <>
      <section className="settings-group release-settings">
        <h3>{text.title}</h3>
        <SettingRow
          label={text.current}
          hint={`${state?.currentVersion ?? __APP_VERSION__} · ${text[state?.currentChannel ?? __APP_CHANNEL__]}`}
        >
          <button
            type="button"
            className="secondary-action"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setMessage("");
              try {
                const next = await api.releaseCheck();
                setState(next);
                if (!next.release) setMessage(text.latest);
              } catch (reason) {
                setMessage(readError(reason));
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? text.checking : text.check}
          </button>
        </SettingRow>
        {state && (
          <SettingRow label={text.channel}>
            <Select
              label={text.channel}
              value={state.channel}
              options={[
                { value: "stable", label: text.stable },
                { value: "test", label: text.test },
              ]}
              onChange={(channel) => void configure(channel)}
            />
          </SettingRow>
        )}
        {state && <ReleaseActions state={state} onState={setState} />}
      </section>
      {message && (
        <Toast message={message} onDismiss={() => setMessage("")} />
      )}
    </>
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
