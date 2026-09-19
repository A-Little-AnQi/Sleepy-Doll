import { useEffect, useState } from "react";
import { api } from "../../ipc/api";
import { Dialog } from "../overlay/Dialog";
import { TextField } from "../controls/TextField";
import { useT } from "../../i18n";

export function ConfigEditor({
  path,
  open,
  onClose,
  onSaved,
  onPath,
}: {
  path: string;
  open: boolean;
  onClose(): void;
  onSaved(): Promise<void>;
  onPath?(path: string): void;
}) {
  const t = useT();
  const [filePath, setFilePath] = useState(path);
  const [content, setContent] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setError("");
    setFilePath(path);
    void api
      .configRead()
      .then((result) => {
        setContent(result.content);
        if (result.path) {
          setFilePath(result.path);
          onPath?.(result.path);
        }
      })
      .catch((error: Error) => setError(error.message));
  }, [open, path, onPath]);

  const save = async () => {
    setSaving(true);
    setError("");
    try {
      await api.configWrite(content);
      await onSaved();
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : t.configEditor.saveFailed);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="编辑配置"
      subtitle={filePath || path}
      footer={
        <>
          <button type="button" className="subtle-action" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="primary-action"
            disabled={saving}
            onClick={() => void save()}
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </>
      }
    >
      {error ? <p className="inline-error">{error}</p> : null}
      <TextField
        multiline
        aria-label={t.configEditor.fileContent}
        spellCheck={false}
        value={content}
        onChange={(event) => setContent(event.target.value)}
      />
    </Dialog>
  );
}
