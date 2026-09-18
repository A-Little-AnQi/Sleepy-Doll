import { useId } from "react";

/** Shared 24×24 stroke icons. They inherit `currentColor` so callers control the
 * colour, and they are always paired with a visible text label — an icon alone
 * tells a first-time user nothing. `BrandIcon` is the product moon mark and
 * carries its own fills. */
type IconProps = { className?: string };

function markId(prefix: string) {
  const raw = useId();
  return `${prefix}${raw.replace(/[^a-zA-Z0-9]/g, "")}`;
}

/** 产品图标：月牙与嵌在开口里的金色四角星。对外界面都用这一枚。 */
export function BrandIcon({ className }: IconProps) {
  const uid = markId("bm");
  return (
    <svg
      className={className ? `app-brand-icon ${className}` : "app-brand-icon"}
      viewBox="0 0 22 22"
      overflow="visible"
      aria-hidden
    >
      <MoonGraphic uid={uid} />
    </svg>
  );
}
export function SidebarIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <path d="M9 4v16" />
    </Svg>
  );
}
export function CloseIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m6 6 12 12M18 6 6 18" />
    </Svg>
  );
}
export function SearchIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m16 16 4 4" />
    </Svg>
  );
}

function Svg({
  className,
  children,
}: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function ChatIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4 5.5h16v11H9l-5 4z" />
    </Svg>
  );
}

export function HistoryIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4 6h16M4 12h16M4 18h10" />
    </Svg>
  );
}

export function BridgeIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M9 7V4M15 7V4M7 7h10v5a5 5 0 0 1-10 0zM12 17v4" />
    </Svg>
  );
}

export function ModelIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9z" />
      <path d="m4 7.5 8 4.5 8-4.5M12 12v9" />
    </Svg>
  );
}

export function PluginIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M10 4a2 2 0 1 1 4 0v2h4v4h2a2 2 0 1 1 0 4h-2v4H8v-2a2 2 0 1 0-4 0v2H4V8h4z" />
    </Svg>
  );
}

export function SettingsIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6" />
    </Svg>
  );
}

export function HelpIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M9.5 9.4a2.5 2.5 0 1 1 3.7 2.2c-.8.5-1.2 1-1.2 1.9" />
      <path d="M12 17.2h.01" />
    </Svg>
  );
}

export function SunIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 3.2v1.8M12 19v1.8M3.2 12h1.8M19 12h1.8M5.4 5.4l1.3 1.3M17.3 17.3l1.3 1.3M18.6 5.4l-1.3 1.3M6.7 17.3l-1.3 1.3" />
    </Svg>
  );
}

export function MoonIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M16.2 14.2A6.4 6.4 0 0 1 9.4 5.8 5.6 5.6 0 1 0 16.2 14.2Z" />
    </Svg>
  );
}

export function ChevronIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m14 6-6 6 6 6" />
    </Svg>
  );
}

export function FolderIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3.6 8h5.2l1.7 2.2H20.4v9.2H3.6Z" />
      <path d="M3.6 8V6.4A1.6 1.6 0 0 1 5.2 4.8h4.1L11 6.8" />
    </Svg>
  );
}

export function PlusIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M12 5v14M5 12h14" />
    </Svg>
  );
}

export function SendIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M12 20V5M6 11l6-6 6 6" />
    </Svg>
  );
}

export function RefreshIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M20 12a8 8 0 1 1-2.6-5.9" />
      <path d="M20 4v4h-4" />
    </Svg>
  );
}

export function StopIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="7" y="7" width="10" height="10" rx="1.5" />
    </Svg>
  );
}

export function AlertIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M12 4 2.5 20h19z" />
      <path d="M12 10v4.5M12 17.4v.2" />
    </Svg>
  );
}

export function CheckIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </Svg>
  );
}

export function CopyIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="8.2" y="8.2" width="11.3" height="12.3" rx="1.8" />
      <path d="M15.5 8.2V6.2A1.7 1.7 0 0 0 13.8 4.5H6.2A1.7 1.7 0 0 0 4.5 6.2v12.1A1.7 1.7 0 0 0 6.2 20" />
    </Svg>
  );
}

export function MoreIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="5.5" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="18.5" r="1.4" fill="currentColor" stroke="none" />
    </Svg>
  );
}

