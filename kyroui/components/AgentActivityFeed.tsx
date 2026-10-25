import React from "react";
import { Message, AgentStepEvent } from "../types/agent";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { ToolCallCard } from "./ToolCallCard";
import { ThinkingIndicator } from "./ThinkingIndicator";
import { Copy, Check } from "lucide-react";

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
  /** Production adapters: the App owns clipboard/TTS state (Bug 18). */
  onCopyMessage?: (msgId: string, text: string) => void;
  onSpeakMessage?: (msgId: string, text: string) => void;
  copiedMessageId?: string | null;
  speakingMessageId?: string | null;
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
  onCopyMessage,
  onSpeakMessage,
  copiedMessageId = null,
  speakingMessageId = null,
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
        const isCopyable = !isUser && typeof onCopyMessage === "function";
        const isSpeakable = !isUser && typeof onSpeakMessage === "function";
        // Timeline tool calls persisted with an assistant message (the same
        // rendering the production App performed inline; moved here so this
        // component is the single message renderer for the app).
        let messageToolCalls: Array<any> = [];
        if (!isUser && msg.tool_calls_json) {
          try {
            const parsed = JSON.parse(msg.tool_calls_json);
            if (Array.isArray(parsed) && parsed.length > 0) messageToolCalls = parsed;
          } catch {
            messageToolCalls = [];
          }
        }
        let messageObservations: Array<any> = [];
        if (!isUser && (msg as any).observations_json) {
          try {
            const parsedObs = JSON.parse((msg as any).observations_json);
            if (Array.isArray(parsedObs)) messageObservations = parsedObs;
          } catch {
            messageObservations = [];
          }
        }
        return (
          <div
            key={msg.id}
            data-testid={`message-${msg.role}`}
            className={`flex flex-col ${
              isUser ? "items-end" : "items-start"
            }`}
          >
            <div
              className={`max-w-2xl px-4 py-3 rounded-2xl text-sm leading-relaxed ${
                isUser
                  ? "bg-[#1f1f1f] text-[#ededed] border border-[#2e2e2e] rounded-br-none shadow-sm"
                  : "bg-[#121212] text-[#ededed] border border-[#262626] rounded-bl-none shadow-sm"
              }`}
            >
              {isUser ? (
                <div className="whitespace-pre-wrap">{msg.content}</div>
              ) : (
                <MarkdownRenderer content={msg.content} />
              )}

              {messageToolCalls.length > 0 && (
                <div className="space-y-2 mt-3 pt-2 border-t border-[#1f1f1f] w-full">
                  {messageToolCalls.map((tc: any, idx: number) => {
                    const res: any = messageObservations.find(
                      (r: any) => r.tool_call_id === tc.id
                    );
                    return (
                      <ToolCallCard
                        key={`msg-tc-${tc.id || idx}`}
                        toolCall={tc}
                        result={res}
                        defaultExpanded={false}
                      />
                    );
                  })}
                </div>
              )}

              {!isUser && (isCopyable || isSpeakable) && (
                <div className="flex items-center justify-between pt-2 mt-2 border-t border-neutral-800 text-xs">
                  <span className="text-[10px] text-neutral-400 font-mono">
                    {new Date(msg.created_at).toLocaleTimeString()}
                  </span>
                  <div className="flex items-center gap-3">
                    {isCopyable && (
                      <button
                        onClick={() => onCopyMessage?.(msg.id, msg.content)}
                        className="text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
                        title="Copy response"
                      >
                        {copiedMessageId === msg.id ? (
                          <>
                            <Check className="w-3.5 h-3.5 text-neutral-200" />
                            <span className="text-[10px]">Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-3.5 h-3.5" />
                            <span className="text-[10px]">Copy</span>
                          </>
                        )}
                      </button>
                    )}
                    {isSpeakable && (
                      <button
                        onClick={() => onSpeakMessage?.(msg.id, msg.content)}
                        className="text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
                        title="Read message aloud (Local TTS)"
                      >
                        {speakingMessageId === msg.id ? (
                          <>
                            <span className="text-[10px]">Stop</span>
                          </>
                        ) : (
                          <>
                            <span className="text-[10px]">Speak</span>
                          </>
                        )}
                      </button>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>
        );
      })}

      {/* Live Tool Calls, Steps and Thoughts */}
      {steps.map((st) => (
        <React.Fragment key={`step-${st.step}-${st.run_id || ""}`}>
          {st.thought && (
            <div className="flex flex-col items-start">
              {st.is_child_run && (
                <div className="mb-1.5 text-[10px] uppercase tracking-wide text-neutral-500 font-mono">
                  Delegated agent
                  {st.agent_id ? ` · ${st.agent_id}` : ""}
                </div>
              )}
              <div className="rounded-2xl p-4 max-w-[85%] sm:max-w-[75%] leading-relaxed bg-[#121212] text-neutral-200 border border-neutral-800/80 select-text">
                <MarkdownRenderer content={st.thought} />
              </div>
            </div>
          )}
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
