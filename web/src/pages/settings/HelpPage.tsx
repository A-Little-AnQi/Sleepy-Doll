import {
  Children,
  isValidElement,
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Components } from "react-markdown";
import source from "./help.md?raw";
import "./HelpPage.css";

export type HelpOpen = "models" | "bridge" | "extensions" | "tasks";

const TARGETS: ReadonlySet<string> = new Set([
  "models",
  "bridge",
  "extensions",
  "tasks",
]);

const TOC = [...source.matchAll(/^## (.+)$/gm)].map((match) => match[1]!);

function textOf(children: ReactNode): string {
  return Children.toArray(children)
    .map((child) => {
      if (typeof child === "string" || typeof child === "number") {
        return String(child);
      }
      if (isValidElement<{ children?: ReactNode }>(child)) {
        return textOf(child.props.children);
      }
      return "";
    })
    .join("");
}

function jumpTo(event: MouseEvent<HTMLAnchorElement>, id: string) {
  event.preventDefault();
  document.getElementById(id)?.scrollIntoView?.({
    behavior: "smooth",
    block: "start",
  });
}

function helpComponents(onOpen?: (target: HelpOpen) => void): Components {
  const heading = (Tag: "h1" | "h2" | "h3") =>
    function HelpHeading({ children }: { children?: ReactNode }) {
      return <Tag id={textOf(children).trim()}>{children}</Tag>;
    };
  return {
    h1: heading("h1"),
    h2: heading("h2"),
    h3: heading("h3"),
    a({ href, children }) {
      if (href?.startsWith("#")) {
        const id = decodeURIComponent(href.slice(1));
        return (
          <a href={href} onClick={(event) => jumpTo(event, id)}>
            {children}
          </a>
        );
      }
      const target = href?.startsWith("/app/") ? href.slice(5) : "";
      if (TARGETS.has(target)) {
        if (!onOpen) return <span>{children}</span>;
        return (
          <button
            type="button"
            className="help-link"
            onClick={() => onOpen(target as HelpOpen)}
          >
            {children}
          </button>
        );
      }
      if (!href) return <span>{children}</span>;
      return (
        <a href={href} rel="noreferrer">
          {children}
        </a>
      );
    },
  };
}

export function HelpPage({ onOpen }: { onOpen?(target: HelpOpen): void }) {
  const root = useRef<HTMLElement>(null);
  const [active, setActive] = useState(TOC[0] ?? "");
  const components = helpComponents(onOpen);

  useEffect(() => {
    const page = root.current;
    const scroller = page?.closest(".settings-content");
    if (!(scroller instanceof HTMLElement)) return;

    const sync = () => {
      const line = scroller.getBoundingClientRect().top + 28;
      let current = TOC[0] ?? "";
      for (const id of TOC) {
        const heading = document.getElementById(id);
        if (!heading) continue;
        if (heading.getBoundingClientRect().top <= line) current = id;
      }
      if (
        scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight <
        8
      ) {
        current = TOC.at(-1) ?? current;
      }
      setActive(current);
    };

    sync();
    scroller.addEventListener("scroll", sync, { passive: true });
    return () => scroller.removeEventListener("scroll", sync);
  }, []);

  return (
    <article className="help-page" ref={root}>
      <div className="help-article">
        <Markdown remarkPlugins={[remarkGfm]} components={components}>
          {source}
        </Markdown>
      </div>
      <nav className="help-toc" aria-label="目录">
        <p className="help-toc-label">目录</p>
        <ol>
          {TOC.map((id) => (
            <li key={id}>
              <a
                href={`#${id}`}
                className={active === id ? "is-active" : undefined}
                aria-current={active === id ? "location" : undefined}
                onClick={(event) => {
                  jumpTo(event, id);
                  setActive(id);
                }}
              >
                {id}
              </a>
            </li>
          ))}
        </ol>
      </nav>
    </article>
  );
}
