import { Button } from "@sleepy-doll/ui";
import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { api, type ReleaseState } from "../ipc/api";
import { Select } from "@sleepy-doll/ui/controls/Select";
import { SettingRow } from "@sleepy-doll/ui/controls/SettingRow";
import { Dialog } from "@sleepy-doll/ui/overlay/Dialog";
import { Toast } from "@sleepy-doll/ui/overlay/Toast";
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
  revised: "当前版本有修订",
  later: "稍后",
  verified: "下载完成，校验已通过。安装时将关闭并重启软件。",
  preserve: "更新会保留现有数据。",
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
  revised: "A revision is available",
  later: "Later",
  verified: "Download verified. Installing will close and restart the app.",
  preserve: "Your data will be kept.",
};

function ReleaseActions({
  state,
  onState,
  onDismiss,
}: {
  state: ReleaseState;
  onState(state: ReleaseState): void;
  onDismiss?(): void;
}) {
  const locale = useLocale();
  const text = locale === "en" ? en : zh;
  const [phase, setPhase] = useState<"idle" | "downloading" | "installing">(
    "idle",
  );
  const [error, setError] = useState("");
  if (!state.release) return null;
  const release = state.release;
  const notes = release.notes
    .trim()
    .replace(/^#\s+([^\r\n]+)\r?\n+/, (heading, title: string) =>
      [release.version, `v${release.version}`].includes(title.trim())
        ? ""
        : heading,
    );
  const available =
    release.version === state.currentVersion ? text.revised : text.available;
  const size =
    release.size > 0
      ? `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(release.size / 1024 / 1024)} MB`
      : "";
  const channel = release.channel === "test" ? text.test : text.stable;
  const busy = phase !== "idle";
  return (
    <div
      className={`release-actions${onDismiss ? " is-notification" : ""}`}
      aria-busy={busy}
    >
      <div className="release-info">
        <strong>
          {available} · {release.version}
        </strong>
        <span className="release-metadata">
          {text.current} {state.currentVersion} · {channel}
          {size ? ` · ${size}` : ""}
        </span>
      </div>
      {notes && (
        <div
          className="release-notes"
          tabIndex={0}
          role="region"
          aria-label={text.title}
        >
          <Markdown remarkPlugins={[remarkGfm]}>{notes}</Markdown>
        </div>
      )}
      <p className="release-progress" role="status" aria-live="polite">
        {busy
          ? phase === "installing"
            ? text.installing
            : text.downloading
          : state.downloaded
            ? text.verified
            : text.preserve}
      </p>
      {error && (
        <p className="release-error" role="alert">
          {error}
        </p>
      )}
      <div className="release-action-buttons">
        {onDismiss && (
          <Button
            type="button"
            variant="secondary"
            disabled={phase === "installing"}
            onClick={onDismiss}
          >
            {text.later}
          </Button>
        )}
        <Button
          type="button"
          variant="primary"
          disabled={busy}
          onClick={async () => {
            setPhase(state.downloaded ? "installing" : "downloading");
            setError("");
            try {
              if (state.downloaded) {
                await api.releaseInstall();
              } else {
                onState(await api.releaseDownload());
                setPhase("idle");
              }
            } catch (reason) {
              setError(readError(reason));
              setPhase("idle");
            }
          }}
        >
          {busy
            ? phase === "installing"
              ? text.installing
              : text.downloading
            : state.downloaded
              ? text.install
              : text.download}
        </Button>
      </div>
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
          <Button
            type="button"
            variant="secondary"
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
          </Button>
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
      {message && <Toast message={message} onDismiss={() => setMessage("")} />}
    </>
  );
}

export function ReleaseNotifications({ paused = false }: { paused?: boolean }) {
  const text = useLocale() === "en" ? en : zh;
  const [state, setState] = useState<ReleaseState | null>(null);
  const [open, setOpen] = useState(false);
  const [ready, setReady] = useState(!paused);
  useEffect(() => {
    if (paused) {
      setReady(false);
      return;
    }
    // 等前一个 Dialog 的退出动画结束，避免两层遮罩和焦点陷阱叠加。
    const timer = window.setTimeout(() => setReady(true), 300);
    return () => window.clearTimeout(timer);
  }, [paused]);
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
    <Dialog
      title={text.title}
      open={open && !paused && ready}
      onClose={() => setOpen(false)}
      compact
      className="release-update-dialog"
    >
      {state && (
        <ReleaseActions
          state={state}
          onState={setState}
          onDismiss={() => setOpen(false)}
        />
      )}
    </Dialog>
  );
}
