import { useEffect, useRef, useState } from "react";
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
  const [info, setInfo] = useState(PREVIEW_INFO);
  const [directory, setDirectory] = useState(PREVIEW_INFO.defaultDirectory);
  const [shortcut, setShortcut] = useState(true);
  const [removeUserData, setRemoveUserData] = useState(false);
  const [state, setState] = useState<SetupState>(IDLE);
  const [retry, setRetry] = useState(false);
  const [launchError, setLaunchError] = useState("");
  /** 用户动过目录之后，迟到的 info 回复不再覆盖。 */
  const touched = useRef(false);

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
        if (touched.current) return;
        setInfo(next);
        setDirectory(presetDirectory(next));
      })
      .catch(() => undefined);
  }, []);

  const uninstall = info.uninstallMode;
  const action = uninstall ? "卸载" : "安装";
  const target = installedDirectory(directory);
  const view = retry || state.phase === "idle" ? "form" : state.phase;
  const busy = view === "running";
  const percent = progressPercent(state.progress);

  async function start() {
    touched.current = true;
    setRetry(false);
    setState({
      phase: "running",
      progress: 0,
      message: "正在准备…",
      error: null,
    });
    try {
      if (uninstall) await setupApi.uninstall(removeUserData);
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
      touched.current = true;
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
        setLaunchError("没有启动，请从安装目录运行 sleepy-doll.exe。");
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
                  ? "卸载会删除程序文件，数据默认保留。"
                  : "程序与数据都装在所选目录下。"}
              </p>
            ) : null}
          </header>

          <div className="setup-body">
            {resting ? (
              uninstall ? (
                <dl className="setup-meta">
                  <div className="setup-meta-row">
                    <dt>安装位置</dt>
                    <dd className="setup-mono">{info.directory}</dd>
                  </div>
                  <div className="setup-meta-row">
                    <dt>版本</dt>
                    <dd>{info.installedVersion ?? "未知"}</dd>
                  </div>
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
                      disabled={busy || info.installed}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(event) => {
                        touched.current = true;
                        setDirectory(event.target.value);
                      }}
                    />
                    <button
                      type="button"
                      className="secondary-action"
                      disabled={busy || info.installed}
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
                    disabled={busy}
                    onChange={(event) =>
                      setRemoveUserData(event.target.checked)
                    }
                  />
                  <span>同时删除数据（配置、模型密钥、会话记录、日志）</span>
                </label>
                <p className="setup-note">
                  不勾选则保留，删除后无法恢复。
                </p>
              </div>
            ) : null}

            {resting && !uninstall ? (
              <label className="setup-check">
                <input
                  type="checkbox"
                  checked={shortcut}
                  disabled={busy}
                  onChange={(event) => setShortcut(event.target.checked)}
                />
                <span>创建桌面快捷方式</span>
              </label>
            ) : null}

            {busy ? (
              <div className="setup-progress" role="status">
                <div className="setup-progress-head">
                  <span>{state.message || `${action}中…`}</span>
                  <span>{percent}%</span>
                </div>
                <div
                  className="setup-bar"
                  role="progressbar"
                  aria-label={`${action}进度`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                >
                  <div
                    className="setup-bar-fill"
                    style={{ width: `${percent}%` }}
                  />
                </div>
                <p className="setup-note">过程中请不要关闭窗口。</p>
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
                    {removeUserData ? "数据已删除。" : "数据已保留。"}
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
                <p>{state.error ?? `${action}没有完成。`}</p>
                <p className="setup-note">可以点「返回重试」换个位置。</p>
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
                  disabled={busy || (!uninstall && !target)}
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
