import { useRef, useState } from "react";
import { api } from "../api";
import { PlusIcon } from "../components/icons";
import { Select } from "../components/Select";
import type { Bootstrap } from "../types";
import "./ModelsPage.css";

const protocols = [
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "openai-chat", label: "OpenAI 兼容" },
  { value: "anthropic-messages", label: "Anthropic" },
  { value: "gemini", label: "Google Gemini" },
  { value: "ollama-chat", label: "Ollama" },
];
type ModelForm = {
  id: string;
  name: string;
  protocol: string;
  model: string;
  baseUrl: string;
  apiKey: string;
  timeoutMs: number;
};

function formFor(model?: Bootstrap["models"][number]): ModelForm {
  return {
    id: model?.id ?? `model-${Date.now()}`,
    name: model?.name ?? "",
    protocol: model?.protocol ?? "openai-responses",
    model: model?.model ?? "",
    baseUrl: model?.baseUrl ?? "https://api.openai.com/v1",
    apiKey: "",
    timeoutMs: model?.timeoutMs ?? 120000,
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
    bootstrap.models.find((model) => model.active)?.id ??
      bootstrap.models[0]?.id ??
      "new",
  );
  const selected = bootstrap.models.find((model) => model.id === selectedId);
  const [form, setForm] = useState<ModelForm>(() => formFor(selected));
  const drafts = useRef(new Map<string, ModelForm>());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const choose = (id: string) => {
    drafts.current.set(selectedId, form);
    const model = bootstrap.models.find((item) => item.id === id);
    setSelectedId(id);
    setForm(drafts.current.get(id) ?? formFor(model));
    setError("");
    setNotice("");
  };
  const updateForm = (next: (draft: ModelForm) => ModelForm) =>
    setForm((current) => {
      const updated = next(current);
      drafts.current.set(selectedId, updated);
      return updated;
    });
  const change = (
    key: "name" | "protocol" | "model" | "baseUrl" | "apiKey",
    value: string,
  ) => updateForm((draft) => ({ ...draft, [key]: value }));
  const save = async () => {
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
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="page-sheet model-settings">
      <header className="model-picker" data-motion="panel">
        <div>
          <h2>模型</h2>
          <p>选择现有连接，或添加一个新的模型服务。</p>
        </div>
        <div className="model-picker-actions">
          <Select
            label="选择模型"
            value={selectedId}
            options={[
              ...bootstrap.models.map((model) => ({
                value: model.id,
                label: model.name,
                description: `${model.model}${model.active ? " · 当前使用" : ""}`,
              })),
              ...(selectedId === "new"
                ? [{ value: "new", label: "新模型" }]
                : []),
            ]}
            onChange={choose}
          />
          <button
            className="icon-button"
            aria-label="添加模型"
            onClick={() => {
              if (selectedId === "new") {
                const fresh = formFor();
                drafts.current.set("new", fresh);
                setForm(fresh);
              } else choose("new");
            }}
          >
            <PlusIcon className="button-icon" />
          </button>
        </div>
      </header>
      <form
        key={selectedId}
        className="detail-pane model-form"
        data-motion="panel"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <header className="detail-head">
          <h2>{selected?.name ?? "添加模型"}</h2>
          {selected?.active && <span className="tag">当前模型</span>}
        </header>
        {error && (
          <div className="inline-error" role="alert">
            {error}
          </div>
        )}
        {notice && (
          <p className="notice" role="status">
            {notice}
          </p>
        )}
        <section className="form-section">
          <div className="form-grid">
            <label>
              <span>名称</span>
              <input
                required
                value={form.name}
                placeholder="模型显示名称"
                onChange={(event) => change("name", event.target.value)}
              />
            </label>
            <label>
              <span>协议</span>
              <Select
                label="模型协议"
                value={form.protocol}
                options={protocols}
                onChange={(value) => change("protocol", value)}
              />
            </label>
          </div>
          <label>
            <span>API 地址</span>
            <input
              type="url"
              required
              value={form.baseUrl}
              onChange={(event) => change("baseUrl", event.target.value)}
            />
          </label>
          <label>
            <span>模型 ID</span>
            <input
              required
              placeholder="服务商提供的模型标识"
              value={form.model}
              onChange={(event) => change("model", event.target.value)}
            />
          </label>
          <label>
            <span>API Key</span>
            <input
              type="password"
              autoComplete="off"
              placeholder={selected ? "留空保留现有密钥" : "输入密钥"}
              value={form.apiKey}
              onChange={(event) => change("apiKey", event.target.value)}
            />
          </label>
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
          <p className="field-help">等待首个响应或流式数据的最长时间。</p>
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
                  .catch((reason) => setError(String(reason)))
                  .finally(() => setBusy(false));
              }}
            >
              设为当前模型
            </button>
          )}
        </footer>
      </form>
    </div>
  );
}
