import { useMemo, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Dialog } from "../components/overlay/Dialog";
import { useT, type Text } from "../i18n";
import "./UpdateDialog.css";
import changelogSource from "./release-notes/changelog-0.1.0.md?raw";
import guideSource from "./release-notes/guide.md?raw";
import faqSource from "./release-notes/faq.md?raw";

/** 当前版本的更新日志。发新版本时：加一份 md、package.json 升版本号。 */
const CHANGELOG_VERSION = "0.1.0";

function tabs(t: Text): Array<{ key: string; title: string; source: string }> {
  return [
    { key: "changelog", title: t.update.changelog, source: changelogSource },
    { key: "guide", title: t.update.guide, source: guideSource },
    { key: "faq", title: t.update.faq, source: faqSource },
  ];
}

/**
 * 版本更新弹窗。启动时由 App 在「本地记录的版本 != 当前版本」时打开，
 * 包括第一次使用（无记录）。
 */
export function UpdateDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose(): void;
}) {
  const t = useT();
  const [tab, setTab] = useState("changelog");
  const items = useMemo(() => tabs(t), [t]);
  const active = items.find((item) => item.key === tab) ?? items[0];
  if (!active) return null;
  return (
    <Dialog
      title={`${t.update.title} · ${CHANGELOG_VERSION}`}
      open={open}
      onClose={onClose}
      footer={
        <button type="button" className="primary-action" onClick={onClose}>
          {t.update.start}
        </button>
      }
    >
      <div className="update-dialog">
        <div className="update-dialog-tabs" role="tablist">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              role="tab"
              aria-selected={item.key === active.key}
              className={item.key === active.key ? "is-active" : undefined}
              onClick={() => setTab(item.key)}
            >
              {item.title}
            </button>
          ))}
        </div>
        <div className="update-dialog-body" role="tabpanel">
          <Markdown remarkPlugins={[remarkGfm]}>{active.source}</Markdown>
        </div>
      </div>
    </Dialog>
  );
}
