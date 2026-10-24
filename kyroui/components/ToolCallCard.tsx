import React, { useState } from "react";
import { ToolCall, ToolResult } from "../types/agent";
import { Terminal, Camera, FileText, Monitor, CheckCircle, AlertTriangle, ChevronDown, ChevronRight } from "lucide-react";

interface ToolCallCardProps {
  toolCall: ToolCall;
  result?: ToolResult;
}

export const ToolCallCard: React.FC<ToolCallCardProps> = ({ toolCall, result }) => {
  const [isExpanded, setIsExpanded] = useState(true);

  const getIcon = () => {
    switch (toolCall.name) {
      case "execute_shell":
        return <Terminal className="w-4 h-4 text-neutral-400" />;
      case "computer_action":
        return <Monitor className="w-4 h-4 text-neutral-400" />;
      case "capture_screen":
        return <Camera className="w-4 h-4 text-neutral-400" />;
      case "read_file":
      case "write_file":
        return <FileText className="w-4 h-4 text-neutral-400" />;
      default:
        return <Terminal className="w-4 h-4 text-neutral-400" />;
    }
  };

  const getPreviewText = () => {
    if (toolCall.name === "execute_shell") {
      return toolCall.arguments.command;
    }
    if (toolCall.name === "computer_action") {
      return `${toolCall.arguments.action} ${toolCall.arguments.x ? `(${toolCall.arguments.x}, ${toolCall.arguments.y})` : ""}`;
    }
    if (toolCall.name === "read_file" || toolCall.name === "write_file") {
      return toolCall.arguments.path;
    }
    return JSON.stringify(toolCall.arguments);
  };

  return (
    <div className="my-2 rounded-lg border border-neutral-800 bg-[#111111] overflow-hidden text-xs font-mono shadow-sm">
      {/* Header */}
      <div
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex items-center justify-between px-3.5 py-2.5 bg-[#141414] cursor-pointer hover:bg-[#181818] transition select-none"
      >
        <div className="flex items-center gap-2 truncate mr-2">
          {getIcon()}
          <span className="font-semibold text-neutral-200">{toolCall.name}</span>
          <span className="text-neutral-400 truncate max-w-md">{getPreviewText()}</span>
        </div>
        <div className="flex items-center gap-2">
          {result ? (
            result.is_error ? (
              <span className="flex items-center gap-1 text-red-400 text-[11px]">
                <AlertTriangle className="w-3.5 h-3.5" /> Failed
              </span>
            ) : (
              <span className="flex items-center gap-1 text-neutral-300 text-[11px]">
                <CheckCircle className="w-3.5 h-3.5" /> Done
              </span>
            )
          ) : (
            <span className="flex items-center gap-1.5 text-neutral-300 text-[11px]">
              <span className="w-1.5 h-1.5 rounded-full bg-neutral-400 animate-ping"></span>
              Running
            </span>
          )}
          {isExpanded ? <ChevronDown className="w-4 h-4 text-neutral-500" /> : <ChevronRight className="w-4 h-4 text-neutral-500" />}
        </div>
      </div>

      {/* Expanded details */}
      {isExpanded && (
        <div className="p-3 border-t border-neutral-800 space-y-2 bg-[#0a0a0a]">
          <div>
            <div className="text-[10px] uppercase tracking-wider text-neutral-500 mb-1">Arguments</div>
            <pre className="p-2 bg-[#000000] rounded border border-neutral-800 text-neutral-300 overflow-x-auto font-mono text-[12px] leading-relaxed">
              {JSON.stringify(toolCall.arguments, null, 2)}
            </pre>
          </div>

          {result && (
            <div>
              <div className="text-[10px] uppercase tracking-wider text-neutral-500 mb-1">Output</div>
              <pre
                className={`p-2 rounded border overflow-x-auto max-h-60 font-mono text-[12px] leading-relaxed ${
                  result.is_error
                    ? "bg-red-950/20 border-red-900/40 text-red-300"
                    : "bg-[#000000] border-neutral-800 text-neutral-300"
                }`}
              >
                {result.output}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
