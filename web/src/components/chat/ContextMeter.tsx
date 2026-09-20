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
  const ratio = window > 0 ? Math.min(1, used / window) : 0;
  const high = ratio >= 0.85;
  const title = [
    compacted ? t.context.compacted : t.context.inModel,
    cacheHit > 0 ? t.context.cacheHit(formatTokens(cacheHit)) : "",
  ]
    .filter(Boolean)
    .join("；");
  return (
    <div className={`sd-context${high ? " is-high" : ""}`} title={title}>
      <span className="sd-context-track" aria-hidden="true">
        <span
          className="sd-context-fill"
          style={{ width: `${ratio * 100}%` }}
        />
      </span>
      <span className="sd-context-copy">
        {formatTokens(used)} / {formatTokens(window)}
      </span>
      {cacheHit > 0 ? <em>缓存 {formatTokens(cacheHit)}</em> : null}
      {compacted ? <em>已压缩</em> : null}
    </div>
  );
}
