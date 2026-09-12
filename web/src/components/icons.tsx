/** Shared 24×24 stroke icons. They inherit `currentColor` so callers control the
 * colour, and they are always paired with a visible text label — an icon alone
 * tells a first-time user nothing. */
type IconProps = { className?: string };

export function BrandIcon({ className }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M23.8 21.7A11.4 11.4 0 0 1 10.3 8.2 11.5 11.5 0 1 0 23.8 21.7Z"
        fill="currentColor"
      />
      <path
        d="m23 5 .9 3.1L27 9l-3.1.9L23 13l-.9-3.1L19 9l3.1-.9Z"
        fill="currentColor"
      />
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

export function ChevronIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m14 6-6 6 6 6" />
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
