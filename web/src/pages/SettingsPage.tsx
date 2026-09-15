import { useEffect, useState } from "react";
import "./SettingsPage.css";
import type { Bootstrap } from "../types";
import {
  BridgeIcon,
  ModelIcon,
  SettingsIcon,
  BrandIcon,
} from "../components/icons";
import { Select } from "../components/Select";
import { ModelsPage } from "./ModelsPage";
import { BridgePage } from "./BridgePage";
import { MotionSwitch } from "../components/MotionSwitch";
import {
  readReducedMotion,
  readTheme,
  writeReducedMotion,
  writeTheme,
} from "../appearance";
type Section = "settings" | "models" | "bridge" | "sponsor";
export function SettingsPage({
  bootstrap,
  section,
  onSection,
  reload,
}: {
  bootstrap: Bootstrap;
  section: Section;
  onSection(section: Section): void;
  reload(): Promise<void>;
}) {
  const [motion, setMotion] = useState(readReducedMotion);
  const [theme, setTheme] = useState(readTheme);
  const [sendKey, setSendKey] = useState(
    () => localStorage.getItem("sleepy-doll-send-key") ?? "enter",
  );
  useEffect(() => {
    writeReducedMotion(motion);
  }, [motion]);
  useEffect(() => {
    writeTheme(theme);
  }, [theme]);
  const [copied, setCopied] = useState(false);
  return (
    <div className="settings-layout">
      <nav className="settings-nav" aria-label="设置分类">
        {(
          [
            { id: "settings", name: "通用", Icon: SettingsIcon },
            { id: "models", name: "模型", Icon: ModelIcon },
            { id: "bridge", name: "BetterGI", Icon: BridgeIcon },
            { id: "sponsor", name: "赞助作者", Icon: BrandIcon },
          ] as const
        ).map(({ id, name, Icon }) => (
          <button
            key={id}
            className={section === id ? "is-active" : ""}
            aria-current={section === id ? "page" : undefined}
            onClick={() => onSection(id)}
          >
            <Icon className="button-icon" />
            {name}
          </button>
        ))}
      </nav>
      <div className="settings-content">
        <MotionSwitch viewKey={section} kind="panel">
        {section === "models" ? (
          <ModelsPage bootstrap={bootstrap} reload={reload} />
        ) : section === "bridge" ? (
          <BridgePage bootstrap={bootstrap} reload={reload} />
        ) : section === "sponsor" ? (
          <SponsorNote />
        ) : (
          <div className="settings-general">
            <h2>通用</h2>
            <section className="settings-group">
              <h3>外观</h3>
              <div className="setting-row">
                <strong>主题</strong>
                <Select
                  label="主题"
                  value={theme}
                  options={[
                    { value: "light", label: "浅色" },
                    { value: "dark", label: "深色" },
                  ]}
                  onChange={(value) =>
                    setTheme(value === "dark" ? "dark" : "light")
                  }
                />
              </div>
              <div className="setting-row">
                <strong>减少动态效果</strong>
                <button
                  className={`switch ${motion ? "on" : ""}`}
                  role="switch"
                  aria-label="减少动态效果"
                  aria-checked={motion}
                  onClick={() => setMotion(!motion)}
                />
              </div>
            </section>
            <section className="settings-group">
              <h3>对话</h3>
              <div className="setting-row">
                <strong>发送快捷键</strong>
                <Select
                  label="发送快捷键"
                  value={sendKey}
                  options={[
                    { value: "enter", label: "Enter" },
                    { value: "modifier", label: "Ctrl + Enter" },
                  ]}
                  onChange={(value) => {
                    setSendKey(value);
                    localStorage.setItem("sleepy-doll-send-key", value);
                  }}
                />
              </div>
            </section>
            <section className="settings-group">
              <h3>本地数据</h3>
              <div className="setting-row">
                <div>
                  <strong>配置文件</strong>
                  <p className="path-value">{bootstrap.configPath}</p>
                </div>
                <button
                  className="subtle-action"
                  onClick={() =>
                    void navigator.clipboard
                      .writeText(bootstrap.configPath)
                      .then(() => {
                        setCopied(true);
                        setTimeout(() => setCopied(false), 2000);
                      })
                  }
                >
                  {copied ? "已复制" : "复制路径"}
                </button>
              </div>
            </section>
            <div className="settings-about">
              <BrandIcon className="brand-mark" />
              <span>
                Sleepy Doll <span className="muted">0.1.0</span>
              </span>
            </div>
          </div>
        )}
        </MotionSwitch>
      </div>
    </div>
  );
}

function SponsorNote() {
  return (
    <aside className="settings-sponsor">
      <h2>赞助作者</h2>
      <p>
        业余时间做的小工具。如果用得顺手，扫一张收款码请我喝杯咖啡就好。
      </p>
      <div
        className="settings-sponsor-qr"
        role="img"
        aria-label="收款二维码"
      />
    </aside>
  );
}
