import { useRef, useState } from "react";
import { api } from "../api";
import { readError } from "../session";
import { Toast } from "../components/Toast";
import { PlusIcon, TrashIcon } from "../components/icons";
import { Select } from "../components/Select";
import { MotionSwitch } from "../components/MotionSwitch";
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

function protocolLabel(value: string) {
  return protocols.find((item) => item.value === value)?.label ?? value;
}

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
    () =>
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
  const creating = selectedId === "new";
  const dirty =
    creating ||
    Boolean(form.apiKey) ||
    (selected != null &&
      (form.name !== selected.name ||
        form.protocol !== selected.protocol ||
        form.model !== selected.model ||
        form.baseUrl !== selected.baseUrl ||
        form.timeoutMs !== (selected.timeoutMs ?? 120000)));
  const choose = (id: string) => {
    drafts.current.set(selectedId, form);
    const model = bootstrap.models.find((item) => item.id === id);
    setSelectedId(id);
    setForm(drafts.current.get(id) ?? formFor(model));
    setError("");
    setNotice("");
  };
  const add = () => {
    if (creating) {
      const fresh = formFor();
      drafts.current.set("new", fresh);
      setForm(fresh);
      setError("");
      setNotice("");
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
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="model-settings">
      <header className="model-page-head">
        <div>
          <h2>模型</h2>
          <p>配置对话用的模型服务。新对话使用默认模型，每个已有对话都有自己绑定的模型。</p>
        </div>
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
                <small>尚未保存</small>
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
                  {protocolLabel(model.protocol)}
                  {model.model ? ` · ${model.model}` : ""}
                </small>
              </span>
              {model.active && <span className="tag tag-active">默认</span>}
            </button>
          ))}
          {!bootstrap.models.length && !creating && (
            <p className="model-list-empty">还没有模型服务</p>
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
              <h4>显示</h4>
              <div className="form-grid">
                <label>
                  <span>名称</span>
                  <input
                    required
                    value={form.name}
                    placeholder="出现在对话和列表里的名字"
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
            </section>
            <section className="form-section">
              <h4>连接</h4>
              <label>
                <span>API 地址</span>
                <input
                  type="url"
                  required
                  value={form.baseUrl}
                  onChange={(event) => change("baseUrl", event.target.value)}
                />
              </label>
              <div className="form-grid">
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
              </div>
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
              <p className="field-help">
                {selected
                  ? "密钥不会回显。留空继续用已保存的，填写则替换。"
                  : "密钥只保存在本机配置里。"}
              </p>
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
                  设为默认模型
                </button>
              )}
              {selected && (
                <button
                  type="button"
                  className="secondary-action"
                  disabled={busy || bootstrap.models.length < 2}
                  title={
                    bootstrap.models.length < 2
                      ? "至少保留一个模型作为默认"
                      : "删除这个模型配置"
                  }
                  onClick={() => {
                    if (
                      !window.confirm(
                        `删除「${selected.name}」？使用它的对话会改用默认模型。`,
                      )
                    ) {
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
                        setSelectedId(next?.id ?? "new");
                        setForm(formFor(next));
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
