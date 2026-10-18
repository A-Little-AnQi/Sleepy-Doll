import markup from "./icon.svg?raw";
import { SvgAsset } from "./SvgAsset";

type IconProps = { className?: string };

/** 产品图标。图形在 `icon.svg`，可直接打开。 */
export function BrandIcon({ className }: IconProps) {
  return (
    <SvgAsset
      markup={markup}
      idPrefix="brand-icon-"
      className={className ? `app-brand-icon ${className}` : "app-brand-icon"}
    />
  );
}
