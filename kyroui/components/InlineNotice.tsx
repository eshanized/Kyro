import React from "react";
import { AlertCircle, CheckCircle2, X } from "lucide-react";

export type NoticeTone = "error" | "success" | "info";

export interface InlineNoticeProps {
  tone: NoticeTone;
  message: string;
  onDismiss?: () => void;
}

/**
 * An inline notice.
 *
 * It replaces the browser `alert()` calls that were used for error
 * reporting, so that failures are visible inside the application instead of
 * in a modal browser dialog that blocks the whole UI.
 */
export const InlineNotice: React.FC<InlineNoticeProps> = ({ tone, message, onDismiss }) => {
  // The application theme is a neutral Next.js-inspired palette; only the
  // error tone uses a colour, matching the destructive actions elsewhere.
  const palette =
    tone === "error"
      ? "border-red-500/30 bg-red-500/10 text-red-200"
      : tone === "success"
        ? "border-neutral-700 bg-[#161616] text-neutral-200"
        : "border-neutral-700 bg-[#161616] text-neutral-300";

  const Icon = tone === "success" ? CheckCircle2 : AlertCircle;

  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={`flex items-start gap-2 px-3 py-2 rounded-lg border text-xs ${palette}`}
    >
      <Icon className="w-4 h-4 mt-px shrink-0" />
      <span className="flex-1 break-words">{message}</span>
      {onDismiss && (
        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss notification"
          className="opacity-70 hover:opacity-100 transition"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      )}
    </div>
  );
};
