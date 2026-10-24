import React, { useState } from "react";
import { Check, Copy } from "lucide-react";

export interface CodePanelProps {
  code: string;
  title?: string;
  language?: string;
  maxHeight?: string;
  className?: string;
  isError?: boolean;
}

export const CodePanel: React.FC<CodePanelProps> = ({
  code,
  title,
  language,
  maxHeight = "max-h-72",
  className = "",
  isError = false,
}) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(code);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }
    } catch {
      // ignore
    }
  };

  return (
    <div
      className={`rounded-xl border overflow-hidden text-[12px] font-mono shadow-inner ${
        isError
          ? "bg-red-950/20 border-red-900/40 text-red-300"
          : "bg-[#000000] border-neutral-800 text-neutral-300"
      } ${className}`}
    >
      {(title || language) && (
        <div className="flex items-center justify-between px-3.5 py-1.5 bg-[#111111] border-b border-neutral-800/80 text-[11px] text-neutral-400 select-none">
          <span className="font-semibold text-neutral-300 truncate">
            {title || language?.toUpperCase()}
          </span>
          <button
            type="button"
            onClick={handleCopy}
            aria-label="Copy code to clipboard"
            className="flex items-center gap-1 text-[11px] text-neutral-400 hover:text-neutral-100 transition-colors focus:outline-none focus:ring-1 focus:ring-neutral-500 rounded px-1.5 py-0.5"
            title="Copy code"
          >
            {copied ? (
              <>
                <Check className="w-3.5 h-3.5 text-neutral-200" />
                <span>Copied</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Copy</span>
              </>
            )}
          </button>
        </div>
      )}
      <div className={`overflow-x-auto ${maxHeight} p-3 select-text`}>
        <pre className="font-mono text-[12px] leading-relaxed m-0 p-0 whitespace-pre-wrap break-all select-text font-normal">
          {code}
        </pre>
      </div>
    </div>
  );
};
