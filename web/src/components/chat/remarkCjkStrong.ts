import type { Parent, Root } from "mdast";

const guard = "\u2060";
type Range = { start: number; end: number };

function escaped(text: string, at: number) {
  let slashes = 0;
  while (at > 0 && text[--at] === "\\") slashes += 1;
  return slashes % 2 === 1;
}

function protectedRanges(text: string) {
  const ranges: Range[] = [];
  let fence: { char: string; size: number; start: number } | undefined;
  let offset = 0;
  for (const line of text.split(/(?<=\n)/)) {
    const fenceMatch = /^[ \t]{0,3}(?:>[ \t]*)*(`{3,}|~{3,})/.exec(line);
    const marker = fenceMatch?.[1];
    if (fence) {
      if (
        marker?.[0] === fence.char &&
        marker.length >= fence.size &&
        /^[ \t\r\n]*$/.test(line.slice(fenceMatch![0].length))
      ) {
        ranges.push({ start: fence.start, end: offset + line.length });
        fence = undefined;
      }
    } else if (marker) {
      fence = { char: marker[0]!, size: marker.length, start: offset };
    } else if (/^(?: {4}|\t)/.test(line)) {
      ranges.push({ start: offset, end: offset + line.length });
    }
    offset += line.length;
  }
  if (fence) ranges.push({ start: fence.start, end: text.length });
  for (let at = 0; at < text.length; at += 1) {
    const block = ranges.find((range) => at >= range.start && at < range.end);
    if (block) {
      at = block.end - 1;
      continue;
    }
    if (text[at] !== "`" || escaped(text, at)) continue;
    let end = at + 1;
    while (text[end] === "`") end += 1;
    const size = end - at;
    let next = end;
    while ((next = text.indexOf("`", next)) >= 0) {
      let close = next + 1;
      while (text[close] === "`") close += 1;
      if (close - next === size) {
        ranges.push({ start: at, end: close });
        at = close - 1;
        break;
      }
      next = close;
    }
  }
  for (const match of text.matchAll(
    /\]\((?:\\.|[^)\n])*\)|<(?:https?:\/\/|mailto:)[^>\n]*>/g,
  ))
    ranges.push({ start: match.index, end: match.index + match[0].length });
  return ranges;
}

/** 仅在显示副本中修正中文标点相邻的粗体分隔符。 */
export function prepareCjkStrong(text: string) {
  const ranges = protectedRanges(text);
  const markers = [...text.matchAll(/(?<!\*)\*\*(?!\*)/g)]
    .map((match) => match.index)
    .filter(
      (at) =>
        !escaped(text, at) &&
        !ranges.some((range) => at >= range.start && at < range.end),
    );
  const starts = new Set<number>();
  let output = "";
  let cursor = 0;
  for (let i = 0; i + 1 < markers.length; i += 2) {
    const start = markers[i]!;
    const end = markers[i + 1]!;
    const content = text.slice(start + 2, end);
    if (
      content !== content.trim() ||
      content.includes("\n") ||
      !/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}「」『』（）【】《》〈〉“”‘’]/u.test(
        content,
      ) ||
      !/(?:^\p{P}|\p{P}$)/u.test(content)
    )
      continue;
    output += text.slice(cursor, start);
    starts.add(output.length);
    output += `**${guard}${content}${guard}**`;
    cursor = end + 2;
  }
  return { text: output + text.slice(cursor), starts };
}

/** 解析后移除本次插入的排版标记，保留原始内容。 */
export function remarkCjkStrong(starts: ReadonlySet<number>) {
  return (tree: Root) => {
    const visit = (parent: Parent) => {
      for (const child of parent.children) {
        if (
          child.type === "strong" &&
          starts.has(child.position?.start.offset ?? -1)
        ) {
          const first = child.children[0];
          const last = child.children.at(-1);
          if (first?.type === "text" && first.value.startsWith(guard))
            first.value = first.value.slice(1);
          if (last?.type === "text" && last.value.endsWith(guard))
            last.value = last.value.slice(0, -1);
        }
        if ("children" in child) visit(child as Parent);
      }
    };
    visit(tree);
  };
}
