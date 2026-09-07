import type { ButtonHTMLAttributes, ReactNode } from "react";

import "./sleepy-action-buttons.css";

type NativeButtonProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "children"
>;

interface ActionButtonProps extends NativeButtonProps {
  label: string;
  active?: boolean;
  status?: "idle" | "working" | "success";
  children: ReactNode;
  variant: "moon" | "scan" | "route" | "send";
}

function ActionButton({
  label,
  active = false,
  status = "idle",
  variant,
  className,
  children,
  type = "button",
  ...props
}: ActionButtonProps) {
  return (
    <button
      className={`sd-action-button sd-action-${variant} ${className ?? ""}`}
      type={type}
      aria-label={label}
      aria-pressed={
        variant === "moon" || variant === "route" ? active : undefined
      }
      data-active={active}
      data-status={status}
      title={label}
      {...props}
    >
      {children}
    </button>
  );
}

function ArtSvg({ children }: { children: ReactNode }) {
  return (
    <svg
      className="sd-action-art"
      viewBox="0 0 24 24"
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export interface MoonPhaseButtonProps extends NativeButtonProps {
  label?: string;
  active?: boolean;
}

export function MoonPhaseButton({
  label = "Sleepy Doll",
  active = false,
  ...props
}: MoonPhaseButtonProps) {
  return (
    <ActionButton label={label} active={active} variant="moon" {...props}>
      <ArtSvg>
        <circle className="sd-paper" cx="12" cy="12" r="10" />
        <circle className="sd-color sd-moon-disc" cx="12" cy="12" r="8.35" />
        <path
          className="sd-outline"
          d="M12 3.2c5.25 0 8.8 3.55 8.8 8.8s-3.55 8.8-8.8 8.8S3.2 17.25 3.2 12 6.75 3.2 12 3.2Z"
        />
        <path
          className="sd-outline sd-moon-cut"
          d="M13.25 6.35a4.85 4.85 0 0 0 4.55 6.55 5.9 5.9 0 1 1-4.55-6.55Z"
        />
        <ellipse
          className="sd-detail sd-moon-orbit sd-moon-orbit-back"
          cx="12"
          cy="12"
          rx="10.1"
          ry="5.8"
          transform="rotate(-17 12 12)"
        />
      </ArtSvg>
    </ActionButton>
  );
}

export interface StateScanButtonProps extends NativeButtonProps {
  label?: string;
  status?: "idle" | "working" | "success";
}

export function StateScanButton({
  label = "读取当前状态",
  status = "idle",
  ...props
}: StateScanButtonProps) {
  return (
    <ActionButton label={label} status={status} variant="scan" {...props}>
      <ArtSvg>
        <circle className="sd-paper" cx="10" cy="10" r="7.3" />
        <circle className="sd-color sd-scan-field" cx="10" cy="10" r="5.6" />
        <circle className="sd-outline" cx="10" cy="10" r="6.4" />
        <path className="sd-outline" d="m14.6 14.6 5.8 5.8" />
        <path
          className="sd-detail sd-scan-line"
          d="m10 5.4 1.25 3.35L14.6 10l-3.35 1.25L10 14.6l-1.25-3.35L5.4 10l3.35-1.25Z"
        />
        <circle className="sd-detail sd-scan-lock" cx="10" cy="10" r="1.1" />
        <path className="sd-detail sd-success-check" d="m15.1 17.4 1.45 1.45 3.2-3.45" />
      </ArtSvg>
    </ActionButton>
  );
}

export interface RouteSearchButtonProps extends NativeButtonProps {
  label?: string;
  active?: boolean;
}

export function RouteSearchButton({
  label = "查找路线能力",
  active = false,
  ...props
}: RouteSearchButtonProps) {
  return (
    <ActionButton label={label} active={active} variant="route" {...props}>
      <ArtSvg>
        <circle className="sd-paper sd-route-paper" cx="12" cy="12" r="9.5" />
        <circle className="sd-color sd-route-panel panel-three" cx="12" cy="12" r="7.5" />
        <circle className="sd-outline" cx="12" cy="12" r="8.4" />
        <path
          className="sd-outline sd-route-path"
          d="m12 3.2 2.4 6.4 6.4 2.4-6.4 2.4-2.4 6.4-2.4-6.4L3.2 12l6.4-2.4Z"
        />
        <path
          className="sd-detail"
          d="m12 7.2 1.35 3.45L16.8 12l-3.45 1.35L12 16.8l-1.35-3.45L7.2 12l3.45-1.35Z"
        />
      </ArtSvg>
    </ActionButton>
  );
}

export interface SendActionButtonProps extends NativeButtonProps {
  label?: string;
  launching?: boolean;
}

export function SendActionButton({
  label = "发送",
  launching = false,
  ...props
}: SendActionButtonProps) {
  return (
    <ActionButton
      label={label}
      variant="send"
      data-launching={launching}
      {...props}
    >
      <ArtSvg>
        <circle className="sd-paper" cx="12" cy="12" r="10" />
        <circle className="sd-color sd-send-fill" cx="12" cy="12" r="8" />
        <circle className="sd-outline sd-send-ring" cx="12" cy="12" r="8.3" />
        <g className="sd-key-shape">
          <circle className="sd-outline" cx="8.2" cy="8.1" r="2.55" />
          <circle className="sd-outline" cx="13.3" cy="8.1" r="2.55" />
          <path className="sd-outline" d="M10.7 10.2v7.1l2 1.5-2 1.4-2-1.4 2-1.5" />
          <path className="sd-detail sd-send-inner" d="m10.75 5.7.95 1.55-.95 1.55-.95-1.55Z" />
        </g>
        <path className="sd-detail sd-send-trail trail-one" d="M3.2 12h-2.7" />
        <path className="sd-detail sd-send-trail trail-two" d="M4 15.5H1.8" />
      </ArtSvg>
    </ActionButton>
  );
}
