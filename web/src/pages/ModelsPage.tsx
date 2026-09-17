import { useRef, useState } from "react";
import { api } from "../api";
import { readError } from "../session";
import { Toast } from "../components/Toast";
import { PlusIcon, TrashIcon } from "../components/icons";
import { Select } from "../components/Select";
import { MotionSwitch } from "../components/MotionSwitch";
import { DisclosureChevron } from "../components/DisclosureChevron";
import type { Bootstrap, ModelInfo } from "../types";
import {
  MODEL_PRESETS,
  matchPreset,
  normalizeEndpoint,
  presetById,
  presetLabel,
  type ModelAuthMode,
  type ModelPreset,
} from "../model-presets";
import "./ModelsPage.css";

const protocols = [
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "openai-chat", label: "OpenAI 兼容" },
  { value: "anthropic-messages", label: "Anthropic" },
  { value: "gemini", label: "Google Gemini" },
  { value: "ollama-chat", label: "Ollama" },
];

const authModes = [
  { value: "auto", label: "自动" },
  { value: "apiKey", label: "API Key" },
  { value: "bearer", label: "Bearer" },
];

const presetOptions = MODEL_PRESETS.map((preset) => ({
  value: preset.id,
  label: preset.name,
}));

type ModelForm = {
  id: string;
  preset: string;
  name: string;
  protocol: string;
  model: string;
  baseUrl: string;
  apiKey: string;
  timeoutMs: number;
  contextWindow: number;
  maxOutputTokens: number;
  auth: ModelAuthMode;
  promptCache: boolean;
};

function emptyForm(id = `model-${Date.now()}`): ModelForm {
  return {
    id,
    preset: "",
    name: "",
    protocol: "",
    model: "",
    baseUrl: "",
    apiKey: "",
    timeoutMs: 120000,
    contextWindow: 200_000,
    maxOutputTokens: 8192,
    auth: "auto",
    promptCache: true,
  };
}

function fromPreset(preset: ModelPreset, id = `model-${Date.now()}`): ModelForm {
  return {
    id,
    preset: preset.id,
    name: preset.id === "custom" ? "" : preset.name,
    protocol: preset.protocol,
    model: "",
    baseUrl: preset.baseUrl,
    apiKey: "",
    timeoutMs: preset.timeoutMs,
    contextWindow: preset.contextWindow,
    maxOutputTokens: preset.maxOutputTokens,
    auth: preset.auth,
    promptCache: true,
  };
}

function formFor(model?: ModelInfo): ModelForm {
  if (!model) return emptyForm();
  return {
    id: model.id,
    preset: matchPreset(model),
    name: model.name,
    protocol: model.protocol,
    model: model.model,
    baseUrl: model.baseUrl,
    apiKey: "",
    timeoutMs: model.timeoutMs ?? 120000,
    contextWindow: model.contextWindow ?? 200_000,
    maxOutputTokens: model.maxOutputTokens ?? 8192,
    auth: model.auth ?? "auto",
    promptCache: model.promptCache !== false,
  };
}

