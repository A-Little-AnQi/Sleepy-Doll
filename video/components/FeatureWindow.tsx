import { ModelsPage } from "../../web/src/pages/settings/ModelsPage";
import { ExtensionsPage } from "../../web/src/pages/extensions/ExtensionsPage";
import { BridgePage } from "../../web/src/pages/bridge/BridgePage";
import { MODEL_PRESETS } from "../../web/src/models/presets";
import { api } from "../../web/src/ipc/api";
import { bootstrapAt, filmAt } from "../film";
import { ProductWindow } from "./ProductWindow";
import { ease, seg } from "../clock";
import recording from "../recording.json";
import { cueTime } from "../sync";
import type { CSSProperties } from "react";

const reload = async () => undefined;
const noop = () => undefined;
// 演示页读取本地 fixture，不向运行中的宿主扫描扩展。
api.reloadExtensions = reload;

export function FeatureWindow({ time }: { time: number }) {
  const feature = recording.features.find(
    (item) => time >= item.start && time < item.end,
  );
  if (!feature) return null;
  const local = time - feature.start;
  const bootstrap = bootstrapAt(15000);
  const selected = local < 3500 ? "openai" : "ollama";
  bootstrap.models = ["openai", "anthropic", "gemini", "ollama", "custom"].map(
    (id) => {
      const preset = MODEL_PRESETS.find((p) => p.id === id)!;
      return {
        id: preset.id,
        name: preset.name,
        protocol: preset.protocol,
        model: preset.model,
        baseUrl: preset.baseUrl,
        active: id === selected,
        contextWindow: preset.contextWindow,
        timeoutMs: preset.timeoutMs,
        maxOutputTokens: preset.maxOutputTokens,
        auth: preset.auth,
        promptCache: true,
      };
    },
  );
  bootstrap.plugins = bootstrap.plugins.map((plugin) => ({
    ...plugin,
    status: "enabled",
    manifest: {
      ...plugin.manifest,
      description: "连接 BetterGI，提供任务执行、状态检查和结果核验工具。",
    },
  }));
  const page =
    feature.page === "models"
      ? "models"
      : feature.page === "extensions"
        ? "extensions"
        : "bridge";
  const content =
    page === "models" ? (
      <ModelsPage key={selected} bootstrap={bootstrap} reload={reload} />
    ) : page === "extensions" ? (
      <ExtensionsPage
        bootstrap={bootstrap}
        reload={reload}
        tab="plugins"
        onTab={noop}
      />
    ) : (
      <BridgePage bootstrap={bootstrap} reload={reload} />
    );
  const enter = ease.camera(seg(local, 0, 520));
  const reveal = ease.camera(seg(time, cueTime(page === "models" ? "models" : page === "extensions" ? "plugins" : "bridge"),
    cueTime(page === "models" ? "models" : page === "extensions" ? "plugins" : "bridge") + 420));
  return (
    <div
      className="feature-window"
      data-feature={page}
      style={{
        opacity: enter,
        transform: page === "models" ? `translateX(${(1 - enter) * 110}px)` : "none",
        clipPath: page === "extensions" ? `inset(0 0 ${(1 - enter) * 100}% 0)` : "none",
        "--feature-reveal": reveal,
      } as CSSProperties}
    >
      <ProductWindow
        t={15000}
        box={{ x: 300, y: 130, w: 1320, h: 880 }}
        viewport={{ width: 1200, height: 800 }}
        opacity={1}
        brandFade={0}
        blur={0}
        film={filmAt(15000)}
        presses={{}}
        presentation={{ page, bootstrap, content }}
      />
    </div>
  );
}
