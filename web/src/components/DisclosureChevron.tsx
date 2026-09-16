import { type AnimationEvent, useLayoutEffect, useRef, useState } from "react";
import "./DisclosureChevron.css";

type Motion = "opening" | "closing" | null;

/** 下拉开合时左右笔画拉开再合上，抄自 VoiceRoom DisclosureChevron。 */
export function DisclosureChevron({
  expanded,
  className,
}: {
  expanded: boolean;
  className?: string;
}) {
  const previous = useRef(expanded);
  const [motion, setMotion] = useState<Motion>(null);

  useLayoutEffect(() => {
    if (previous.current === expanded) return;
    previous.current = expanded;
    setMotion(expanded ? "opening" : "closing");
  }, [expanded]);

  const finish = (event: AnimationEvent<SVGPathElement>) => {
    if (event.currentTarget.dataset.stroke === "left") setMotion(null);
  };

  return (
    <svg
      className={["sd-chevron", className].filter(Boolean).join(" ")}
      viewBox="0 0 20 20"
      width="16"
      height="16"
      fill="none"
      aria-hidden="true"
      data-expanded={expanded ? "true" : "false"}
      data-motion={motion ?? undefined}
    >
      <path
        className="sd-chevron-stroke sd-chevron-joined"
        d="M4.8 6.8 8.85 10.15 Q10 11.9 11.15 10.15 L15.2 6.8"
      />
      <path
        className="sd-chevron-stroke sd-chevron-motion sd-chevron-left"
        data-stroke="left"
        d="M4.8 6.8 10 11.05"
        onAnimationEnd={finish}
      />
      <path
        className="sd-chevron-stroke sd-chevron-motion sd-chevron-right"
        data-stroke="right"
        d="M15.2 6.8 10 11.05"
      />
    </svg>
  );
}
