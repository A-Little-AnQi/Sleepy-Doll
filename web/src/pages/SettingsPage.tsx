import { useEffect, useState } from "react";

import type { Bootstrap } from "../types";

export function SettingsPage({ bootstrap }: { bootstrap: Bootstrap }) {
  const [reducedMotion, setReducedMotion] = useState(
    () => localStorage.getItem("sleepy-doll-reduced-motion") === "true",
  );

  useEffect(() => {
    document.documentElement.dataset.reducedMotion = String(reducedMotion);
    localStorage.setItem("sleepy-doll-reduced-motion", String(reducedMotion));
  }, [reducedMotion]);

  return (
    <div className="page-sheet">
      <section className="page-block">
        <h2>界面</h2>
        <div className="setting-row">
          <span>
            <strong>减少动态效果</strong>
            <small>关掉界面里的浮动和过渡动画。</small>
          </span>
          <button
            type="button"
            className={`switch ${reducedMotion ? "on" : ""}`}
            role="switch"
            aria-checked={reducedMotion}
            aria-label="减少动态效果"
            onClick={() => setReducedMotion((value) => !value)}
          >
            <span />
          </button>
        </div>
      </section>

      <section className="page-block">
        <h2>文件位置</h2>
        <p className="muted">
          Sleepy Doll
          的配置、模型密钥、对话记录都放在这一个文件夹里，不会写到别处。
          备份或迁移时整个拷走即可。
        </p>
        <dl className="path-list">
          <div>
            <dt>配置文件</dt>
            <dd>
              <code>{bootstrap.configPath}</code>
            </dd>
          </div>
        </dl>
        <p className="muted">
          模型密钥以明文保存在这个文件里。如果这台电脑有别的账户，请把这个文件夹的访问权限
          限制为只有你自己。
        </p>
      </section>
    </div>
  );
}
