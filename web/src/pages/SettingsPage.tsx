import { useEffect, useState } from "react";
import "./SettingsPage.css";
import type { Bootstrap } from "../types";
import { hostPluginEnabled } from "../providers";
import {
  BridgeIcon,
  ModelIcon,
  SettingsIcon,
  BrandIcon,
} from "../components/icons";
import { Select } from "../components/Select";
import { ThemeSwitch } from "../components/ThemeSwitch";
import { ModelsPage } from "./ModelsPage";
import { BridgePage } from "./BridgePage";
import { SettingRow } from "../components/SettingRow";
import { SlidingTabs } from "../components/SlidingTabs";
import { ConfigEditor } from "../components/ConfigEditor";
import { api } from "../api";
import { readTheme, writeTheme } from "../appearance";
import {
  LOCALE_OPTIONS,
  readLocale,
  writeLocale,
  type LocaleId,
} from "../locale";

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
  const [theme, setTheme] = useState(readTheme);
  const [locale, setLocale] = useState(readLocale);
  const [sendKey, setSendKey] = useState(
    () => localStorage.getItem("sleepy-doll-send-key") ?? "enter",
  );
  const [editing, setEditing] = useState(false);
  const [configPath, setConfigPath] = useState(bootstrap.configPath);
  useEffect(() => setConfigPath(bootstrap.configPath), [bootstrap.configPath]);
  useEffect(() => {
    if (bootstrap.configPath) return;
    void api
      .configRead()
      .then((result) => {
        if (result.path) setConfigPath(result.path);
      })
      .catch(() => undefined);
  }, [bootstrap.configPath]);
  return (
    <div className="settings-layout">
      <div className="settings-nav">
        <SlidingTabs
          ariaLabel="设置分类"
          value={section}
          onChange={onSection}
          items={[
            { id: "settings", name: "通用", icon: <SettingsIcon className="button-icon" /> },
            { id: "models", name: "模型", icon: <ModelIcon className="button-icon" /> },
            ...(hostPluginEnabled(bootstrap)
              ? [
                  {
                    id: "bridge" as const,
                    name: "BetterGI",
                    icon: <BridgeIcon className="button-icon" />,
                  },
                ]
              : []),
            { id: "sponsor", name: "赞助作者", icon: <BrandIcon className="button-icon" /> },
          ]}
        />
      </div>
      <div className="settings-content">
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
              <SettingRow label="主题">
                <ThemeSwitch
                  theme={theme}
                  onChange={(next, origin) => {
                    setTheme(next);
                    return writeTheme(next, origin);
                  }}
                />
              </SettingRow>
              <SettingRow label="语言">
                <Select
                  label="语言"
                  value={locale}
                  options={LOCALE_OPTIONS}
                  onChange={(value) => {
                    const next = value as LocaleId;
                    setLocale(next);
                    writeLocale(next);
                  }}
                />
              </SettingRow>
            </section>
            <section className="settings-group">
              <h3>对话</h3>
              <SettingRow label="发送快捷键">
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
              </SettingRow>
            </section>
            <section className="settings-group">
              <h3>配置</h3>
              <SettingRow label="配置文件" hint={configPath}>
                <button
                  className="subtle-action"
                  onClick={() => setEditing(true)}
                >
                  编辑
                </button>
              </SettingRow>
            </section>
            <div className="settings-about">
              <BrandIcon className="brand-mark" />
              <span>
                Sleepy Doll <span className="muted">0.1.0</span>
              </span>
            </div>
          </div>
        )}
      </div>
      <ConfigEditor
        path={configPath}
        open={editing}
        onClose={() => setEditing(false)}
        onSaved={reload}
        onPath={setConfigPath}
      />
    </div>
  );
}

function SponsorNote() {
  return (
    <aside className="settings-sponsor">
      <h2>赞助作者</h2>
      <p>如果这个工具对你有帮助，欢迎扫码支持。</p>
      <div
        className="settings-sponsor-qr"
        role="img"
        aria-label="收款二维码"
      />
    </aside>
  );
}
