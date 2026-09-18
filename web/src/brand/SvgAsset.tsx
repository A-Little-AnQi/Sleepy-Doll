import { useId } from "react";

/** 把品牌 SVG 原文嵌进文档。磁盘上的 id 方便直接打开预览；进页面后换成实例 id，避免两枚图标撞名。 */
export function SvgAsset({
  markup,
  idPrefix,
  className,
}: {
  markup: string;
  idPrefix: string;
  className?: string;
}) {
  const uid = `bm${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const html = markup
    .trim()
    .replaceAll(`id="${idPrefix}`, `id="${uid}`)
    .replaceAll(`url(#${idPrefix}`, `url(#${uid}`)
    .replace(/>\s+</g, "><")
    .replace(/<svg\b([^>]*)>/, (_match, attrs: string) => {
      const cleaned = attrs.replace(/\sclass="[^"]*"/, "");
      return `<svg${cleaned}${className ? ` class="${className}"` : ""}>`;
    });
  return (
    <span
      style={{ display: "contents" }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
