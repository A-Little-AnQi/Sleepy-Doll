import { useId, useState } from "react";
import { formatTokens } from "../../session/context-usage";
import "./ContextMeter.css";
import { useT } from "../../i18n";
import { useLocale } from "../../appearance/locale";

export function ContextMeter({
  used,
  window,
  compacted = false,
  cacheHit = 0,
}: {
  used: number;
  window: number;
  compacted?: boolean;
  cacheHit?: number;
}) {
  const t = useT();
  const english = useLocale() === "en";
  const hintId = useId();
  const [hintDismissed, setHintDismissed] = useState(false);
  const safeUsed = Number.isFinite(used) ? Math.max(0, used) : 0;
  const safeWindow = Number.isFinite(window) ? Math.max(0, window) : 0;
  const ratio = safeWindow > 0 ? Math.min(1, safeUsed / safeWindow) : 0;
  const percent = Math.round(ratio * 100);
  const usage = `${formatTokens(safeUsed)} / ${safeWindow > 0 ? formatTokens(safeWindow) : "—"}`;
  return (
    <div
      className={`sd-context${ratio >= 0.85 ? " is-high" : ""}`}
      tabIndex={0}
      role="group"
      aria-label={`Tokens · ${usage}`}
      aria-describedby={hintId}
      data-hint-dismissed={hintDismissed}
      onFocus={() => setHintDismissed(false)}
      onMouseEnter={() => setHintDismissed(false)}
      onKeyDown={(event) => {
        if (event.key === "Escape") setHintDismissed(true);
      }}
    >
      <svg
        className="sd-context-ring"
        viewBox="0 0 20 20"
        role="progressbar"
        aria-label={t.context.inModel}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={safeWindow > 0 ? percent : undefined}
        aria-valuetext={usage}
      >
        <circle className="sd-context-track" cx="10" cy="10" r="8" />
        <circle
          className="sd-context-fill"
          cx="10"
          cy="10"
          r="8"
          pathLength="100"
          strokeDasharray={`${ratio * 100} 100`}
          opacity={ratio > 0 ? 1 : 0}
        />
      </svg>
      <span className="sd-context-hint" id={hintId} role="tooltip">
        <span className="sd-context-hint-title">
          <span>{english ? "Context usage" : "上下文用量"}</span>
          <span>{safeWindow > 0 ? `${percent}%` : "—"}</span>
        </span>
        <span className="sd-context-hint-row">
          <span>{english ? "Used / limit" : "已用 / 总量"}</span>
          <span>{usage}</span>
        </span>
        {cacheHit > 0 && Number.isFinite(cacheHit) ? (
          <span className="sd-context-hint-row">
            <span>{english ? "Cache hit" : "缓存命中"}</span>
            <span>{formatTokens(cacheHit)}</span>
          </span>
        ) : null}
        {compacted ? (
          <span className="sd-context-hint-note">{t.context.compacted}</span>
        ) : null}
      </span>
    </div>
  );
}
