import markup from "./wordmark.svg?raw";
import { SvgAsset } from "./SvgAsset";

type IconProps = { className?: string };

/** 产品字标。图形在 `wordmark.svg`，可直接打开。 */
export function Wordmark({ className }: IconProps) {
  return (
    <SvgAsset
      markup={markup}
      idPrefix="brand-wordmark-"
      className={className ?? "app-wordmark"}
    />
  );
}
