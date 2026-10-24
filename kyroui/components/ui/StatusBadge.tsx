import React from "react";
import { CheckCircle2, AlertTriangle, AlertCircle, Clock, Loader2, Minus } from "lucide-react";

export type StatusType =
  | "active"
  | "running"
  | "completed"
  | "verified"
  | "failed"
  | "warning"
  | "unverified"
  | "pending"
  | "cancelled"
  | "neutral";

export interface StatusBadgeProps {
  status: StatusType | string;
  label?: string;
  size?: "xs" | "sm" | "md";
  showIcon?: boolean;
  className?: string;
}

export const StatusBadge: React.FC<StatusBadgeProps> = ({
  status,
  label,
  size = "sm",
  showIcon = true,
  className = "",
}) => {
  const norm = status.toLowerCase();

  const getStyle = () => {
    switch (norm) {
      case "completed":
      case "verified":
      case "active":
      case "done":
        return {
          bg: "bg-green-950/40 text-green-300 border-green-800/50",
          icon: <CheckCircle2 className="w-3 h-3 text-green-400 shrink-0" />,
          defaultLabel: norm === "verified" ? "Verified" : "Completed",
        };
      case "running":
      case "executing":
        return {
          bg: "bg-neutral-800 text-neutral-200 border-neutral-700",
          icon: <Loader2 className="w-3 h-3 text-neutral-300 animate-spin shrink-0" />,
          defaultLabel: "Running",
        };
      case "failed":
      case "error":
        return {
          bg: "bg-red-950/40 text-red-300 border-red-800/50",
          icon: <AlertCircle className="w-3 h-3 text-red-400 shrink-0" />,
          defaultLabel: "Failed",
        };
      case "warning":
      case "unverified":
      case "needs_auth":
      case "needs_reauth":
        return {
          bg: "bg-amber-950/40 text-amber-300 border-amber-800/50",
          icon: <AlertTriangle className="w-3 h-3 text-amber-400 shrink-0" />,
          defaultLabel: norm === "unverified" ? "Unverified" : "Needs Review",
        };
      case "pending":
      case "waiting":
        return {
          bg: "bg-neutral-900 text-neutral-400 border-neutral-800",
          icon: <Clock className="w-3 h-3 text-neutral-400 shrink-0" />,
          defaultLabel: "Pending",
        };
      case "cancelled":
        return {
          bg: "bg-neutral-900 text-neutral-500 border-neutral-800",
          icon: <Minus className="w-3 h-3 text-neutral-500 shrink-0" />,
          defaultLabel: "Cancelled",
        };
      default:
        return {
          bg: "bg-neutral-900 text-neutral-400 border-neutral-800",
          icon: <span className="w-1.5 h-1.5 rounded-full bg-neutral-400 shrink-0" />,
          defaultLabel: status,
        };
    }
  };

  const { bg, icon, defaultLabel } = getStyle();

  const sizeStyles = {
    xs: "text-[10px] px-1.5 py-0.5 gap-1",
    sm: "text-[11px] px-2 py-0.5 gap-1.5",
    md: "text-xs px-2.5 py-1 gap-2",
  }[size];

  return (
    <span
      className={`inline-flex items-center rounded-md font-mono uppercase font-semibold border ${bg} ${sizeStyles} ${className}`}
    >
      {showIcon && icon}
      <span className="truncate">{label || defaultLabel}</span>
    </span>
  );
};
