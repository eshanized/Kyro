import React, { useEffect, useRef } from "react";
import { AlertTriangle } from "lucide-react";

export interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Renders the confirm action in the destructive style. */
  destructive?: boolean;
  isBusy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * An accessible confirmation dialog.
 *
 * It replaces the browser `alert()`/`confirm()` dialogs that were used for
 * destructive actions.  The dialog is modal, labelled, keyboard operable
 * (Escape cancels, Enter confirms) and traps the initial focus, so it cannot
 * be dismissed by accident and it is usable without a mouse.
 */
export const ConfirmDialog: React.FC<ConfirmDialogProps> = ({
  isOpen,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  isBusy = false,
  onConfirm,
  onCancel,
}) => {
  const confirmRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    confirmRef.current?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        if (!isBusy) onCancel();
        return;
      }
      if (e.key === "Tab" && dialogRef.current) {
        // Keep the focus inside the dialog.
        const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [isOpen, isBusy, onCancel]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center bg-black/80 backdrop-blur-md p-4"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !isBusy) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="kyro-confirm-title"
        aria-describedby="kyro-confirm-message"
        className="w-full max-w-md bg-[#141414] border border-[#2a2a2a] rounded-2xl shadow-2xl overflow-hidden"
      >
        <div className="flex items-start gap-3 px-6 pt-6 pb-4">
          <div
            className={
              destructive
                ? "mt-0.5 p-2 rounded-full bg-red-500/15 text-red-300 shrink-0"
                : "mt-0.5 p-2 rounded-full bg-neutral-800 text-neutral-300 shrink-0"
            }
          >
            <AlertTriangle className="w-5 h-5" />
          </div>
          <div className="min-w-0">
            <h2 id="kyro-confirm-title" className="text-sm font-semibold text-neutral-100">
              {title}
            </h2>
            <p id="kyro-confirm-message" className="mt-1.5 text-xs leading-relaxed text-neutral-400">
              {message}
            </p>
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 px-6 pb-6">
          <button
            type="button"
            onClick={onCancel}
            disabled={isBusy}
            className="px-3.5 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:text-neutral-100 hover:bg-surface-hover border border-transparent transition disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            type="button"
            onClick={onConfirm}
            disabled={isBusy}
            className={
              destructive
                ? "px-3.5 py-2 rounded-lg text-xs font-semibold bg-red-600 hover:bg-red-500 text-white transition disabled:opacity-50 disabled:cursor-not-allowed"
                : "px-3.5 py-2 rounded-lg text-xs font-semibold bg-[#ededed] hover:bg-white text-[#0a0a0a] transition disabled:opacity-50 disabled:cursor-not-allowed"
            }
          >
            {isBusy ? "Working…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
};
