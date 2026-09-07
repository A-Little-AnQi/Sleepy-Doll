import { useEffect, useState } from "react";

function MoonPhaseRail() {
  return (
    <svg className="settings-moon-rail" viewBox="0 0 620 180" aria-hidden="true">
      <path d="M54 94C168 18 356 20 566 94" />
      <circle cx="84" cy="78" r="26" />
      <path d="M88 55a21 21 0 0 0 18 31 25 25 0 1 1-18-31Z" />
      <circle cx="310" cy="40" r="30" />
      <path d="M310 10a30 30 0 0 1 0 60c14-12 14-48 0-60Z" />
      <circle cx="536" cy="78" r="26" />
      <path d="M532 55a21 21 0 0 1-18 31 25 25 0 1 0 18-31Z" />
      <path className="rail-star" d="m310 98 6 17 17 6-17 6-6 17-6-17-17-6 17-6Z" />
    </svg>
  );
}

export function SettingsPage() {
  const [reducedMotion, setReducedMotion] = useState(
    () => localStorage.getItem("sleepy-doll-reduced-motion") === "true",
  );

  useEffect(() => {
    document.documentElement.dataset.reducedMotion = String(reducedMotion);
    localStorage.setItem(
      "sleepy-doll-reduced-motion",
      String(reducedMotion),
    );
  }, [reducedMotion]);

  return (
    <section className="settings-workspace">
      <MoonPhaseRail />
      <header>
        <span>SETTINGS</span>
        <h1>设置</h1>
        <p>只保留会影响日常使用的界面偏好。</p>
      </header>
      <div className="settings-sheet">
        <div className="setting-row">
          <span>
            <strong>减少动态效果</strong>
            <small>停用首页人物浮动、Bridge 光流和按钮动画</small>
          </span>
          <button
            className={`switch ${reducedMotion ? "on" : ""}`}
            role="switch"
            aria-checked={reducedMotion}
            aria-label="减少动态效果"
            onClick={() => setReducedMotion((value) => !value)}
          >
            <span />
          </button>
        </div>
      </div>
    </section>
  );
}
