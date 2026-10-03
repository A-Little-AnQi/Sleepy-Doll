import { useEffect, useState } from "react";
import { api, framelessWindow } from "../ipc/api";
import { readError } from "../session";
import { TitleBar } from "../components/shell/TitleBar";
import {
  AlertIcon,
  BrandIcon,
  CheckIcon,
  FolderIcon,
} from "../components/icons";
import { setupApi, type SetupInfo, type SetupState } from "./api";
import "./setup.css";
import { useT } from "../i18n";

/** 原生没应答时用它渲染，目录取安装器的首选值。 */
const PREVIEW_INFO: SetupInfo = {
  version: "0.1.0",
  directory: "D:\\Sleepy Doll",
  defaultDirectory: "D:\\Sleepy Doll",
  installed: false,
  installedVersion: null,
  uninstallMode: false,
};

const IDLE: SetupState = {
  phase: "idle",
  progress: 0,
  message: "",
  error: null,
};

/** 原生侧会把产品目录名接到所选目录后面。 */
export function installedDirectory(chosen: string) {
  const base = chosen.trim().replace(/[\\/]+$/, "");
  if (!base) return "";
  const last = base.split(/[\\/]/).pop()?.toLowerCase();
  return last === "sleepy doll" || last === "sleepy-doll"
    ? base
    : `${base}\\Sleepy Doll`;
}

/** 进度可能是 0–1 的小数，也可能是 0–100 的百分数。 */
export function progressPercent(progress: number) {
  const ratio = progress > 1 ? progress / 100 : progress;
  return Math.round(Math.min(1, Math.max(0, ratio)) * 100);
}

/** 已安装时预填现有目录。 */
function presetDirectory(info: SetupInfo) {
  return info.installed && info.directory
    ? info.directory
    : info.defaultDirectory;
}

