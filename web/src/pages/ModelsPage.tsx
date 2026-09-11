import { useEffect, useState } from "react";

import { api } from "../api";
import { CheckIcon, PlusIcon } from "../components/icons";
import type { Bootstrap, ModelInfo } from "../types";

/** The machine protocol names are what the configuration stores, but a user
 * picking from a dropdown needs to recognise their provider. */
const protocols = [
  ["openai-responses", "OpenAI（Responses 接口）"],
  ["openai-chat", "OpenAI 兼容（Chat Completions）"],
  ["anthropic-messages", "Anthropic Claude"],
  ["gemini", "Google Gemini"],
  ["ollama-chat", "Ollama（装在自己电脑上的模型）"],
] as const;

const protocolLabel = (value: string) =>
  protocols.find(([id]) => id === value)?.[1] ?? value;

type ModelDraft = Omit<ModelInfo, "active"> & { apiKey: string };

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
  const adding = selectedId === "new";
  const [form, setForm] = useState<ModelDraft>(() => draft(selected));
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setForm(draft(selected));
    setError("");
    setNotice("");
  }, [selectedId, selected]);

  const update = (field: keyof ModelDraft, value: string) => {
    setForm((current) => ({ ...current, [field]: value }));
  };

  const missing = [
    !form.name.trim() && "显示名称",
    !form.model.trim() && "模型名称",
    !form.baseUrl.trim() && "服务地址",
  ].filter(Boolean) as string[];

  const save = async () => {
    setSaving(true);
    setError("");
    setNotice("");
    try {
      await api.saveModel(form);
      await reload();
      setSelectedId(form.id);
      setNotice("已保存。新任务会使用这份配置。");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  };

  const activate = async () => {
    setSaving(true);
    setError("");
    setNotice("");
    try {
      await api.useModel(form.id);
      await reload();
      setNotice("已切换：之后的新任务都用这个模型。");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page-sheet split-page">
      <aside className="picker">
        <div className="picker-head">
          <span>已配置的模型</span>
          <button
            type="button"
            className="subtle-action"
            onClick={() => {
              setSelectedId("new");
              setForm(draft());
            }}
          >
            <PlusIcon className="button-icon" />
            添加
          </button>
        </div>
        <div className="picker-list">
          {adding ? (
            <div className="picker-item is-active">
              <span>
                <strong>新模型</strong>
                <small>还没保存</small>
              </span>
            </div>
          ) : null}
          {bootstrap.models.map((model) => (
            <button
              key={model.id}
              type="button"
              className={`picker-item${selectedId === model.id ? " is-active" : ""}`}
              onClick={() => setSelectedId(model.id)}
            >
              <span>
                <strong>{model.name}</strong>
                <small>
                  {protocolLabel(model.protocol)} · {model.model}
                </small>
              </span>
              {model.active ? (
                <span className="tag tag-active">
                  <CheckIcon className="tag-icon" />
                  正在用
                </span>
              ) : null}
            </button>
          ))}
        </div>
      </aside>

      <div className="detail-pane">
        <header className="detail-head">
          <h2>{adding ? "添加模型" : selected?.name}</h2>
          {selected?.active ? (
            <span className="tag tag-active">
              <CheckIcon className="tag-icon" />
              正在用
            </span>
          ) : null}
        </header>

        {notice ? <p className="notice ok">{notice}</p> : null}
        {error ? (
          <div className="inline-error" role="alert">
            <strong>保存失败</strong>
            <span>{error}</span>
          </div>
        ) : null}

        <section className="form-section">
          <h3>怎么称呼它</h3>
          <label>
            <span>显示名称</span>
            <input
              value={form.name}
              placeholder="例如：主力模型"
              onChange={(event) => update("name", event.target.value)}
            />
          </label>
        </section>

        <section className="form-section">
          <h3>连接到哪家服务</h3>
          <label>
            <span>接口类型</span>
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
          <p className="field-help">
            不确定选哪个？看你用的服务商文档里写的是哪种接口。多数第三方中转站用
            “OpenAI 兼容”。
          </p>
          <label>
            <span>服务地址</span>
            <input
              placeholder="https://api.openai.com/v1"
              value={form.baseUrl}
              onChange={(event) => update("baseUrl", event.target.value)}
            />
          </label>
          <label>
            <span>模型名称</span>
            <input
              placeholder="服务商给的模型代号，例如 gpt-4o"
              value={form.model}
              onChange={(event) => update("model", event.target.value)}
            />
          </label>
          <label>
            <span>密钥</span>
            <input
              type="password"
              autoComplete="off"
              placeholder={
                selected ? "留空表示不改动原来的密钥" : "粘贴服务商给你的密钥"
              }
              value={form.apiKey}
              onChange={(event) => update("apiKey", event.target.value)}
            />
          </label>
          <p className="field-help">
            密钥会明文保存在配置文件里，界面不会把它显示回来。
          </p>
        </section>

        <footer className="detail-actions">
          {selected && !selected.active ? (
            <button
              type="button"
              className="secondary-action"
              disabled={saving}
              onClick={() => void activate()}
            >
              改用这个模型
            </button>
          ) : null}
          <button
            type="button"
            className="primary-action"
            disabled={saving || missing.length > 0}
            title={
              missing.length ? `还需要填写：${missing.join("、")}` : undefined
            }
            onClick={() => void save()}
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </footer>
        {missing.length ? (
          <p className="field-help">还需要填写：{missing.join("、")}。</p>
        ) : null}
      </div>
    </div>
  );
}
