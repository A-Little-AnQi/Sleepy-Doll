import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { readError } from "../session";
import { TitleBar } from "../components/TitleBar";
import { AlertIcon, CheckIcon, FolderIcon } from "../components/icons";
import moonCharacter from "../assets/moon-character.webp";
import { setupApi, type SetupInfo, type SetupState } from "./api";
import "./setup.css";

/** 拿不到原生回复时就照这份默认值渲染，界面在任何时候都不会是空白。 */
/** 原生没应答时（浏览器预览、或原生侧起不来）用它渲染，避免白屏。默认目录取
 *  真实安装器的首选值，别用一个会被它拒绝的路径。 */
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

/** 原生侧会把产品目录名接到所选目录后面，这里算出同一个结果给用户看。 */
export function installedDirectory(chosen: string) {
  const base = chosen.trim().replace(/[\\/]+$/, "");
  if (!base) return "";
  return base.split(/[\\/]/).pop()?.toLowerCase() === "sleepy doll"
    ? base
    : `${base}\\Sleepy Doll`;
}

/** 进度可能是 0–1 的小数，也可能是 0–100 的百分数。 */
export function progressPercent(progress: number) {
  const ratio = progress > 1 ? progress / 100 : progress;
  return Math.round(Math.min(1, Math.max(0, ratio)) * 100);
}

/** 已安装就预填现有目录：覆盖安装不该换地方。 */
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
  /** 用户动过目录之后，迟到的 info 回复不再覆盖他的选择。 */
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
      // 对话框打不开就保持原值：路径本来也可以直接输入。
    }
  }

  const resting = view === "form" || view === "running";

  return (
    <div className="setup-shell">
      <TitleBar />
      <aside className="setup-aside">
        <img className="setup-art" src={moonCharacter} alt="" />
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
                ? "卸载会删掉程序文件；user\\ 目录是否一起删由你决定。"
                : info.installed
                  ? `已安装${info.installedVersion ? ` ${info.installedVersion}` : ""}，继续会覆盖程序文件，user\\ 目录里的配置和会话会保留。`
                  : "选好安装位置，点「安装」开始复制文件。"}
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
                <label className="setup-field-label" htmlFor="setup-directory">
                  安装位置
                </label>
                <div className="setup-path">
                  <input
                    id="setup-directory"
                    value={directory}
                    disabled={busy}
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
                    disabled={busy}
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
                  onChange={(event) => setRemoveUserData(event.target.checked)}
                />
                <span>
                  同时删除 user\ 目录（配置、模型密钥、会话数据库、日志）
                </span>
              </label>
              <p className="setup-note">
                不勾选会保留这个目录，重装后可以继续用；勾选后无法恢复。
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
              <span className="setup-badge">
                <CheckIcon className="button-icon" />
              </span>
              <h3>{action}完成</h3>
              <p className="setup-note">{uninstall ? "已删除" : "已安装到"}</p>
              <p className="setup-target">
                {uninstall ? info.directory : target}
              </p>
              <p className="setup-note">
                {uninstall
                  ? removeUserData
                    ? "user\\ 目录也已一并删除。"
                    : "user\\ 目录保留在安装目录下，重装后可以继续用。"
                  : shortcut
                    ? "桌面上的快捷方式可以直接启动。"
                    : "运行安装目录里的 sleepy-doll.exe 启动。"}
              </p>
            </div>
          ) : null}

          {view === "failed" ? (
            <div className="setup-result">
              <span className="setup-badge">
                <AlertIcon className="button-icon" />
              </span>
              <h3>{action}失败</h3>
              <p>{state.error ?? `${action}没有完成。`}</p>
              <p className="setup-note">点「返回重试」可以换个位置再来一次。</p>
            </div>
          ) : null}
        </div>

        <footer className="setup-foot">
          {view === "done" ? (
            <button
              type="button"
              className="primary-action"
              onClick={() => void api.windowClose()}
            >
              关闭
            </button>
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
                disabled={busy}
                onClick={() => void start()}
              >
                {action}
              </button>
            </>
          )}
        </footer>
      </main>
    </div>
  );
}
