import "./DisclosureChevron.css";

/** 折叠指示箭头：收起朝右，展开整体旋转 90° 朝下。 */
export function DisclosureChevron({
  expanded,
  className,
}: {
  expanded: boolean;
  className?: string;
}) {
  return (
    <svg
      className={["sd-chevron", className].filter(Boolean).join(" ")}
      viewBox="0 0 16 16"
      width="14"
      height="14"
      fill="none"
      aria-hidden="true"
      data-expanded={expanded ? "true" : "false"}
    >
      <path d="M6 4 L10.6 8 L6 12" />
    </svg>
  );
}
