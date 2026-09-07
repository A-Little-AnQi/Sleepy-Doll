import { useEffect, useState } from "react";

import { api } from "../api";
import type { Bootstrap, ModelInfo } from "../types";

const protocols = [
  ["openai-responses", "OpenAI Responses"],
  ["openai-chat", "OpenAI-compatible Chat"],
  ["anthropic-messages", "Anthropic Messages"],
  ["gemini", "Google Gemini"],
  ["ollama-chat", "Ollama Chat"],
] as const;

type ModelDraft = Omit<ModelInfo, "active"> & { apiKey: string };

function ModelArt({ protocol }: { protocol: string }) {
  if (protocol === "openai-chat") {
    return (
      <svg viewBox="0 0 44 44" aria-hidden="true">
        <path d="M7 10h24v18H18l-8 6 2-6H7Z" />
        <path d="M14 17h16M14 22h10" />
        <path d="M31 15h6v16h-9" />
      </svg>
    );
  }
  if (protocol === "anthropic-messages") {
    return (
      <svg viewBox="0 0 44 44" aria-hidden="true">
        <path d="m22 6 4 11 11-4-7 9 9 7-12-1-2 12-4-11-11 4 7-9-9-7 12 1Z" />
        <path d="M18 27 22 16l4 11M19.5 23h5" />
      </svg>
    );
  }
  if (protocol === "gemini") {
    return (
      <svg viewBox="0 0 44 44" aria-hidden="true">
        <path d="M22 5c2 10 7 15 17 17-10 2-15 7-17 17-2-10-7-15-17-17C15 20 20 15 22 5Z" />
      </svg>
    );
  }
  if (protocol === "ollama-chat") {
    return (
      <svg viewBox="0 0 44 44" aria-hidden="true">
        <path d="M8 11h28v23H8Z" />
        <path d="M14 18h16M14 24h10M14 29h13" />
        <path d="M15 7v4M29 7v4" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 44 44" aria-hidden="true">
      <path d="M9 13h18l8 8v11H17l-8-8Z" />
      <path d="M27 13v9h8M15 20h7M15 25h13" />
    </svg>
  );
}

function AddModelArt() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true">
      <path d="M6 5h14l6 6v16H6Z" />
      <path d="M20 5v7h6M11 18h10M16 13v10" />
    </svg>
  );
}

function CheckArt({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 28 28" aria-hidden="true">
      <path d="m4 15 6 6L24 6" />
    </svg>
  );
}

function draft(model?: ModelInfo): ModelDraft {
  return model
    ? { ...model, apiKey: "" }
    : {
        id: `model-${Date.now()}`,
        name: "新模型",
        protocol: "openai-responses",
        model: "",
        baseUrl: "https://api.openai.com/v1",
        apiKey: "",
      };
}

interface ModelsPageProps {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
}

export function ModelsPage({ bootstrap, reload }: ModelsPageProps) {
  const [selectedId, setSelectedId] = useState(
    bootstrap.models[0]?.id ?? "new",
  );
  const selected = bootstrap.models.find((model) => model.id === selectedId);
  const [form, setForm] = useState<ModelDraft>(() => draft(selected));
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setForm(draft(selected));
  }, [selectedId, selected]);

  const update = (field: keyof ModelDraft, value: string) => {
    setForm((current) => ({ ...current, [field]: value }));
  };

  const save = async () => {
    setSaving(true);
    setNotice("");
    try {
      await api.saveModel(form);
      await reload();
      setSelectedId(form.id);
      setNotice("模型配置已保存。密钥不会返回到界面。\n");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const activate = async () => {
    await api.useModel(form.id);
    await reload();
    setNotice("已设为唯一激活模型，新任务会使用这项配置。");
  };

  return (
    <section className="models-workspace">
      <aside className="model-browser">
        <div className="browser-title">
          <span>模型配置</span>
          <button
            className="add-model-action"
            aria-label="添加模型"
            onClick={() => {
              setSelectedId("new");
              setForm(draft());
            }}
          >
            <AddModelArt />
            <span>添加</span>
          </button>
        </div>
        <div className="model-browser-list">
          {bootstrap.models.map((model) => (
            <button
              key={model.id}
              className={selectedId === model.id ? "active" : ""}
              onClick={() => setSelectedId(model.id)}
            >
              <span className="model-monogram">
                <ModelArt protocol={model.protocol} />
              </span>
              <span>
                <strong>{model.name}</strong>
                <small>{model.model}</small>
              </span>
              {model.active ? <CheckArt className="active-check" /> : null}
            </button>
          ))}
        </div>
      </aside>

      <div className="model-editor">
        <header className="editor-header">
          <div>
            <h2>{selected ? selected.name : "添加模型"}</h2>
          </div>
          {selected?.active ? (
            <span className="active-model-badge">
              <CheckArt />
              当前模型
            </span>
          ) : null}
        </header>
        {notice ? <div className="notice">{notice}</div> : null}
        <div className="form-section">
          <h3>基本信息</h3>
          <div className="form-grid two-columns">
            <label>
              <span>显示名称</span>
              <input
                value={form.name}
                onChange={(event) => update("name", event.target.value)}
              />
            </label>
            <label>
              <span>配置 ID</span>
              <input
                value={form.id}
                disabled={Boolean(selected)}
                onChange={(event) => update("id", event.target.value)}
              />
            </label>
          </div>
          <label>
            <span>协议</span>
            <select
              value={form.protocol}
              onChange={(event) => update("protocol", event.target.value)}
            >
              {protocols.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="form-section">
          <h3>连接</h3>
          <label>
            <span>模型名称</span>
            <input
              placeholder="模型 API 使用的 ID"
              value={form.model}
              onChange={(event) => update("model", event.target.value)}
            />
          </label>
          <label>
            <span>Base URL</span>
            <div className="input-with-icon">
              <input
                value={form.baseUrl}
                onChange={(event) => update("baseUrl", event.target.value)}
              />
            </div>
          </label>
          <label>
            <span>API Key</span>
            <input
              type="password"
              autoComplete="off"
              placeholder={
                selected ? "留空以保留原值" : "支持 ${ENV:VARIABLE_NAME}"
              }
              value={form.apiKey}
              onChange={(event) => update("apiKey", event.target.value)}
            />
          </label>
          <p className="field-help">
            密钥保存在本地配置中。建议填写环境变量引用，不要直接保存明文。
          </p>
        </div>
        <footer className="editor-actions">
          {selected && !selected.active ? (
            <button
              className="secondary-action"
              onClick={() => void activate()}
            >
              设为当前
            </button>
          ) : (
            <span />
          )}
          <button
            className="save-action"
            disabled={saving || !form.id || !form.model || !form.baseUrl}
            onClick={() => void save()}
          >
            {saving ? "保存中…" : "保存配置"}
          </button>
        </footer>
      </div>
    </section>
  );
}
