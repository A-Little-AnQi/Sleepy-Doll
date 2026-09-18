export type ModelAuthMode = "auto" | "apiKey" | "bearer";

export type ModelPreset = {
  id: string;
  name: string;
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

const DEFAULTS = {
  contextWindow: 200_000,
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
    protocol: "anthropic-messages",
    baseUrl: "https://open.bigmodel.cn/api/anthropic/v1",
    model: "glm-4.5",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://open.bigmodel.cn/usercenter/apikeys",
    platformUrl: "https://open.bigmodel.cn/dev",
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
    keyUrl: "https://platform.moonshot.cn/console/api-keys",
    platformUrl: "https://platform.moonshot.cn/docs",
    contextWindow: 256_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "bailian",
    name: "通义百炼",
    protocol: "anthropic-messages",
    baseUrl: "https://dashscope.aliyuncs.com/apps/anthropic/v1",
    model: "qwen-plus",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://bailian.console.aliyun.com/",
    platformUrl: "https://help.aliyun.com/zh/model-studio/",
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
    keyUrl:
      "https://platform.minimaxi.com/user-center/basic-information/interface-key",
    platformUrl: "https://platform.minimaxi.com/docs",
    contextWindow: 1_000_000,
    maxOutputTokens: 8192,
    timeoutMs: 300_000,
  },
  {
    id: "siliconflow",
    name: "硅基流动",
    protocol: "anthropic-messages",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "deepseek-ai/DeepSeek-V3",
    auth: "bearer",
    needsKey: true,
    keyUrl: "https://cloud.siliconflow.cn/account/ak",
    platformUrl: "https://docs.siliconflow.cn",
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
    platformUrl: "https://openrouter.ai/docs",
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
    platformUrl: "https://platform.openai.com/docs",
    contextWindow: 128_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "anthropic",
    name: "Claude 官方",
    protocol: "anthropic-messages",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-5",
    auth: "auto",
    needsKey: true,
    keyUrl: "https://console.anthropic.com/settings/keys",
    platformUrl: "https://docs.anthropic.com",
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
    platformUrl: "https://ai.google.dev/gemini-api/docs",
    contextWindow: 1_000_000,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "ollama",
    name: "Ollama 本机",
    protocol: "ollama-chat",
    baseUrl: "http://127.0.0.1:11434",
    model: "",
    auth: "auto",
    platformUrl: "https://ollama.com/docs",
    needsKey: false,
    contextWindow: 32_768,
    maxOutputTokens: 8192,
    timeoutMs: DEFAULTS.timeoutMs,
  },
  {
    id: "custom",
    name: "自定义",
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

export function presetLabel(model: { protocol: string; baseUrl: string }) {
  const preset = presetById(matchPreset(model));
  return !preset || preset.id === "custom" ? "自定义" : preset.name;
}