export function PinIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M9 3.5h6l-.7 5.2 3.2 3.1H6.5l3.2-3.1Z" />
      <path d="M12 11.8V20.5" />
    </Svg>
  );
}

export function ArchiveIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3.5 5.5h17v3.6h-17z" />
      <path d="M5.2 9.1v9.4h13.6V9.1M10 12.6h4" />
    </Svg>
  );
}

export function PlayIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M8 5.5 18.5 12 8 18.5Z" />
    </Svg>
  );
}

export function ToolIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M14.7 6.3a4 4 0 0 0 5 5L20 11v6.5a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 6 17.5V8.5A2.5 2.5 0 0 1 8.5 6h6Z" />
      <path d="M13.5 3.9 17 7.4" />
    </Svg>
  );
}

export function PanelIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3.5" y="5" width="17" height="14" rx="2" />
      <path d="M15 5v14" />
    </Svg>
  );
}

export function TrashIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4.5 6.5h15M9.5 6.5V4.8h5v1.7" />
      <path d="M6.3 6.5 7.2 20h9.6l.9-13.5M10.4 10v6.4M13.6 10v6.4" />
    </Svg>
  );
}

export function EditIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4.5 19.5h4L19 9a2.1 2.1 0 0 0-3-3L5.5 16.5Z" />
      <path d="m14.5 7.5 2.5 2.5" />
    </Svg>
  );
}

function sparkle(cx: number, cy: number, r: number, longR = r) {
  const i = r * 0.3;
  const arm = r * 0.24;
  const pts: [number, number][] = [
    [cx + longR, cy],
    [cx + i, cy - arm],
    [cx, cy - r],
    [cx - i, cy - i],
    [cx - r, cy],
    [cx - i, cy + i],
    [cx, cy + r],
    [cx + i, cy + arm],
  ];
  const round = Math.min(r, longR) * 0.28;
  const toward = (px: number, py: number, tx: number, ty: number) => {
    const dx = px - tx;
    const dy = py - ty;
    const len = Math.hypot(dx, dy) || 1;
    return [tx + (dx / len) * round, ty + (dy / len) * round] as const;
  };
  let d = "";
  for (let t = 0; t < 4; t += 1) {
    const k = t * 2;
    const [tx, ty] = pts[k];
    const prev = pts[(k + 7) % 8];
    const next = pts[(k + 1) % 8];
    const [ax, ay] = toward(prev[0], prev[1], tx, ty);
    const [bx, by] = toward(next[0], next[1], tx, ty);
    d += t === 0 ? `M${ax} ${ay}` : `L${ax} ${ay}`;
    d += `Q${tx} ${ty} ${bx} ${by}L${next[0]} ${next[1]}`;
  }
  return `${d}Z`;
}

const MOON_D =
  "M17.71 5.01A9 9 0 1 0 17.71 16.99A6.5 6.5 0 1 1 17.71 5.01Z";