export function ModelsPage({
  bootstrap,
  reload,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}) {
  const [selectedId, setSelectedId] = useState(
    () =>
      bootstrap.models.find((model) => model.active)?.id ??
      bootstrap.models[0]?.id ??
      "new",
  );
  const selected = bootstrap.models.find((model) => model.id === selectedId);
  const [form, setForm] = useState<ModelForm>(() => formFor(selected));
  const drafts = useRef(new Map<string, ModelForm>());
  const [catalog, setCatalog] = useState<string[]>([]);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [listing, setListing] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const creating = selectedId === "new";
  const preset = presetById(form.preset);
  const needsKey = preset?.needsKey !== false;
  const dirty =
    creating ||
    Boolean(form.apiKey) ||
    (selected != null &&
      (form.name !== selected.name ||
        form.protocol !== selected.protocol ||
        form.model !== selected.model ||
        form.baseUrl !== selected.baseUrl ||
        form.timeoutMs !== (selected.timeoutMs ?? 120000) ||
        form.contextWindow !== (selected.contextWindow ?? 200_000) ||
        form.maxOutputTokens !== (selected.maxOutputTokens ?? 8192) ||
        form.auth !== (selected.auth ?? "auto") ||
        form.promptCache !== (selected.promptCache !== false)));
  const resetEditor = (id: string, next: ModelForm) => {
    setSelectedId(id);
    setForm(next);
    setCatalog([]);
    setAdvanced(false);
    setError("");
    setNotice("");
  };
  const choose = (id: string) => {
    drafts.current.set(selectedId, form);
    const model = bootstrap.models.find((item) => item.id === id);
    resetEditor(id, drafts.current.get(id) ?? formFor(model));
  };
  const add = () => {
    if (creating) {
      const fresh = formFor();
      drafts.current.set("new", fresh);
      resetEditor("new", fresh);
      return;
    }
    choose("new");
  };
  const updateForm = (next: (draft: ModelForm) => ModelForm) =>
    setForm((current) => {
      const updated = next(current);
      drafts.current.set(selectedId, updated);
      return updated;
    });
  const applyPreset = (presetId: string) => {
    const next = presetById(presetId);
    setCatalog([]);
    if (!next) {
      updateForm((draft) => ({ ...emptyForm(draft.id), apiKey: draft.apiKey }));
      return;
    }
    updateForm((draft) => ({
      ...fromPreset(next, draft.id),
      apiKey: draft.apiKey,
    }));
  };
  const change = (
    key: "name" | "protocol" | "model" | "baseUrl" | "apiKey",
    value: string,
  ) => updateForm((draft) => ({ ...draft, [key]: value }));
  const fetchModels = async () => {
    setListing(true);
    setError("");
    try {
      const matched =
        preset != null &&
        normalizeEndpoint(form.baseUrl) === normalizeEndpoint(preset.baseUrl);
      const result = await api.listModels({
        ...(creating ? {} : { id: form.id }),
        protocol: form.protocol,
        baseUrl: form.baseUrl,
        apiKey: form.apiKey,
        auth: form.auth,
        ...(matched && preset.modelsUrl ? { modelsUrl: preset.modelsUrl } : {}),
      });
      const models = result.models.filter(Boolean);
      setCatalog(models);
      if (!models.length) {
        setNotice("没有可用模型，请手动填写。");
        return;
      }
      if (!form.model || !models.includes(form.model)) {
        change("model", models[0]!);
      }
      setNotice(`已获取 ${models.length} 个模型`);
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setListing(false);
    }
  };
  const save = async () => {
    if (!form.name.trim() || !form.protocol || !form.baseUrl.trim() || !form.model.trim()) {
      setError("请填写名称、协议、地址和模型。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await api.saveModel(form);
      await reload();
      setSelectedId(form.id);
      drafts.current.delete(selectedId);
      const saved = { ...form, apiKey: "" };
      drafts.current.set(form.id, saved);
      setForm(saved);
      setNotice("已保存");
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };
  const modelOptions = Array.from(new Set([form.model, ...catalog].filter(Boolean))).map(
    (id) => ({ value: id, label: id }),
  );

  return (
    <div className="model-settings">
      <header className="model-page-head">
        <h2>模型</h2>
        <button className="secondary-action" type="button" onClick={add}>
          <PlusIcon className="button-icon" />
          添加
        </button>
      </header>
      <div className="model-workspace">
        <nav className="model-list" aria-label="已配置的模型">
          {creating && (
            <button
              type="button"
              className="model-item is-active"
              aria-current="true"
              onClick={() => choose("new")}
            >
              <span>
                <strong>{form.name.trim() || "新模型"}</strong>
                <small>未保存</small>
              </span>
            </button>
          )}
          {bootstrap.models.map((model) => (
            <button
              key={model.id}
              type="button"
              className={`model-item${model.id === selectedId ? " is-active" : ""}`}
              aria-current={model.id === selectedId ? "true" : undefined}
              onClick={() => choose(model.id)}
            >
              <span>
                <strong>{model.name}</strong>
                <small>
                  {presetLabel(model)}
                  {model.model ? ` · ${model.model}` : ""}
                </small>
              </span>
              {model.active && <span className="tag tag-active">默认</span>}
            </button>
          ))}
          {!bootstrap.models.length && !creating && (
            <p className="model-list-empty">还没有模型</p>
          )}
        </nav>
        <MotionSwitch viewKey={selectedId} kind="panel" className="model-editor">
          <form
            className="model-form"
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
          >
            <header className="model-form-head">
              <h3>{creating ? "添加模型" : (selected?.name ?? "模型")}</h3>
              {dirty && <span className="tag">未保存</span>}
              {selected?.active && <span className="tag tag-active">默认模型</span>}
            </header>
            {error && <Toast message={error} onDismiss={() => setError("")} />}
            {notice && <Toast message={notice} onDismiss={() => setNotice("")} />}
            <section className="form-section">
              <label>
                <span>服务商</span>
                <Select
                  label="服务商"
                  placeholder="选择服务商"
                  value={form.preset}
                  options={presetOptions}
                  onChange={applyPreset}
                />
              </label>
              <label>
                <span>名称</span>
                <input
                  required
                  value={form.name}
                  placeholder="例如 DeepSeek"
                  onChange={(event) => change("name", event.target.value)}
                />
              </label>
              <label>
                <span>请求协议</span>
                <Select
                  label="请求协议"
                  placeholder="选择协议"
                  value={form.protocol}
                  options={protocols}
                  onChange={(value) => change("protocol", value)}
                />
              </label>
              <label>
                <span>API 地址</span>
                <input
                  type="url"
                  required
                  placeholder="https://api.example.com/v1"
                  value={form.baseUrl}
                  onChange={(event) => change("baseUrl", event.target.value)}
                />
              </label>
              {needsKey && (
                <>
                  <label>
                    <span>
                      API Key
                      {preset?.keyUrl ? (
                        <a
                          className="model-key-link"
                          href={preset.keyUrl}
                          target="_blank"
                          rel="noreferrer"
                        >
                          申请密钥
                        </a>
                      ) : null}
                    </span>
                    <input
                      type="password"
                      autoComplete="off"
                      aria-label="API Key"
                      required={creating}
                      placeholder={selected ? "不修改请留空" : "sk-…"}
                      value={form.apiKey}
                      onChange={(event) => change("apiKey", event.target.value)}
                    />
                  </label>
                </>
              )}
              <div className="model-field">
                <span>模型</span>
                <div className="model-pick">
                  {catalog.length ? (
                    <Select
                      label="模型"
                      placeholder="选择模型"
                      value={form.model}
                      options={modelOptions}
                      onChange={(value) => change("model", value)}
                    />
                  ) : (
                    <input
                      required
                      aria-label="模型"
                      placeholder={preset?.model || "模型名称"}
                      value={form.model}
                      onChange={(event) => change("model", event.target.value)}
                    />
                  )}
                  <button
                    type="button"
                    className="secondary-action"
                    disabled={listing || busy || !form.protocol || !form.baseUrl.trim()}
                    onClick={() => void fetchModels()}
                  >
                    {listing ? "获取中…" : "获取模型"}
                  </button>
                </div>
              </div>
            </section>
            <section className="form-section model-advanced">
              <button
                type="button"
                className="model-advanced-toggle"
                aria-expanded={advanced}
                onClick={() => setAdvanced((open) => !open)}
              >
                <DisclosureChevron expanded={advanced} />
                高级选项
              </button>
              <div
                className={`model-advanced-body${advanced ? "" : " is-collapsed"}`}
                inert={!advanced}
              >
                <div className="model-advanced-inner">
                  <h4>连接</h4>
                  {(form.protocol === "anthropic-messages" ||
                    form.protocol === "gemini") && (
                    <>
                      <Select
                        label="鉴权方式"
                        value={form.auth}
                        options={authModes}
                        onChange={(value) =>
                          updateForm((draft) => ({
                            ...draft,
                            auth: value as ModelForm["auth"],
                          }))
                        }
                      />
                    </>
                  )}
                  <label>
                    <span>响应超时（秒）</span>
                    <input
                      type="number"
                      min={1}
                      max={600}
                      required
                      value={form.timeoutMs / 1000}
                      onChange={(event) =>
                        updateForm((draft) => ({
                          ...draft,
                          timeoutMs: Number(event.target.value) * 1000,
                        }))
                      }
                    />
                  </label>
                  <h4>窗口</h4>
                  <div className="form-grid">
                    <label>
                      <span>上下文长度</span>
                      <input
                        type="number"
                        min={8192}
                        max={2000000}
                        required
                        value={form.contextWindow}
                        onChange={(event) =>
                          updateForm((draft) => ({
                            ...draft,
                            contextWindow: Number(event.target.value),
                          }))
                        }
                      />
                    </label>
                    <label>
                      <span>最大输出</span>
                      <input
                        type="number"
                        min={256}
                        max={128000}
                        required
                        value={form.maxOutputTokens}
                        onChange={(event) =>
                          updateForm((draft) => ({
                            ...draft,
                            maxOutputTokens: Number(event.target.value),
                          }))
                        }
                      />
                    </label>
                  </div>
                  {form.protocol === "anthropic-messages" && (
                    <label className="toggle-row">
                      <span>提示缓存</span>
                      <button
                        type="button"
                        className={`switch ${form.promptCache ? "on" : ""}`}
                        role="switch"
                        aria-checked={form.promptCache}
                        onClick={() =>
                          updateForm((draft) => ({
                            ...draft,
                            promptCache: !draft.promptCache,
                          }))
                        }
                      />
                    </label>
                  )}
                </div>
              </div>
            </section>
            <footer className="detail-actions">
              <button className="primary-action" disabled={busy}>
                {busy ? "保存中…" : "保存"}
              </button>
              {selected && !selected.active && (
                <button
                  type="button"
                  className="secondary-action"
                  disabled={busy}
                  onClick={() => {
                    setBusy(true);
                    void api
                      .useModel(selected.id)
                      .then(reload)
                      .catch((reason) => setError(readError(reason)))
                      .finally(() => setBusy(false));
                  }}
                >
                  设为默认
                </button>
              )}
              {selected && (
                <button
                  type="button"
                  className="secondary-action"
                  disabled={busy}
                  onClick={() => {
                    if (!window.confirm(`删除「${selected.name}」？`)) {
                      return;
                    }
                    setBusy(true);
                    void api
                      .deleteModel(selected.id)
                      .then(async (result) => {
                        drafts.current.delete(selected.id);
                        await reload();
                        const next =
                          bootstrap.models.find(
                            (model) =>
                              model.id === result.activeModel &&
                              model.id !== selected.id,
                          ) ??
                          bootstrap.models.find(
                            (model) => model.id !== selected.id,
                          );
                        resetEditor(next?.id ?? "new", formFor(next));
                        setNotice("已删除");
                      })
                      .catch((reason) => setError(readError(reason)))
                      .finally(() => setBusy(false));
                  }}
                >
                  <TrashIcon className="button-icon" />
                  删除
                </button>
              )}
            </footer>
          </form>
        </MotionSwitch>
      </div>
    </div>
  );
}
