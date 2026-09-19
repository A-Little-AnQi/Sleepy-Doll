import { formatTokens } from "../../session/context-usage";
import "./ContextMeter.css";

export function ContextMeter({
  used,
  window,
  compacted = false,
  cacheRead = 0,
}: {
  used: number;
  window: number;
  compacted?: boolean;
  cacheRead?: number;
}) {
  const ratio = window > 0 ? Math.min(1, used / window) : 0;
  const high = ratio >= 0.85;
  const title = [
    compacted
      ? "已压缩较早上下文；完整记录仍保存在本机"
      : "当前装进模型的上下文",
    cacheRead > 0 ? `缓存命中 ${formatTokens(cacheRead)}` : "",
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
      {cacheRead > 0 ? <em>缓存 {formatTokens(cacheRead)}</em> : null}
      {compacted ? <em>已压缩</em> : null}
    </div>
  );
}
