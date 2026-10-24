import React, { useState, useEffect, useRef } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

export interface ThinkingIndicatorProps {
  /** The display name or handle of the active agent, e.g. "Kyro" or "@Scout". */
  agentName?: string;
  /** Live reasoning stream accumulated from the provider. */
  reasoningText?: string;
  /** Whether reasoning tokens are currently arriving. */
  isStreaming?: boolean;
  /** Fallback status message when reasoning is unsupported or hasn't started yet. */
  statusText?: string;
  /** Optional container class name. */
  className?: string;
}

export const ThinkingIndicator: React.FC<ThinkingIndicatorProps> = ({
  agentName = "Kyro",
  reasoningText,
  isStreaming = true,
  statusText = "Waiting for model response…",
  className = "",
}) => {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const userScrolledUpRef = useRef<boolean>(false);

  // Auto-scroll logic: only auto-scroll if the user is already near the bottom
  const handleScroll = () => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight <= 35;
    userScrolledUpRef.current = !isAtBottom;
  };

  useEffect(() => {
    if (scrollRef.current && !userScrolledUpRef.current && !isCollapsed) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [reasoningText, isCollapsed]);

  const hasReasoning = Boolean(reasoningText && reasoningText.trim().length > 0);

  // If reasoning is absent or not yet received: display the subtle animated waiting indicator
  if (!hasReasoning) {
    return (
      <div
        role="status"
        aria-label={`${agentName} is thinking`}
        aria-live="polite"
        className={`flex items-center gap-2.5 px-3 py-2 rounded-lg bg-[#0e0e0e]/80 border border-neutral-800/80 text-xs font-mono select-none thinking-shimmer ${className}`}
      >
        <div className="flex items-center gap-1.5" aria-hidden="true">
          <span className="w-1.5 h-1.5 rounded-full bg-neutral-300 animate-thinking-dot-1" />
          <span className="w-1.5 h-1.5 rounded-full bg-neutral-400 animate-thinking-dot-2" />
          <span className="w-1.5 h-1.5 rounded-full bg-neutral-500 animate-thinking-dot-3" />
        </div>
        <span className="text-[12px] text-neutral-300 font-mono tracking-tight">{statusText}</span>
      </div>
    );
  }

  // When reasoning text is present: display the header and plain-text stream
  return (
    <div
      role="status"
      aria-label={`${agentName} model reasoning`}
      className={`rounded-lg border border-neutral-800 bg-[#0e0e0e] shadow-sm overflow-hidden transition-all duration-200 ${
        isStreaming ? "thinking-panel-glow" : ""
      } ${className}`}
    >
      {/* Header bar */}
      <div
        className="flex items-center justify-between px-3 py-2 bg-neutral-900/60 border-b border-neutral-800/60 cursor-pointer select-none hover:bg-neutral-900/90 transition-colors"
        onClick={() => setIsCollapsed((prev) => !prev)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            setIsCollapsed((prev) => !prev);
          }
        }}
        aria-expanded={!isCollapsed}
      >
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1" aria-hidden="true">
            {isStreaming ? (
              <>
                <span className="w-1.5 h-1.5 rounded-full bg-neutral-200 animate-thinking-dot-1" />
                <span className="w-1.5 h-1.5 rounded-full bg-neutral-400 animate-thinking-dot-2" />
                <span className="w-1.5 h-1.5 rounded-full bg-neutral-500 animate-thinking-dot-3" />
              </>
            ) : (
              <span className="w-1.5 h-1.5 rounded-full bg-neutral-500" />
            )}
          </div>
          <span className="font-mono text-[12px] text-neutral-200 font-medium tracking-tight">
            {agentName} · Model reasoning
          </span>
        </div>

        <div className="flex items-center gap-1.5 text-neutral-400 hover:text-neutral-200 transition-colors">
          <span className="text-[11px] text-neutral-500 font-mono">
            {isCollapsed ? "Show" : "Hide"}
          </span>
          {isCollapsed ? (
            <ChevronRight className="w-3.5 h-3.5" />
          ) : (
            <ChevronDown className="w-3.5 h-3.5" />
          )}
        </div>
      </div>

      {/* Stream body: strictly plain-text, preserving all newlines and whitespace */}
      {!isCollapsed && (
        <div
          ref={scrollRef}
          onScroll={handleScroll}
          className="max-h-60 overflow-y-auto p-3 font-mono text-[12px] leading-relaxed text-neutral-300 bg-[#0a0a0a] select-text"
          tabIndex={0}
          aria-live="off"
        >
          <pre className="whitespace-pre-wrap break-words font-mono text-[12px] leading-relaxed text-neutral-300 m-0 p-0 font-normal">
            {reasoningText}
          </pre>
        </div>
      )}
    </div>
  );
};