function MoonGraphic({ uid }: { uid: string }) {
  const gold = `url(#${uid}gold)`;
  const star = sparkle(16.6, 11, 5.1, 7.4);
  return (
    <>
      <defs>
        <radialGradient
          id={`${uid}form`}
          gradientUnits="userSpaceOnUse"
          cx="13.2"
          cy="8.6"
          r="11.2"
        >
          <stop offset="0" stopColor="#fff" stopOpacity="0.38" />
          <stop offset="0.26" stopColor="#fff" stopOpacity="0.12" />
          <stop offset="0.58" stopColor="#000" stopOpacity="0.05" />
          <stop offset="1" stopColor="#000" stopOpacity="0.28" />
        </radialGradient>
        <radialGradient
          id={`${uid}spec`}
          gradientUnits="userSpaceOnUse"
          cx="10.1"
          cy="7.4"
          r="3.8"
        >
          <stop offset="0" stopColor="#fff" stopOpacity="0.55" />
          <stop offset="0.4" stopColor="#fff" stopOpacity="0.14" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
        <clipPath id={`${uid}clip`}>
          <path d={MOON_D} />
        </clipPath>
        <linearGradient id={`${uid}gold`} x1="0" y1="0.5" x2="1" y2="0.5">
          <stop offset="0" stopColor="var(--gold)" />
          <stop offset="0.55" stopColor="var(--gold-bright)" />
          <stop offset="1" stopColor="var(--gold-bright)" />
        </linearGradient>
        <linearGradient id={`${uid}sheen`} x1="0.5" y1="0" x2="0.5" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.4" />
          <stop offset="0.38" stopColor="#fff" stopOpacity="0" />
          <stop offset="1" stopColor="#000" stopOpacity="0.18" />
        </linearGradient>
        <filter
          id={`${uid}glow`}
          x="-80%"
          y="-80%"
          width="260%"
          height="260%"
          colorInterpolationFilters="sRGB"
        >
          <feGaussianBlur stdDeviation="1.35" />
        </filter>
      </defs>
      <g transform="rotate(-35 11 11)">
        <path
          d={star}
          fill="var(--gold-bright)"
          opacity="0.5"
          filter={`url(#${uid}glow)`}
        />
        <path d={MOON_D} fill="currentColor" />
        <path d={MOON_D} fill={`url(#${uid}form)`} />
        <path d={MOON_D} fill={`url(#${uid}spec)`} />
        <g
          clipPath={`url(#${uid}clip)`}
          fill="none"
          stroke="#000"
          strokeOpacity="0.28"
          strokeWidth="0.32"
          strokeLinejoin="round"
          strokeLinecap="round"
        >
          <path
            d={sparkle(12.2, 4.2, 3.1)}
            transform="rotate(18 12.2 4.2)"
          />
          <path
            d={sparkle(2.6, 10.5, 3.5)}
            transform="rotate(-24 2.6 10.5)"
          />
          <path
            d={sparkle(9.8, 13.8, 2.8)}
            transform="rotate(38 9.8 13.8)"
          />
          <path
            d={sparkle(14.2, 16.8, 2.3)}
            transform="rotate(-10 14.2 16.8)"
          />
        </g>
        <path
          d={star}
          fill={gold}
          stroke={gold}
          strokeWidth="0.85"
          strokeLinejoin="round"
          strokeLinecap="round"
        />
        <path d={star} fill={`url(#${uid}sheen)`} />
      </g>
    </>
  );
}

