import bgiFixture from "../fixtures/bettergi.json";
import type { BgiState, Box } from "../scenes";
import "./bgi.css";

/**
 * BetterGI 主窗口的舞台复刻：按 better-genshin-impact 真实布局重排
 * （WPF-UI Fluent 暗色、左侧 NavigationView、调度器页配置组 + 任务表 + 启动）。
 * 文案全部取自 BetterGI 真实界面。正式成片此块应替换为真实录屏。
 */
export function BetterGiWindow({ box, state }: { box: Box; state: BgiState }) {
  const scheduler = bgiFixture.scheduler;
  const windowScale = box.w / 1280;
  const logs = [
    ...scheduler.idleLog,
    ...scheduler.runningLog.slice(0, Math.max(0, state.logCount - 1)),
  ].slice(-5);

  return (
    <div
      className="bgi-window"
      style={{
        left: box.x,
        top: box.y,
        width: 1280,
        height: box.h / windowScale,
        transform: `scale(${windowScale})`,
        transformOrigin: "0 0",
        opacity: state.brightness,
      }}
    >
      <div className="bgi-titlebar">
        <span className="bgi-logo" aria-hidden />
        <span className="bgi-title">{bgiFixture.windowTitle}</span>
        <div className="bgi-titlebar-actions">
          <button type="button" tabIndex={-1}>
            <svg viewBox="0 0 10 10">
              <rect x="1" y="4.6" width="8" height="0.9" />
            </svg>
          </button>
          <button type="button" tabIndex={-1}>
            <svg viewBox="0 0 10 10">
              <rect
                x="1.2"
                y="1.2"
                width="7.6"
                height="7.6"
                fill="none"
                strokeWidth="0.9"
              />
            </svg>
          </button>
          <button type="button" className="bgi-close" tabIndex={-1}>
            <svg viewBox="0 0 10 10">
              <path d="M1.5 1.5 L8.5 8.5 M8.5 1.5 L1.5 8.5" strokeWidth="0.9" />
            </svg>
          </button>
        </div>
      </div>

      <div className="bgi-body">
        <nav className="bgi-nav">
          {bgiFixture.navGroups.map((group, gi) => (
            <div className="bgi-nav-group" key={gi}>
              {group.label && (
                <div className="bgi-nav-group-label">{group.label}</div>
              )}
              {group.items.map((item) => (
                <div
                  key={item.id}
                  className={`bgi-nav-item${"selected" in item && item.selected ? " is-selected" : ""}${
                    group.label ? " is-child" : ""
                  }`}
                >
                  <NavIcon id={item.icon} />
                  <span>{item.label}</span>
                  {item.id === "home" && <span className="bgi-nav-dot" />}
                </div>
              ))}
            </div>
          ))}
          <div className="bgi-nav-spacer" />
          {bgiFixture.navFooter.map((item) => (
            <div key={item.id} className="bgi-nav-item">
              <NavIcon id={item.icon} />
              <span>{item.label}</span>
            </div>
          ))}
        </nav>

        <main className="bgi-main">
          <aside className="bgi-groups">
            <div className="bgi-groups-label">{scheduler.groupLabel}</div>
            {scheduler.groups.map((name) => (
              <div
                key={name}
                className={`bgi-group${name === scheduler.activeGroup ? " is-active" : ""}`}
              >
                <span className="bgi-group-grip" aria-hidden />
                {name}
              </div>
            ))}
          </aside>

          <section className="bgi-content">
            <header className="bgi-content-head">
              <h2>
                配置组 -{" "}
                <span className="bgi-accent-text">{scheduler.activeGroup}</span>
              </h2>
              <div className="bgi-toolbar">
                <span className="bgi-tool-btn is-dropdown">
                  {scheduler.addTaskLabel}
                  <svg viewBox="0 0 10 10" className="bgi-caret">
                    <path
                      d="M2 3.8 L5 6.8 L8 3.8"
                      fill="none"
                      strokeWidth="1"
                    />
                  </svg>
                </span>
                <span className="bgi-tool-btn">
                  {scheduler.groupSettingsLabel}
                </span>
                <span
                  className={`bgi-tool-btn bgi-run-btn${state.running ? " is-running" : ""}`}
                  data-video-anchor="bgi-start"
                >
                  <svg viewBox="0 0 10 10" className="bgi-run-icon">
                    {state.running ? (
                      <rect x="2" y="2" width="6" height="6" />
                    ) : (
                      <path d="M2.5 1.5 L8.5 5 L2.5 8.5 Z" />
                    )}
                  </svg>
                  {state.running ? scheduler.stopLabel : scheduler.startLabel}
                </span>
              </div>
            </header>

            <div className="bgi-table" role="table">
              <div className="bgi-table-head" role="row">
                {scheduler.columns.map((col, i) => (
                  <span
                    key={col}
                    style={{ width: colWidth(i, box.w) }}
                    role="columnheader"
                  >
                    {col}
                  </span>
                ))}
              </div>
              {scheduler.rows.map((row, index) => (
                <div
                  key={row.name}
                  role="row"
                  className={`bgi-table-row${index === 0 && state.rowSelected ? " is-selected" : ""}`}
                  {...(index === 0
                    ? { "data-video-anchor": "bgi-route1" }
                    : {})}
                >
                  <span style={{ width: colWidth(0, box.w) }}>{index + 1}</span>
                  <span
                    style={{ width: colWidth(1, box.w) }}
                    className="bgi-cell-name"
                  >
                    {row.name}
                  </span>
                  <span style={{ width: colWidth(2, box.w) }}>{row.type}</span>
                  <span style={{ width: colWidth(3, box.w) }}>
                    <StatusCell
                      status={index === 0 ? state.row1Status : "待运行"}
                    />
                  </span>
                  <span style={{ width: colWidth(4, box.w) }}>
                    {row.lastRun}
                  </span>
                </div>
              ))}
            </div>

            <div className="bgi-log">
              {logs.map((line, index) => (
                <div className="bgi-log-line" key={`${line}-${index}`}>
                  {line}
                </div>
              ))}
            </div>
          </section>
        </main>
      </div>

      <footer className="bgi-status">
        <span
          className={`bgi-status-dot${state.running ? " is-running" : ""}`}
        />
        {state.statusText}
        <span className="bgi-status-right">v0.45</span>
      </footer>
    </div>
  );
}

