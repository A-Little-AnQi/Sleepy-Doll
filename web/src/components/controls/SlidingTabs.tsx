import { useLayoutEffect, useRef, type ReactNode } from "react";
import "./SlidingTabs.css";

export function SlidingTabs<T extends string>({
  items,
  value,
  onChange,
  ariaLabel,
}: {
  items: ReadonlyArray<{
    id: T;
    name: string;
    icon?: ReactNode;
    extra?: ReactNode;
  }>;
  value: T;
  /** T 只由 items 与 value 推导；不加 NoInfer 时，直接传入的 setState 会把它推导成 string。 */
  onChange(id: NoInfer<T>): void;
  ariaLabel: string;
}) {
  const root = useRef<HTMLElement>(null);
  const pill = useRef<HTMLSpanElement>(null);
  const from = useRef<DOMRect | null>(null);
  const last = useRef(value);

  useLayoutEffect(() => {
    const list = root.current;
    const marker = pill.current;
    const active = list?.querySelector<HTMLElement>("[data-tab-active='true']");
    if (!list || !marker || !active) return;
    const next = active.getBoundingClientRect();
    const origin = list.getBoundingClientRect();
    const left = next.left - origin.left;
    const width = next.width;
    const prev = from.current;
    const moved = last.current !== value;
    last.current = value;
    from.current = next;
    marker.style.width = `${width}px`;
    marker.style.transform = `translateX(${left}px)`;
    if (!prev || !moved) {
      marker.dataset.ready = "true";
      return;
    }
    const delta = prev.left - next.left;
    const scaleX = Math.max(0.42, prev.width / Math.max(width, 1));
    marker.style.transformOrigin = `${delta > 0 ? "right" : "left"} center`;
    if (typeof marker.getAnimations === "function") {
      for (const animation of marker.getAnimations()) animation.cancel();
    }
    if (typeof marker.animate !== "function") return;
    marker.animate(
      [
        {
          transform: `translateX(${left + delta}px) scaleX(${scaleX}) scaleY(0.92)`,
          offset: 0,
        },
        {
          transform: `translateX(${left + delta * 0.28}px) scaleX(${Math.max(scaleX, 1.16)}) scaleY(0.86)`,
          offset: 0.36,
        },
        {
          transform: `translateX(${left - delta * 0.06}px) scaleX(0.94) scaleY(1.08)`,
          offset: 0.72,
        },
        {
          transform: `translateX(${left}px) scaleX(1) scaleY(1)`,
          offset: 1,
        },
      ],
      {
        duration: 720,
        easing: "cubic-bezier(0.22, 1.18, 0.36, 1)",
      },
    );
  }, [value, items]);

  return (
    <nav ref={root} className="sd-tabs" aria-label={ariaLabel}>
      <span className="sd-tabs-pill" ref={pill} aria-hidden="true" />
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={value === item.id ? "is-active" : ""}
          data-tab-active={value === item.id ? "true" : undefined}
          aria-current={value === item.id ? "page" : undefined}
          onClick={() => onChange(item.id)}
        >
          {item.icon ? (
            <span className="sd-tabs-icon" aria-hidden="true">
              {item.icon}
            </span>
          ) : null}
          {item.name}
          {item.extra}
        </button>
      ))}
    </nav>
  );
}