export function SetupApp() {
  const t = useT();
  const [info, setInfo] = useState(PREVIEW_INFO);
  const [directory, setDirectory] = useState(PREVIEW_INFO.defaultDirectory);
  const [shortcut, setShortcut] = useState(true);
  const [removeUserData, setRemoveUserData] = useState(false);
  const [state, setState] = useState<SetupState>(IDLE);
  const [retry, setRetry] = useState(false);
  const [launchError, setLaunchError] = useState("");
  const [ready, setReady] = useState(!window.ipc);
  const [infoError, setInfoError] = useState("");

  useEffect(() => {
    window.__setupState = (next) => {
      setRetry(false);
      setState(next);
    };
    return () => {
      delete window.__setupState;
    };
  }, []);

  useEffect(() => {
    void setupApi
      .info()
      .then((next) => {
        setInfo(next);
        setDirectory(presetDirectory(next));
        setReady(true);
      })
      .catch((error) => {
        if (window.ipc) setInfoError(readError(error));
      });
  }, []);

  const uninstall = info.uninstallMode;
  const update = !uninstall && info.installed;
  const action = uninstall
    ? t.setup.uninstall
    : update
      ? t.setup.update
      : t.setup.install;
  const target =
    update || uninstall ? info.directory : installedDirectory(directory);
  const view = retry || state.phase === "idle" ? "form" : state.phase;
  const busy = view === "running";
  const percent = progressPercent(state.progress);

  async function start() {
    if (!ready || busy) return;
    setRetry(false);
    setState({
      phase: "running",
      progress: 0,
      message: t.setup.preparing,
      error: null,
    });
    try {
      if (uninstall) await setupApi.uninstall(removeUserData);
      else if (update) await setupApi.update();
      else await setupApi.install(directory.trim(), shortcut);
    } catch (error) {
      setState({
        phase: "failed",
        progress: 0,
        message: "",
        error: readError(error),
      });
    }
  }

  async function browse() {
    try {
      const picked = await setupApi.browse();
      if (!picked.directory) return;
      setDirectory(picked.directory);
    } catch {
      // 对话框打不开就保持原值。
    }
  }

  /** 启动刚装好的程序。主程序要求管理员，系统会先弹 UAC。 */
  async function launch() {
    setLaunchError("");
    try {
      const result = await setupApi.launch();
      if (!result.started) {
        setLaunchError(t.setup.notStarted);
        return;
      }
      await api.windowClose();
    } catch (error) {
      setLaunchError(readError(error));
    }
  }

  const resting = view === "form" || view === "running";

  return (
    <div className="setup-root">
      {framelessWindow() && (
        <TitleBar canMaximize={false} closeDisabled={busy} />
      )}
      <div className="setup-shell">
        <aside className="setup-aside">
          <BrandIcon className="setup-mark" />
          <div className="setup-brand">
            <h1>Sleepy Doll</h1>
            <span>版本 {info.version}</span>
          </div>
        </aside>
        <main className="setup-main">
          <header className="setup-head">
            <h2>{action} Sleepy Doll</h2>
            {resting ? (
              <p>
                {uninstall
                  ? t.setup.uninstallNote
                  : update
                    ? t.setup.updateNote
                    : t.setup.installNote}
              </p>
            ) : null}
          </header>

          <div className="setup-body">
            {resting ? (
              uninstall || update ? (
                <dl className="setup-meta">
                  <div className="setup-meta-row">
                    <dt>{t.setup.installLocation}</dt>
                    <dd className="setup-mono">{info.directory}</dd>
                  </div>
                  <div className="setup-meta-row">
                    <dt>{update ? t.setup.currentVersion : t.setup.version}</dt>
                    <dd>{info.installedVersion ?? t.setup.unknown}</dd>
                  </div>
                  {update ? (
                    <div className="setup-meta-row">
                      <dt>{t.setup.targetVersion}</dt>
                      <dd>{info.version}</dd>
                    </div>
                  ) : null}
                </dl>
              ) : (
                <div className="setup-field">
                  <label
                    className="setup-field-label"
                    htmlFor="setup-directory"
                  >
                    安装位置
                  </label>
                  <div className="setup-path">
                    <input
                      id="setup-directory"
                      value={directory}
                      disabled={busy || !ready}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(event) => {
                        setDirectory(event.target.value);
                      }}
                    />
                    <button
                      type="button"
                      className="secondary-action"
                      disabled={busy || !ready}
                      onClick={() => void browse()}
                    >
                      <FolderIcon className="button-icon" />
                      浏览
                    </button>
                  </div>
                  {target ? (
                    <p className="setup-note">
                      最终会装到 <span className="setup-mono">{target}</span>
                    </p>
                  ) : null}
                </div>
              )
            ) : null}

            {resting && uninstall ? (
              <div className="setup-choice">
                <label className="setup-check">
                  <input
                    type="checkbox"
                    checked={removeUserData}
                    disabled={busy || !ready}
                    onChange={(event) =>
                      setRemoveUserData(event.target.checked)
                    }
                  />
                  <span>同时删除数据（配置、模型密钥、会话记录、日志）</span>
                </label>
                <p className="setup-note">不勾选则保留，删除后无法恢复。</p>
              </div>
            ) : null}

            {infoError ? <p className="setup-note">{infoError}</p> : null}

            {resting && !uninstall && !update ? (
              <label className="setup-check">
                <input
                  type="checkbox"
                  checked={shortcut}
                  disabled={busy || !ready}
                  onChange={(event) => setShortcut(event.target.checked)}
                />
                <span>{t.setup.createShortcut}</span>
              </label>
            ) : null}

            {busy ? (
              <div className="setup-progress" role="status">
                <div className="setup-progress-head">
                  <span>{state.message || t.setup.progressLabel(action)}</span>
                  <span>{percent}%</span>
                </div>
                <div
                  className="setup-bar"
                  role="progressbar"
                  aria-label={t.setup.progressAria(action)}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                >
                  <div
                    className="setup-bar-fill"
                    style={{ width: `${percent}%` }}
                  />
                </div>
                <p className="setup-note">{t.setup.keepWindowOpen}</p>
              </div>
            ) : null}

            {view === "done" ? (
              <div className="setup-result">
                <div className="setup-result-head">
                  <span className="setup-badge">
                    <CheckIcon className="button-icon" />
                  </span>
                  <h3>{action}完成</h3>
                </div>
                <p className="setup-target">
                  {uninstall ? info.directory : target}
                </p>
                {uninstall ? (
                  <p className="setup-note">
                    {removeUserData ? t.setup.dataDeleted : t.setup.dataKept}
                  </p>
                ) : null}
                {launchError ? (
                  <p className="setup-note">{launchError}</p>
                ) : null}
              </div>
            ) : null}

            {view === "failed" ? (
              <div className="setup-result">
                <div className="setup-result-head">
                  <span className="setup-badge">
                    <AlertIcon className="button-icon" />
                  </span>
                  <h3>{action}失败</h3>
                </div>
                <p>{state.error ?? t.setup.incomplete(action)}</p>
                <p className="setup-note">{t.setup.retryHint}</p>
              </div>
            ) : null}
          </div>

          <footer className="setup-foot">
            {view === "done" ? (
              <>
                <button
                  type="button"
                  className={uninstall ? "primary-action" : "subtle-action"}
                  onClick={() => void api.windowClose()}
                >
                  关闭
                </button>
                {uninstall ? null : (
                  <button
                    type="button"
                    className="primary-action"
                    onClick={() => void launch()}
                  >
                    启动
                  </button>
                )}
              </>
            ) : view === "failed" ? (
              <>
                <button
                  type="button"
                  className="subtle-action"
                  onClick={() => void api.windowClose()}
                >
                  关闭
                </button>
                <button
                  type="button"
                  className="primary-action"
                  onClick={() => setRetry(true)}
                >
                  返回重试
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="subtle-action"
                  disabled={busy}
                  onClick={() => void api.windowClose()}
                >
                  取消
                </button>
                <button
                  type="button"
                  className="primary-action"
                  disabled={busy || !ready || (!uninstall && !target)}
                  onClick={() => void start()}
                >
                  {action}
                </button>
              </>
            )}
          </footer>
        </main>
      </div>
    </div>
  );
}