function colWidth(index: number, _boxW: number) {
  // 相对列宽随窗口等比伸缩，与 WPF DataGrid 的星号布局一致。
  const widths = [0.06, 0.36, 0.17, 0.17, 0.24];
  return `${widths[index]! * 100}%`;
}

function StatusCell({ status }: { status: string }) {
  if (status === "运行中") {
    return (
      <span className="bgi-status-chip is-running">
        <span className="bgi-mini-spin" />
        运行中
      </span>
    );
  }
  if (status === "已完成") {
    return (
      <span className="bgi-status-chip is-done">
        <svg viewBox="0 0 10 10">
          <path d="M1.8 5.2 L4.2 7.6 L8.2 2.6" fill="none" strokeWidth="1.2" />
        </svg>
        已完成
      </span>
    );
  }
  return <span className="bgi-status-chip">{status}</span>;
}

function NavIcon({ id }: { id: string }) {
  return (
    <svg viewBox="0 0 16 16" className="bgi-nav-icon" aria-hidden>
      {id === "play" && <path d="M4 2.5 L13 8 L4 13.5 Z" />}
      {id === "timer" && (
        <>
          <circle cx="8" cy="9" r="5" fill="none" strokeWidth="1.3" />
          <path d="M8 9 L8 5.6 M6 1.6 L10 1.6" fill="none" strokeWidth="1.3" />
        </>
      )}
      {id === "list" && (
        <path
          d="M2.5 4h11M2.5 8h11M2.5 12h7"
          fill="none"
          strokeWidth="1.3"
          strokeLinecap="round"
        />
      )}
      {id === "turtle" && (
        <>
          <ellipse
            cx="8"
            cy="9.4"
            rx="5.4"
            ry="3.6"
            fill="none"
            strokeWidth="1.3"
          />
          <path
            d="M13.4 6.4 C14.8 5.4 15.2 7.6 13.6 8"
            fill="none"
            strokeWidth="1.3"
          />
        </>
      )}
      {id === "board" && (
        <>
          <rect
            x="2.4"
            y="2.4"
            width="11.2"
            height="11.2"
            rx="1.4"
            fill="none"
            strokeWidth="1.3"
          />
          <path d="M5.4 7.4 h5.4 M5.4 10.4 h3" fill="none" strokeWidth="1.3" />
        </>
      )}
      {id === "js" && (
        <path
          d="M5 3 L5 10 Q5 12 3 12 M11 4.4 Q10 3.4 8.8 3.8 Q7.6 4.2 7.8 5.6 Q8 6.8 9.8 7.2 Q11.6 7.6 11.6 9 Q11.6 11 9.8 11.6 Q8 12.2 6.9 11"
          fill="none"
          strokeWidth="1.2"
        />
      )}
      {id === "map" && (
        <>
          <path
            d="M2.6 4.2 L6.2 2.6 L9.8 4.2 L13.4 2.6 L13.4 11.8 L9.8 13.4 L6.2 11.8 L2.6 13.4 Z"
            fill="none"
            strokeWidth="1.2"
          />
          <path
            d="M6.2 2.6 L6.2 11.8 M9.8 4.2 L9.8 13.4"
            fill="none"
            strokeWidth="1.2"
          />
        </>
      )}
      {id === "record" && (
        <>
          <circle cx="8" cy="8" r="5.4" fill="none" strokeWidth="1.3" />
          <circle cx="8" cy="8" r="2.2" />
        </>
      )}
      {id === "pad" && (
        <path
          d="M4.4 3.4 C3 4.8 2.8 8.8 4.6 11.2 C6 13 10 13.2 11.6 11.4 C13 9.8 12.4 7 11 6.4 L8.4 5.2 L7.4 3.2 C6.6 2.2 5.2 2.6 4.4 3.4 Z M9 7.6 L9 7.6"
          fill="none"
          strokeWidth="1.2"
        />
      )}
      {id === "music" && (
        <>
          <circle cx="5.6" cy="11.6" r="2.4" fill="none" strokeWidth="1.3" />
          <circle cx="12" cy="10.4" r="2.2" fill="none" strokeWidth="1.3" />
          <path
            d="M8 11.6 L8 3.4 L14.2 2.4 L14.2 10.4"
            fill="none"
            strokeWidth="1.3"
          />
        </>
      )}
      {id === "flash" && (
        <path d="M9.4 1.6 L4 9.4 L7.6 9.4 L6.6 14.4 L12 6.6 L8.4 6.6 Z" />
      )}
      {id === "alert" && (
        <>
          <path
            d="M8 2.2 L14.6 13.4 L1.4 13.4 Z"
            fill="none"
            strokeWidth="1.3"
            strokeLinejoin="round"
          />
          <path
            d="M8 6.4 L8 9.4 M8 11.2 L8 11.6"
            fill="none"
            strokeWidth="1.3"
            strokeLinecap="round"
          />
        </>
      )}
      {id === "gear" && (
        <>
          <circle cx="8" cy="8" r="2.4" fill="none" strokeWidth="1.3" />
          <path
            d="M8 1.8 L8 3.6 M8 12.4 L8 14.2 M1.8 8 L3.6 8 M12.4 8 L14.2 8 M3.7 3.7 L5 5 M11 11 L12.3 12.3 M12.3 3.7 L11 5 M5 11 L3.7 12.3"
            fill="none"
            strokeWidth="1.3"
            strokeLinecap="round"
          />
        </>
      )}
    </svg>
  );
}
