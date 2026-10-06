import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
} from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Dialog } from "../components/overlay/Dialog";
import { useT, type Text } from "../i18n";
import "./UpdateDialog.css";
import changelogSource from "./release-notes/changelog-0.1.0.md?raw";
import guideSource from "./release-notes/guide.md?raw";
import faqSource from "./release-notes/faq.md?raw";

/** 当前版本的更新日志。发新版本时：加一份 md、package.json 升版本号。 */
const CHANGELOG_VERSION = __APP_VERSION__;

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
  initialTab = "changelog",
}: {
  open: boolean;
  onClose(): void;
  /** 打开时落在哪个标签页；入口是「使用说明」时传 guide。 */
  initialTab?: string;
}) {
  const t = useT();
  const id = useId();
  const body = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState(initialTab);
  useLayoutEffect(() => {
    if (open) setTab(initialTab);
  }, [open, initialTab]);
  const items = useMemo(() => tabs(t), [t]);
  const active = items.find((item) => item.key === tab) ?? items[0];
  // 文档间的跳转链接（如 faq.md）只切换标签页，不离开弹窗；外部链接照常打开。
  const onArticleClick = (event: MouseEvent<HTMLDivElement>) => {
    const link = (event.target as HTMLElement).closest("a");
    if (!link) return;
    const match = /^(guide|faq|changelog[^/?#]*)\.md$/.exec(
      link.getAttribute("href") ?? "",
    );
    const destination = match?.[1];
    if (!destination) return;
    event.preventDefault();
    setTab(
      destination.startsWith("guide")
        ? "guide"
        : destination.startsWith("faq")
          ? "faq"
          : "changelog",
    );
  };
  useLayoutEffect(() => {
    if (open && body.current) body.current.scrollTop = 0;
  }, [tab, open]);
  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement;
    const timer = window.setTimeout(() => {
      body.current
        ?.closest(".sd-dialog")
        ?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')
        ?.focus({ preventScroll: true });
    }, 0);
    const keepFocus = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const dialog = body.current?.closest(".sd-dialog");
      if (!dialog) return;
      const controls = Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button:not([disabled]):not([tabindex="-1"]), a[href], [tabindex="0"]',
        ),
      );
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (!first || !last) return;
      if (!dialog.contains(document.activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", keepFocus);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("keydown", keepFocus);
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus({ preventScroll: true });
      }
    };
  }, [open]);
  if (!active) return null;
  return (
    <Dialog
      title={
        active.key === "guide"
          ? t.account.help
          : active.key === "faq"
            ? t.update.faq
            : t.update.title
      }
      open={open}
      onClose={onClose}
    >
      <div className="update-dialog" data-tab={active.key}>
        <div className="update-dialog-toolbar">
          <div
            className="update-dialog-tabs"
            role="tablist"
            aria-label={t.update.title}
          >
            {items.map((item) => (
              <button
                key={item.key}
                type="button"
                role="tab"
                id={`${id}-${item.key}`}
                aria-controls={`${id}-panel`}
                aria-selected={item.key === active.key}
                tabIndex={item.key === active.key ? 0 : -1}
                className={item.key === active.key ? "is-active" : undefined}
                onClick={() => setTab(item.key)}
                onKeyDown={(event) => {
                  if (
                    !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                      event.key,
                    )
                  )
                    return;
                  event.preventDefault();
                  const index = items.findIndex(
                    (entry) => entry.key === active.key,
                  );
                  const nextIndex =
                    event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? items.length - 1
                        : (index +
                            (event.key === "ArrowRight" ? 1 : -1) +
                            items.length) %
                          items.length;
                  const next = items[nextIndex];
                  if (!next) return;
                  setTab(next.key);
                  document.getElementById(`${id}-${next.key}`)?.focus();
                }}
              >
                {item.title}
              </button>
            ))}
          </div>
          <span className="update-dialog-version">v{CHANGELOG_VERSION}</span>
        </div>
        <div
          ref={body}
          id={`${id}-panel`}
          className="update-dialog-body"
          role="tabpanel"
          aria-labelledby={`${id}-${active.key}`}
          tabIndex={0}
        >
          <article
            className="update-dialog-article"
            key={active.key}
            onClick={onArticleClick}
          >
            <Markdown remarkPlugins={[remarkGfm]}>
              {active.source.replace(/^# [^\r\n]+\r?\n+/, "")}
            </Markdown>
          </article>
        </div>
      </div>
    </Dialog>
  );
}
