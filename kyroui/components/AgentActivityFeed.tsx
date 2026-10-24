import React from "react";
import { Message, AgentStepEvent } from "../types/agent";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { ToolCallCard } from "./ToolCallCard";
import { ThinkingIndicator } from "./ThinkingIndicator";
import { AgentAvatar } from "./AgentAvatar";
import { User } from "lucide-react";

export interface AgentActivityFeedProps {
  messages: Message[];
  steps: AgentStepEvent[];
  isAgentRunning: boolean;
  agentName: string;
  activeReasoning?: {
    text: string;
    isStreaming: boolean;
    step?: number;
  } | null;
  pendingApprovalsCount?: number;
  activeStatusText?: string;
  messagesEndRef?: React.RefObject<HTMLDivElement | null>;
  className?: string;
}

export const AgentActivityFeed: React.FC<AgentActivityFeedProps> = ({
  messages,
  steps,
  isAgentRunning,
  agentName,
  activeReasoning,
  pendingApprovalsCount = 0,
  activeStatusText,
  messagesEndRef,
  className = "",
}) => {
  // Determine accurate status text based on backend/run state
  const computeStatusText = () => {
    if (activeStatusText) return activeStatusText;
    if (pendingApprovalsCount > 0) {
      return "Awaiting your approval to proceed…";
    }
    const lastStep = steps[steps.length - 1];
    if (lastStep && lastStep.tool_calls && lastStep.tool_calls.length > 0 && !lastStep.is_finished) {
      const runningTool = lastStep.tool_calls[lastStep.tool_calls.length - 1];
      if (/verify|test/i.test(runningTool.name)) {
        return `Verifying execution with ${runningTool.name}…`;
      }
      return `Executing tool: ${runningTool.name}…`;
    }
    if (activeReasoning?.text && activeReasoning.isStreaming) {
      return "Receiving model reasoning…";
    }
    return "Waiting for model response…";
  };

  return (
    <div className={`space-y-4 ${className}`}>
      {messages.map((msg) => {
        const isUser = msg.role === "user";
        return (
          <div
            key={msg.id}
            data-testid={`message-${msg.role}`}
            className={`flex items-start gap-3 text-sm ${
              isUser ? "flex-row-reverse" : "flex-row"
            }`}
          >
            <div
              className={`w-7 h-7 rounded-xl flex items-center justify-center shrink-0 border ${
                isUser
                  ? "bg-neutral-800 text-neutral-200 border-neutral-700"
                  : "bg-[#141414] text-neutral-300 border-neutral-800"
              }`}
            >
              {isUser ? (
                <User className="w-3.5 h-3.5" />
              ) : (
                <AgentAvatar name={agentName} className="w-5 h-5" />
              )}
            </div>

            <div
              className={`rounded-2xl p-4 max-w-[85%] sm:max-w-[75%] leading-relaxed ${
                isUser
                  ? "bg-[#1c1c1c] text-neutral-100 border border-neutral-800 select-text"
                  : "bg-[#121212] text-neutral-200 border border-neutral-800/80 select-text"
              }`}
            >
              <MarkdownRenderer content={msg.content} />
            </div>
          </div>
        );
      })}

      {/* Live Tool Calls and Steps */}
      {steps.map((st) => (
        <React.Fragment key={`step-${st.step}`}>
          {(st.tool_calls || []).map((tc, idx) => (
            <ToolCallCard
              key={`${st.step}-${tc.id || idx}`}
              toolCall={tc}
              result={(st.tool_results || []).find((r) => r.tool_call_id === tc.id)}
            />
          ))}
        </React.Fragment>
      ))}

      {/* Live Thinking / Reasoning / Tool status */}
      {isAgentRunning && (
        <div className="py-1">
          <ThinkingIndicator
            agentName={agentName}
            reasoningText={activeReasoning?.text}
            isStreaming={activeReasoning?.isStreaming ?? true}
            statusText={computeStatusText()}
          />
        </div>
      )}

      <div ref={messagesEndRef} />
    </div>
  );
};
