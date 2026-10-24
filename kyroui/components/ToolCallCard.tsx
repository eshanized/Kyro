import React, { useState, useEffect, useRef } from "react";
import { ToolCall, ToolResult } from "../types/agent";
import {
  Terminal,
  Camera,
  FileText,
  Monitor,
  CheckCircle2,
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Copy,
  Check,
  ShieldCheck,
} from "lucide-react";
import { evaluateVerificationOutcome } from "./TaskProgress";

interface ToolCallCardProps {
  toolCall: ToolCall;
  result?: ToolResult;
  defaultExpanded?: boolean;
}

export const ToolCallCard: React.FC<ToolCallCardProps> = ({
  toolCall,
  result,
  defaultExpanded = true,
}) => {
  const userToggledRef = useRef(false);

  const [isExpanded, setIsExpanded] = useState<boolean>(defaultExpanded);
  const [copiedSection, setCopiedSection] = useState<string | null>(null);

  useEffect(() => {
    if (!userToggledRef.current && defaultExpanded !== undefined) {
      setIsExpanded(defaultExpanded);
    }
  }, [defaultExpanded]);

  const handleToggle = () => {
    userToggledRef.current = true;
    setIsExpanded((prev) => !prev);
  };

  const handleCopy = async (e: React.MouseEvent, text: string, section: string) => {
    e.stopPropagation();
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        setCopiedSection(section);
        setTimeout(() => setCopiedSection(null), 2000);
      }
    } catch {
      // ignore
    }
  };

  const getIcon = () => {
    switch (toolCall.name) {
      case "execute_shell":
        return <Terminal className="w-3.5 h-3.5 text-neutral-400 shrink-0" />;
      case "computer_action":
        return <Monitor className="w-3.5 h-3.5 text-neutral-400 shrink-0" />;
      case "capture_screen":
        return <Camera className="w-3.5 h-3.5 text-neutral-400 shrink-0" />;
      case "read_file":
      case "write_file":
        return <FileText className="w-3.5 h-3.5 text-neutral-400 shrink-0" />;
      default:
        return <Terminal className="w-3.5 h-3.5 text-neutral-400 shrink-0" />;
    }
  };

  const getPreviewText = (): string => {
    if (toolCall.name === "execute_shell" && toolCall.arguments?.command) {
      return String(toolCall.arguments.command);
    }
    if (toolCall.name === "computer_action") {
      const act = toolCall.arguments?.action || "";
      const coords = toolCall.arguments?.x ? `(${toolCall.arguments.x}, ${toolCall.arguments.y})` : "";
      return `${act} ${coords}`.trim();
    }
    if (toolCall.name === "read_file" || toolCall.name === "write_file") {
      return toolCall.arguments?.path || "";
    }
    return JSON.stringify(toolCall.arguments);
  };

  const verification = result
    ? evaluateVerificationOutcome(result, toolCall.arguments?.command)
    : null;

  const resultPreview = result && typeof result.output === "string"
    ? result.output.replace(/[\r\n]+/g, " ").slice(0, 90)
    : "";

  return (
    <div
      data-testid="tool-call-card"
      className="my-2 rounded-xl border border-neutral-800 bg-[#111111] overflow-hidden text-xs font-mono shadow-sm transition"
    >
      {/* Header Button with keyboard accessibility */}
      <button
        type="button"
        onClick={handleToggle}
        aria-expanded={isExpanded}
        className="w-full flex items-center justify-between px-3.5 py-2.5 bg-[#141414] hover:bg-[#181818] transition text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-400"
      >
        <div className="flex items-center gap-2 truncate mr-3 min-w-0">
          {getIcon()}
          <span className="font-semibold text-neutral-200 shrink-0">{toolCall.name}</span>
          <span className="text-neutral-400 truncate text-[11px] select-text">
            {getPreviewText()}
          </span>
          {!isExpanded && result && resultPreview && (
            <span className="hidden md:inline text-neutral-500 text-[10px] truncate max-w-xs">
              → {resultPreview}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {result ? (
            result.is_error ? (
              <span className="flex items-center gap-1 text-red-400 text-[11px] font-semibold">
                <AlertTriangle className="w-3.5 h-3.5" /> Failed
              </span>
            ) : verification?.verified ? (
              <span className="flex items-center gap-1 text-green-400 text-[11px] font-semibold">
                <ShieldCheck className="w-3.5 h-3.5" /> Verified
              </span>
            ) : (
              <span className="flex items-center gap-1 text-neutral-300 text-[11px]">
                <CheckCircle2 className="w-3.5 h-3.5 text-neutral-400" /> Done
              </span>
            )
          ) : (
            <span className="flex items-center gap-1.5 text-neutral-300 text-[11px]">
              <span className="w-1.5 h-1.5 rounded-full bg-neutral-300 animate-pulse" />
              Running
            </span>
          )}

          <span className="text-neutral-500">
            {isExpanded ? (
              <ChevronDown className="w-3.5 h-3.5" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5" />
            )}
          </span>
        </div>
      </button>

      {/* Expanded details */}
      {isExpanded && (
        <div className="p-3 border-t border-neutral-800 space-y-3 bg-[#0a0a0a]">
          {/* Arguments */}
          <div>
            <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-neutral-500 mb-1">
              <span>Arguments</span>
              <button
                type="button"
                onClick={(e) => handleCopy(e, JSON.stringify(toolCall.arguments, null, 2), "args")}
                className="flex items-center gap-1 text-[10px] text-neutral-400 hover:text-neutral-200 transition"
                aria-label="Copy tool arguments"
              >
                {copiedSection === "args" ? (
                  <>
                    <Check className="w-3 h-3 text-neutral-300" />
                    <span>Copied</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3 h-3" />
                    <span>Copy</span>
                  </>
                )}
              </button>
            </div>
            <pre className="p-2.5 bg-[#000000] rounded-lg border border-neutral-800/80 text-neutral-300 overflow-x-auto max-h-48 font-mono text-[12px] leading-relaxed select-text m-0 whitespace-pre-wrap break-all">
              {JSON.stringify(toolCall.arguments, null, 2)}
            </pre>
          </div>

          {/* Output */}
          {result && (
            <div>
              <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-neutral-500 mb-1">
                <span>Output</span>
                <button
                  type="button"
                  onClick={(e) => handleCopy(e, result.output || "", "output")}
                  className="flex items-center gap-1 text-[10px] text-neutral-400 hover:text-neutral-200 transition"
                  aria-label="Copy tool output"
                >
                  {copiedSection === "output" ? (
                    <>
                      <Check className="w-3 h-3 text-neutral-300" />
                      <span>Copied</span>
                    </>
                  ) : (
                    <>
                      <Copy className="w-3 h-3" />
                      <span>Copy</span>
                    </>
                  )}
                </button>
              </div>
              <pre
                className={`p-2.5 rounded-lg border overflow-x-auto max-h-72 font-mono text-[12px] leading-relaxed select-text m-0 whitespace-pre-wrap break-all ${
                  result.is_error
                    ? "bg-red-950/20 border-red-900/40 text-red-300"
                    : "bg-[#000000] border-neutral-800 text-neutral-300"
                }`}
              >
                {result.output || ""}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
