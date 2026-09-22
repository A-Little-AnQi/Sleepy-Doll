import { useId, useState } from "react";
import { formatTokens } from "../../session/context-usage";
import "./ContextMeter.css";
import { useT } from "../../i18n";

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
        viewBox="0 0 28 28"
        role="progressbar"
        aria-label={t.context.inModel}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={safeWindow > 0 ? percent : undefined}
        aria-valuetext={usage}
      >
        <circle className="sd-context-track" cx="14" cy="14" r="10" />
        <circle
          className="sd-context-fill"
          cx="14"
          cy="14"
          r="10"
          pathLength="100"
          strokeDasharray={`${ratio * 100} 100`}
        />
        <circle className="sd-context-center" cx="14" cy="14" r="2" />
      </svg>
      <span className="sd-context-copy">
        <span className="sd-context-label">
          Tokens <span>{safeWindow > 0 ? `${percent}%` : "—"}</span>
        </span>
        <span className="sd-context-numbers">{usage}</span>
      </span>
      <span className="sd-context-hint" id={hintId} role="tooltip">
        <span>{compacted ? t.context.compacted : t.context.inModel}</span>
        {cacheHit > 0 && Number.isFinite(cacheHit) ? (
          <em>{t.context.cacheHit(formatTokens(cacheHit))}</em>
        ) : null}
      </span>
      {compacted && (
        <span
          className="sd-context-compacted"
          aria-label={t.context.compactedShort}
          title={t.context.compactedShort}
        />
      )}
    </div>
  );
}
