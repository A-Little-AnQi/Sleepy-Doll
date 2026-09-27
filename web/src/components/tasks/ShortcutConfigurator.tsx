import { useEffect, useRef, useState } from "react";
import { Dialog } from "../overlay/Dialog";
import { Select } from "../controls/Select";
import { api } from "../../ipc/api";
import { isRunning, readError, session, useSession } from "../../session";
import { defaultModelId } from "../../models";
import { taskError } from "./task-display";
import type { Bootstrap, TaskInfo, TaskSummary } from "../../ipc/types";
import "./shortcut-configurator.css";

type Props = {
  bootstrap: Bootstrap;
  open: boolean;
  onClose(): void;
  reload(): Promise<void>;
  target?: TaskSummary | undefined;
  referenceConversationId?: string | undefined;
};
export function ShortcutConfigurator(props: Props) {
  return <ShortcutForm key={props.target?.id ?? "add"} {...props} />;
}
function ShortcutForm({
  bootstrap,
  open,
  onClose,
  reload,
  target,
  referenceConversationId,
}: Props) {
  const draftKey = `sleepy-doll-shortcut-item:${target?.id ?? "add"}`;
  const [item, setItem] = useState(
    () =>
      localStorage.getItem(draftKey) ??
      (target
        ? `将 ${target.shortcut?.applicationName ?? ""} 中的「${target.shortcut?.targetName ?? target.name}」封装成快捷入口`
        : ""),
  );
  const [model, setModel] = useState(() => defaultModelId(bootstrap.models));
  const [reference, setReference] = useState(true);
  const [run, setRun] = useState<TaskInfo>();
  const data = useSession(run?.conversationId);
  const proposal = data.shortcutProposal;
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [closing, setClosing] = useState(false);
  const accepted = useRef<TaskInfo | undefined>(undefined);
  const pending = useRef<Promise<TaskInfo> | undefined>(undefined);
  const initialized = useRef<string | undefined>(undefined);
  const busy = starting || isRunning(data.task ?? run);
  useEffect(() => {
    if (proposal && initialized.current !== run?.id) {
      initialized.current = run?.id;
      setName(proposal.name);
      setDescription(proposal.description);
    }
  }, [proposal, run?.id]);
  useEffect(
    () => () => {
      const request = pending.current;
      if (request)
        void request.then((value) => api.cancelTask(value.id)).catch(() => {});
      else if (accepted.current)
        void api.cancelTask(accepted.current.id).catch(() => {});
    },
    [],
  );
  const identify = async () => {
    if (!item.trim() || busy || saving) return;
    setStarting(true);
    setError("");
    setRun(undefined);
    initialized.current = undefined;
    const request = api.configureShortcut(
      item.trim(),
      undefined,
      crypto.randomUUID(),
      model || undefined,
      target?.id,
      reference ? referenceConversationId : undefined,
    );
    pending.current = request;
    try {
      const value = await request;
      accepted.current = value;
      setRun(value);
      session(value.conversationId).start();
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      pending.current = undefined;
      setStarting(false);
    }
  };
  const close = async () => {
    if (closing || saving) return;
    setClosing(true);
    setError("");
    try {
      const value = pending.current
        ? await pending.current.catch(() => undefined)
        : accepted.current;
      if (value) await api.cancelTask(value.id);
      onClose();
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      setClosing(false);
    }
  };
  const save = async () => {
    if (!run || !proposal || !name.trim() || busy || saving) return;
    setSaving(true);
    setError("");
    try {
      await api.acceptShortcut(run.id, name.trim(), description.trim());
      await reload();
      localStorage.removeItem(draftKey);
      setRun(undefined);
      accepted.current = undefined;
      onClose();
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      setSaving(false);
    }
  };
  const answerQuestion = async () => {
    if (!run || !answer.trim() || starting) return;
    setStarting(true);
    setError("");
    try {
      await api.supplement(run.id, answer.trim());
      setAnswer("");
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      setStarting(false);
    }
  };
  const respondApproval = async (approved: boolean) => {
    if (!run || starting) return;
    setStarting(true);
    setError("");
    try {
      await api.approve(run.id, approved);
    } catch (reason) {
      setError(taskError(readError(reason)));
    } finally {
      setStarting(false);
    }
  };
  const progress =
    data.messages.findLast(
      (message) => message.role === "assistant" && message.content.trim(),
    )?.content || data.stream;
  return (
    <Dialog
      open={open}
      onClose={() => void close()}
      title={target ? `调整「${target.name}」的入口` : "添加快捷任务"}
      subtitle="从已有任务或配置中识别所选项目，确认后保存入口。"
      className="shortcut-configuration-dialog"
      footer={
        <>
          <span className="shortcut-config-state">
            {closing
              ? "正在取消识别…"
              : proposal
                ? "确认入口信息后保存即可使用。"
                : "也可以在原对话里指定已有项加入快捷任务。"}
          </span>
          <button
            className="subtle-action"
            disabled={saving || closing}
            onClick={() => void close()}
          >
            取消
          </button>
          {proposal ? (
            <button
              className="primary-action"
              disabled={busy || saving || closing || !name.trim()}
              onClick={() => void save()}
            >
              {saving ? "正在保存" : "保存入口"}
            </button>
          ) : (
            <button
              className="primary-action"
              disabled={
                busy || closing || !item.trim() || !bootstrap.models.length
              }
              onClick={() => void identify()}
            >
              {busy ? "正在识别" : "识别已有项"}
            </button>
          )}
        </>
      }
    >
      <label className="shortcut-form-field">
        <span>要加入哪一项？</span>
        <textarea
          aria-label="要加入哪一项"
          placeholder="例如：把 BetterGI 里已有的血斛采集配置组加入快捷任务"
          rows={3}
          value={item}
          disabled={busy || saving}
          onChange={(event) => {
            setItem(event.target.value);
            localStorage.setItem(draftKey, event.target.value);
          }}
        />
      </label>
      <div className="shortcut-form-options">
        {referenceConversationId && (
          <label className="shortcut-reference">
            <input
              type="checkbox"
              checked={reference}
              disabled={busy}
              onChange={(event) => setReference(event.target.checked)}
            />
            参考当前对话中的已有内容
          </label>
        )}
        {bootstrap.models.length > 1 && (
          <label className="shortcut-model">
            <span>识别使用的 AI</span>
            <Select
              label="识别使用的 AI"
              value={model}
              onChange={setModel}
              disabled={busy}
              options={bootstrap.models.map((value) => ({
                value: value.id,
                label: value.name,
              }))}
            />
          </label>
        )}
      </div>
      {!bootstrap.models.length && (
        <div className="shortcut-form-notice">
          <p>先添加一个 AI 模型，才能识别并封装已有项。</p>
          <button
            className="secondary-action"
            onClick={() => {
              onClose();
              window.dispatchEvent(new CustomEvent("sleepy-doll:open-models"));
            }}
          >
            设置 AI 模型
          </button>
        </div>
      )}
      {busy && !data.question && !data.approval && (
        <div className="shortcut-identifying" role="status">
          <span className="activity-spinner" />
          <div>
            <strong>正在定位已有项</strong>
            <p>
              {progress
                ? taskError(progress).slice(0, 180)
                : "读取已有任务并核对运行入口…"}
            </p>
          </div>
          <button
            className="subtle-action"
            disabled={closing}
            onClick={() => {
              if (run) void api.cancelTask(run.id);
            }}
          >
            停止
          </button>
        </div>
      )}
      {data.approval && (
        <section className="shortcut-form-notice">
          <h3>确认读取已有内容</h3>
          <p>
            {taskError(
              data.approval.request.binding?.description ??
                "识别所选项目需要读取应用中的已有任务信息。",
            )}
          </p>
          <button
            className="subtle-action"
            disabled={starting}
            onClick={() => void respondApproval(false)}
          >
            拒绝
          </button>
          <button
            className="secondary-action"
            disabled={starting}
            onClick={() => void respondApproval(true)}
          >
            允许读取
          </button>
        </section>
      )}
      {data.question && (
        <section className="shortcut-form-notice">
          <h3>需要确认目标</h3>
          <p>{data.question}</p>
          <label className="shortcut-form-field">
            <span>补充信息</span>
            <input
              aria-label="补充信息"
              value={answer}
              onChange={(event) => setAnswer(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void answerQuestion();
              }}
            />
          </label>
          <button
            className="secondary-action"
            disabled={!answer.trim() || starting}
            onClick={() => void answerQuestion()}
          >
            补充并继续
          </button>
        </section>
      )}
      {error || data.task?.error ? (
        <p className="shortcut-form-error" role="alert">
          {error || taskError(data.task?.error)}
        </p>
      ) : null}
      {!busy &&
        run &&
        !proposal &&
        !data.question &&
        !error &&
        !data.task?.error &&
        data.task && (
          <div className="shortcut-form-notice">
            <p>
              {data.task.state === "cancelled"
                ? "识别已停止，已有内容保持原样。"
                : data.task.result ||
                  "没有找到可以封装的已有项，请补充具体名称。"}
            </p>
          </div>
        )}
      {proposal && (
        <section className="shortcut-preview">
          <div className="shortcut-preview-heading">
            <h3>入口预览</h3>
            <span>尚未保存</span>
          </div>
          <dl>
            <dt>应用</dt>
            <dd>{proposal.binding.applicationName}</dd>
            <dt>已有项</dt>
            <dd>{proposal.binding.targetName}</dd>
          </dl>
          <label className="shortcut-form-field">
            <span>入口名称</span>
            <input
              aria-label="入口名称"
              value={name}
              maxLength={120}
              disabled={saving}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label className="shortcut-form-field">
            <span>用途说明</span>
            <textarea
              aria-label="用途说明"
              value={description}
              rows={2}
              maxLength={400}
              disabled={saving}
              onChange={(event) => setDescription(event.target.value)}
            />
          </label>
          <button
            className="subtle-action"
            disabled={busy || saving}
            onClick={() => {
              setRun(undefined);
              accepted.current = undefined;
            }}
          >
            重新选择已有项
          </button>
        </section>
      )}
    </Dialog>
  );
}
