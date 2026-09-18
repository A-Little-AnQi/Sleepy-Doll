/** Shared 24×24 stroke icons. They inherit `currentColor` so callers control the
 * colour, and they are always paired with a visible text label — an icon alone
 * tells a first-time user nothing. */
type IconProps = { className?: string };

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