/** 产品字标：品牌图标 + Sleepy Doll。 */
export function Wordmark({ className }: IconProps) {
  const uid = markId("wm");
  const fill = `url(#${uid}g)`;
  return (
    <svg
      className={className ?? "app-wordmark"}
      viewBox="0 0 102 22"
      preserveAspectRatio="xMinYMid meet"
      overflow="visible"
      role="img"
      aria-label="Sleepy Doll"
    >
      <title>Sleepy Doll</title>
      <defs>
        <linearGradient id={`${uid}g`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="currentColor" />
          <stop offset="0.45" stopColor="currentColor" stopOpacity="0.92" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0.68" />
        </linearGradient>
      </defs>
      <MoonGraphic uid={uid} />
      <g fill={fill} transform="translate(26.2,0)">
          <path d="M0.60 15.26L0.60 13.29Q1.14 13.74 1.77 13.97Q2.39 14.19 3.03 14.19Q3.41 14.19 3.69 14.12Q3.97 14.06 4.16 13.94Q4.34 13.82 4.44 13.65Q4.53 13.49 4.53 13.30Q4.53 13.04 4.38 12.84Q4.23 12.63 3.98 12.46Q3.72 12.29 3.37 12.13Q3.02 11.97 2.61 11.80Q1.58 11.37 1.07 10.75Q0.57 10.13 0.57 9.25Q0.57 8.56 0.84 8.07Q1.12 7.57 1.60 7.25Q2.07 6.93 2.70 6.78Q3.33 6.63 4.03 6.63Q4.72 6.63 5.25 6.71Q5.78 6.80 6.23 6.97L6.23 8.81Q6.01 8.65 5.75 8.54Q5.49 8.42 5.21 8.34Q4.93 8.27 4.66 8.23Q4.39 8.19 4.14 8.19Q3.80 8.19 3.53 8.26Q3.25 8.32 3.06 8.44Q2.87 8.56 2.76 8.72Q2.66 8.88 2.66 9.08Q2.66 9.31 2.77 9.48Q2.89 9.66 3.11 9.81Q3.32 9.97 3.63 10.12Q3.94 10.27 4.33 10.43Q4.85 10.65 5.28 10.90Q5.70 11.15 6 11.47Q6.30 11.78 6.46 12.18Q6.62 12.59 6.62 13.12Q6.62 13.86 6.34 14.36Q6.06 14.86 5.58 15.17Q5.10 15.48 4.46 15.62Q3.83 15.75 3.12 15.75Q2.39 15.75 1.74 15.63Q1.08 15.51 0.60 15.26" />
          <path d="M10.58 6.27L10.58 15.60L8.64 15.60L8.64 6.27" />
          <path d="M18.63 12.19L18.63 13L14.52 13Q14.62 14.38 16.25 14.38Q17.29 14.38 18.08 13.88L18.08 15.29Q17.21 15.75 15.81 15.75Q14.28 15.75 13.44 14.91Q12.60 14.06 12.60 12.55Q12.60 10.98 13.51 10.06Q14.42 9.15 15.75 9.15Q17.13 9.15 17.88 9.96Q18.63 10.78 18.63 12.19M14.51 11.81L16.83 11.81Q16.83 10.46 15.74 10.46Q15.27 10.46 14.93 10.84Q14.59 11.23 14.51 11.81" />
          <path d="M26.21 12.19L26.21 13L22.10 13Q22.20 14.38 23.83 14.38Q24.87 14.38 25.65 13.88L25.65 15.29Q24.78 15.75 23.38 15.75Q21.86 15.75 21.01 14.91Q20.17 14.06 20.17 12.55Q20.17 10.98 21.08 10.06Q21.99 9.15 23.32 9.15Q24.70 9.15 25.45 9.96Q26.21 10.78 26.21 12.19M22.08 11.81L24.40 11.81Q24.40 10.46 23.31 10.46Q22.84 10.46 22.50 10.84Q22.16 11.23 22.08 11.81" />
          <path d="M30.09 14.87L30.06 14.87L30.06 18.50L28.12 18.50L28.12 9.30L30.06 9.30L30.06 10.25L30.09 10.25Q30.81 9.15 32.11 9.15Q33.34 9.15 34 9.99Q34.67 10.83 34.67 12.27Q34.67 13.85 33.89 14.80Q33.12 15.75 31.82 15.75Q30.68 15.75 30.09 14.87M30.03 12.28L30.03 12.79Q30.03 13.44 30.38 13.85Q30.72 14.26 31.28 14.26Q31.95 14.26 32.31 13.75Q32.68 13.24 32.68 12.30Q32.68 10.64 31.39 10.64Q30.79 10.64 30.41 11.09Q30.03 11.54 30.03 12.28" />
          <path d="M40.76 9.30L42.69 9.30L40.13 16.10Q39.21 18.56 37.35 18.56Q36.64 18.56 36.18 18.40L36.18 16.85Q36.57 17.08 37.03 17.08Q37.78 17.08 38.07 16.37L38.41 15.59L35.85 9.30L38 9.30L39.17 13.13Q39.29 13.49 39.35 13.98L39.37 13.98Q39.43 13.62 39.57 13.15" />
          <path d="M51.57 15.60L48.45 15.60L48.45 6.78L51.57 6.78Q56.27 6.78 56.27 11.08Q56.27 13.14 54.99 14.37Q53.71 15.60 51.57 15.60M51.41 8.40L50.43 8.40L50.43 13.99L51.42 13.99Q52.71 13.99 53.45 13.21Q54.18 12.44 54.18 11.10Q54.18 9.84 53.45 9.12Q52.72 8.40 51.41 8.40" />
          <path d="M61.31 15.75Q59.73 15.75 58.83 14.87Q57.93 13.99 57.93 12.47Q57.93 10.91 58.86 10.03Q59.80 9.15 61.39 9.15Q62.96 9.15 63.85 10.03Q64.74 10.91 64.74 12.36Q64.74 13.93 63.82 14.84Q62.90 15.75 61.31 15.75M61.35 10.64Q60.67 10.64 60.28 11.11Q59.90 11.58 59.90 12.45Q59.90 14.26 61.37 14.26Q62.76 14.26 62.76 12.40Q62.76 10.64 61.35 10.64" />
          <path d="M68.71 6.27L68.71 15.60L66.76 15.60L66.76 6.27" />
          <path d="M73.04 6.27L73.04 15.60L71.10 15.60L71.10 6.27" />
        </g>
    </svg>
  );
}
