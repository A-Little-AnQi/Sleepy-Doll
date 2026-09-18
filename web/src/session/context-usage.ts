/** 粗略 token 估算，与运行时 `context::estimate_tokens` 同口径。 */
export function estimateTokens(text: string) {
  let ascii = 0;
  let wide = 0;
  for (const char of text) {
    if (char.charCodeAt(0) < 128) ascii += 1;
    else wide += 1;
  }
  return Math.ceil(ascii / 4) + wide;
}

export function estimateMessagesTokens(
  messages: ReadonlyArray<{ content?: string | null }>,
  extra: ReadonlyArray<string> = [],
) {
  return (
    messages.reduce(
      (sum, message) => sum + estimateTokens(message.content ?? "") + 8,
      0,
    ) + extra.reduce((sum, text) => sum + estimateTokens(text), 0)
  );
}

export function formatTokens(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 10_000) return `${Math.round(value / 1000)}k`;
  if (value >= 1000) return `${(value / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  return String(Math.max(0, Math.round(value)));
}
