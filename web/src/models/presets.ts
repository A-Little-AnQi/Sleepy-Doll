import type { LocaleId } from "../appearance/locale";

export type ModelAuthMode = "auto" | "apiKey" | "bearer";

export type ModelPreset = {
  id: string;
  name: string;
  /** 英文界面显示名；缺省时两种语言都用 name（多为拉丁品牌名）。 */
  nameEn?: string;
  protocol: string;
  baseUrl: string;
  model: string;
  auth: ModelAuthMode;
  modelsUrl?: string;
  needsKey: boolean;
  keyUrl?: string;
  platformUrl?: string;
  contextWindow: number;
  maxOutputTokens: number;
  timeoutMs: number;
};

export const DEFAULT_CONTEXT_WINDOW = 256_000;

const DEFAULTS = {
  contextWindow: DEFAULT_CONTEXT_WINDOW,
  maxOutputTokens: 8192,
  timeoutMs: 120_000,
};

export const MODEL_PRESETS: readonly ModelPreset[] = [
  {
    id: "deepseek",
    name: "DeepSeek",
    protocol: "anthropic-messages",
    baseUrl: "https://api.deepseek.com/anthropic/v1",
    model: "deepseek-chat",
    auth: "bearer",
    modelsUrl: "https://api.deepseek.com/models",
    needsKey: true,
    keyUrl: "https://platform.deepseek.com/api_keys",
    platformUrl: "https://platform.deepseek.com",
    contextWindow: 128_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    nameEn: "Zhipu GLM",
    protocol: "anthropic-messages",
    baseUrl: "https://open.bigmodel.cn/api/anthropic/v1",
    model: "glm-4.5",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys",
    platformUrl: "https://bigmodel.cn",
    contextWindow: 128_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "kimi",
    name: "Kimi",
    protocol: "anthropic-messages",
    baseUrl: "https://api.moonshot.cn/anthropic/v1",
    model: "kimi-k2.5",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://platform.kimi.com/console/api-keys",
    platformUrl: "https://platform.kimi.com",
    contextWindow: 256_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "bailian",
    name: "通义百炼",
    nameEn: "Alibaba Bailian",
    protocol: "anthropic-messages",
    baseUrl: "https://dashscope.aliyuncs.com/apps/anthropic/v1",
    model: "qwen-plus",
    auth: "bearer",
    needsKey: true,
    keyUrl:
      "https://bailian.console.aliyun.com/cn-beijing/model/settings/api-key",
    platformUrl: "https://bailian.console.aliyun.com/",
    contextWindow: 131_072,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "minimax",
    name: "MiniMax",
    protocol: "anthropic-messages",
    baseUrl: "https://api.minimaxi.com/anthropic/v1",
    model: "MiniMax-M2.5",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://platform.minimax.cn/console/access?tab=api-keys",
    platformUrl: "https://platform.minimax.cn",
    contextWindow: 1_000_000,
    maxOutputTokens: 8192,
    timeoutMs: 300_000,
  },
  {
    id: "siliconflow",
    name: "硅基流动",
    nameEn: "SiliconFlow",
    protocol: "anthropic-messages",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "deepseek-ai/DeepSeek-V3",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://cloud.siliconflow.cn/account/ak",
    platformUrl: "https://cloud.siliconflow.cn",
    contextWindow: 128_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    protocol: "anthropic-messages",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "anthropic/claude-sonnet-4.5",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://openrouter.ai/keys",
    platformUrl: "https://openrouter.ai",
    contextWindow: 200_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "openai",
    name: "OpenAI",
    protocol: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o",
    auth: "auto",
    needsKey: true,
    keyUrl: "https://platform.openai.com/api-keys",
    platformUrl: "https://platform.openai.com",
    contextWindow: 128_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "anthropic",
    name: "Claude 官方",
    nameEn: "Claude",
    protocol: "anthropic-messages",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-5",
    auth: "auto",
    needsKey: true,
    keyUrl: "https://platform.claude.com/settings/keys",
    platformUrl: "https://platform.claude.com",
    contextWindow: 200_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "gemini",
    name: "Gemini",
    protocol: "gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.0-flash",
    auth: "auto",
    needsKey: true,
    keyUrl: "https://aistudio.google.com/apikey",
    platformUrl: "https://aistudio.google.com",
    contextWindow: 1_000_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "ollama",
    name: "Ollama 本机",
    nameEn: "Ollama (local)",
    protocol: "ollama-chat",
    baseUrl: "http://127.0.0.1:11434",
    model: "",
    auth: "auto",
    platformUrl: "https://ollama.com",
    needsKey: false,
    contextWindow: 32_768,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "custom",
    name: "自定义",
    nameEn: "Custom",
    protocol: "openai-chat",
    baseUrl: "",
    model: "",
    auth: "auto",
    needsKey: true,
    contextWindow: DEFAULTS.contextWindow,
    maxOutputTokens: DEFAULTS.maxOutputTokens,
    timeoutMs: DEFAULTS.timeoutMs,
  },
];

export function normalizeEndpoint(url: string) {
  return url.trim().replace(/\/+$/, "");
}

export function suggestedContextWindow(
  model: string,
  reported: Record<string, number> = {},
  fallback = DEFAULT_CONTEXT_WINDOW,
) {
  const limit = reported[model];
  if (
    limit != null &&
    Number.isInteger(limit) &&
    limit >= 8192 &&
    limit <= 2_000_000
  )
    return limit;
  return (
    MODEL_PRESETS.find(
      (preset) => preset.model && preset.model === model.trim(),
    )?.contextWindow ?? fallback
  );
}

export function presetById(id: string) {
  if (!id) return undefined;
  return (
    MODEL_PRESETS.find((preset) => preset.id === id) ??
    MODEL_PRESETS[MODEL_PRESETS.length - 1]
  );
}

export function matchPreset(model: { protocol: string; baseUrl: string }) {
  const url = normalizeEndpoint(model.baseUrl);
  const hits = MODEL_PRESETS.filter(
    (preset) =>
      preset.id !== "custom" && normalizeEndpoint(preset.baseUrl) === url,
  );
  if (!hits.length) return "custom";
  return (
    hits.find((preset) => preset.protocol === model.protocol)?.id ?? hits[0]!.id
  );
}

/** 预设在指定语言下的显示名；只影响界面显示，不改写用户已存的模型名。 */
export function presetName(preset: ModelPreset, locale: LocaleId) {
  return locale === "en" && preset.nameEn ? preset.nameEn : preset.name;
}

export function presetLabel(
  model: { protocol: string; baseUrl: string },
  locale: LocaleId = "zh",
) {
  const preset = presetById(matchPreset(model));
  return !preset || preset.id === "custom"
    ? presetName(preset ?? MODEL_PRESETS[MODEL_PRESETS.length - 1]!, locale)
    : presetName(preset, locale);
}
