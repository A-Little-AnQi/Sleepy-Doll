import { useEffect, useState } from "react";
import "./SettingsPage.css";
import type { Bootstrap } from "../../ipc/types";
import { hostPluginEnabled } from "../../ipc/providers";
import {
  BridgeIcon,
  HelpIcon,
  ModelIcon,
  SettingsIcon,
  BrandIcon,
} from "../../components/icons";
import { Select } from "../../components/controls/Select";
import { ThemeSwitch } from "../../components/controls/ThemeSwitch";
import { ModelsPage } from "./ModelsPage";
import { BridgePage } from "../bridge/BridgePage";
import { HelpPage, type HelpOpen } from "./HelpPage";
import { SettingRow } from "../../components/controls/SettingRow";
import { SlidingTabs } from "../../components/controls/SlidingTabs";
import { ConfigEditor } from "../../components/bridge/ConfigEditor";
import { api, framelessWindow } from "../../ipc/api";
import { useTheme, writeTheme } from "../../appearance";
import {
  LOCALE_OPTIONS,
  useLocale,
  writeLocale,
  type LocaleId,
} from "../../appearance/locale";
import { useT } from "../../i18n";

type Section = "settings" | "models" | "bridge" | "help" | "sponsor";

export function SettingsPage({
  bootstrap,
  section,
  onSection,
  reload,
}: {
  bootstrap: Bootstrap;
  section: Section;
  onSection(section: Section | HelpOpen): void;
  reload(): Promise<void>;
}) {
  const t = useT();
  const theme = useTheme();
  const locale = useLocale();
  const [sendKey, setSendKey] = useState(
    () => localStorage.getItem("sleepy-doll-send-key") ?? "enter",
  );
  // 托盘开关只在桌面壳里可用，浏览器预览下不显示这个分组。
  const [trayEnabled, setTrayEnabled] = useState<boolean | null>(null);
  useEffect(() => {
    if (!framelessWindow()) return;
    api
      .trayState()
      .then((result) => setTrayEnabled(result.enabled))
      .catch(() => undefined);
  }, []);
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
          ariaLabel={t.settings.categories}
          value={section}
          onChange={onSection}
          items={[
            {
              id: "settings",
              name: t.settings.general,
              icon: <SettingsIcon className="button-icon" />,
            },
            {
              id: "models",
              name: t.settings.models,
              icon: <ModelIcon className="button-icon" />,
            },
            ...(hostPluginEnabled(bootstrap)
              ? [
                  {
                    id: "bridge" as const,
                    name: t.settings.bettergi,
                    icon: <BridgeIcon className="button-icon" />,
                  },
                ]
              : []),
            {
              id: "help",
              name: t.settings.help,
              icon: <HelpIcon className="button-icon" />,
            },
            {
              id: "sponsor",
              name: t.settings.sponsor,
              icon: <BrandIcon className="button-icon" />,
            },
          ]}
        />
      </div>
      <div className="settings-content">
        {section === "models" ? (
          <ModelsPage bootstrap={bootstrap} reload={reload} />
        ) : section === "bridge" ? (
          <BridgePage bootstrap={bootstrap} reload={reload} />
        ) : section === "help" ? (
          <HelpPage onOpen={(target) => onSection(target)} />
        ) : section === "sponsor" ? (
          <SponsorNote />
        ) : (
          <div className="settings-general">
            <h2>{t.settings.general}</h2>
            <section className="settings-group">
              <SettingRow label={t.settings.theme}>
                <ThemeSwitch theme={theme} onChange={writeTheme} />
              </SettingRow>
              <SettingRow label={t.settings.language}>
                <Select
                  label={t.settings.language}
                  value={locale}
                  options={LOCALE_OPTIONS}
                  onChange={(value) => writeLocale(value as LocaleId)}
                />
              </SettingRow>
            </section>
            <section className="settings-group">
              <h3>{t.settings.dialogHeading}</h3>
              <SettingRow label={t.settings.sendKey}>
                <Select
                  label={t.settings.sendKey}
                  value={sendKey}
                  options={[
                    { value: "enter", label: t.settings.sendKeyEnter },
                    { value: "modifier", label: t.settings.sendKeyModifier },
                  ]}
                  onChange={(value) => {
                    setSendKey(value);
                    localStorage.setItem("sleepy-doll-send-key", value);
                  }}
                />
              </SettingRow>
            </section>
            {trayEnabled !== null ? (
              <section className="settings-group">
                <h3>{t.settings.tray}</h3>
                <SettingRow
                  label={t.settings.trayIcon}
                  hint={t.settings.trayIconHint}
                >
                  <Select
                    label={t.settings.trayIcon}
                    value={trayEnabled ? "show" : "hide"}
                    options={[
                      { value: "show", label: t.settings.trayShow },
                      { value: "hide", label: t.settings.trayHide },
                    ]}
                    onChange={(value) => {
                      const next = value === "show";
                      setTrayEnabled(next);
                      api
                        .traySetEnabled(next)
                        .catch(() => setTrayEnabled(!next));
                    }}
                  />
                </SettingRow>
              </section>
            ) : null}
            <section className="settings-group">
              <h3>{t.settings.configHeading}</h3>
              <SettingRow label={t.settings.configFile} hint={configPath}>
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
  const t = useT();
  return (
    <aside className="settings-sponsor">
      <h2>{t.settings.sponsor}</h2>
      <p>{t.settings.sponsorNote}</p>
      <div className="settings-sponsor-qr" role="img" aria-label={t.settings.qrLabel} />
    </aside>
  );
}
