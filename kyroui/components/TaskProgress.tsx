import React from "react";
import { ListOrdered, CheckCircle2, AlertCircle, Clock, X, AlertTriangle } from "lucide-react";
import { ToolResult } from "../types/agent";

export interface TaskStep {
  id: string;
  text: string;
  status: "pending" | "running" | "completed" | "failed" | "unverified";
  done?: boolean;
  evidence?: string;
  toolName?: string;
}

export interface TaskPlan {
  id: string;
  runId: string;
  conversationId: string;
  isBackendPlan?: boolean;
  goal: string;
  understanding: string;
  resources: string[];
  steps: TaskStep[];
  status: "planning" | "approved" | "executing" | "completed" | "failed" | "cancelled" | "unverified";
  accomplished: string[];
  remaining: string[];
}

export interface VerificationEvidence {
  verified: boolean;
  attempted: boolean;
  failed: boolean;
  reason: string;
  toolName?: string;
}

/**
 * Truthfully evaluates verification evidence from a tool result.
 * Inspection tools (e.g. `inspect_project`) do NOT constitute verification.
 * A failed test MUST NEVER be counted as successful verification.
 */
export function evaluateVerificationOutcome(toolResult: ToolResult, commandArg?: string): VerificationEvidence {
  const toolName = (toolResult.name || "").toLowerCase();

  // Inspection/diagnostic tools are never task verification
  const isInspectionOnly =
    toolName === "inspect_project" ||
    toolName === "read_file" ||
    toolName === "capture_screen" ||
    toolName === "browser_open" ||
    toolName === "list_directory" ||
    toolName.startsWith("inspect_") ||
    toolName.startsWith("read_");

  if (isInspectionOnly) {
    return {
      verified: false,
      attempted: false,
      failed: false,
      reason: `Inspection performed by ${toolResult.name}; does not verify task outcome.`,
      toolName: toolResult.name,
    };
  }

  // Check if tool is an explicit test or verification tool
  const isTestOrVerificationTool =
    toolName === "run_tests" ||
    toolName === "run_test" ||
    toolName === "test" ||
    toolName === "execute_tests" ||
    toolName === "verify_checkpoint" ||
    toolName === "check_verification" ||
    (toolName === "execute_shell" &&
      Boolean(
        commandArg &&
          /\b(test|pytest|npm\s+test|cargo\s+test|vitest|jest|make\s+test|make\s+verify)\b/i.test(commandArg)
      ));

  if (!isTestOrVerificationTool) {
    return {
      verified: false,
      attempted: false,
      failed: false,
      reason: `Action executed (${toolResult.name}).`,
      toolName: toolResult.name,
    };
  }

  // The tool was a test / verification tool
  if (toolResult.is_error) {
    return {
      verified: false,
      attempted: true,
      failed: true,
      reason: `Verification failed in ${toolResult.name}: ${toolResult.output.slice(0, 100)}`,
      toolName: toolResult.name,
    };
  }

  // Check for test failures inside the output text
  const lowerOutput = (toolResult.output || "").toLowerCase();
  const testFailedPattern =
    lowerOutput.includes("test failed") ||
    lowerOutput.includes("failures:") ||
    lowerOutput.includes("failed:") ||
    lowerOutput.includes("assertion error") ||
    lowerOutput.includes("tests failed") ||
    lowerOutput.includes("exit code: 1") ||
    lowerOutput.includes("exit status: 1");

  if (testFailedPattern) {
    return {
      verified: false,
      attempted: true,
      failed: true,
      reason: `Test execution reported failures: ${toolResult.output.slice(0, 100)}`,
      toolName: toolResult.name,
    };
  }

  return {
    verified: true,
    attempted: true,
    failed: false,
    reason: `Verification check passed via ${toolResult.name}`,
    toolName: toolResult.name,
  };
}

export interface TaskProgressProps {
  plan: TaskPlan;
  activeConversationId?: string | null;
  onDismiss?: () => void;
  className?: string;
}

export const TaskProgress: React.FC<TaskProgressProps> = ({
  plan,
  activeConversationId,
  onDismiss,
  className = "",
}) => {
  // Never show a plan from another conversation
  if (activeConversationId && plan.conversationId && plan.conversationId !== activeConversationId) {
    return null;
  }

  const getStatusBadge = () => {
    switch (plan.status) {
      case "completed":
        return "bg-green-950/40 text-green-300 border border-green-800/60";
      case "failed":
        return "bg-red-950/40 text-red-300 border border-red-800/60";
      case "unverified":
        return "bg-amber-950/40 text-amber-300 border border-amber-800/60";
      case "executing":
        return "bg-neutral-800 text-neutral-200 border border-neutral-700 animate-pulse";
      case "cancelled":
        return "bg-neutral-900 text-neutral-400 border border-neutral-800";
      default:
        return "bg-neutral-900 text-neutral-400 border border-neutral-800";
    }
  };

  return (
    <div
      data-testid="task-progress-card"
      className={`max-w-2xl mx-auto my-3 p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3 shadow-lg select-text ${className}`}
    >
      <div className="flex items-center justify-between pb-2 border-b border-neutral-800">
        <div className="flex items-center gap-2">
          <ListOrdered className="w-4 h-4 text-neutral-300 shrink-0" />
          <span className="text-xs font-semibold text-neutral-200">
            {plan.isBackendPlan ? "Authoritative Execution Plan" : "Suggested Workflow"}
          </span>
          <span className="text-[10px] font-mono text-neutral-500">
            ({plan.isBackendPlan ? "backend" : "suggested"})
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span
            className={`text-[10px] px-2 py-0.5 rounded-full font-mono uppercase font-bold ${getStatusBadge()}`}
          >
            {plan.status}
          </span>
          {onDismiss && (
            <button
              type="button"
              onClick={onDismiss}
              className="text-neutral-500 hover:text-neutral-300 p-1 rounded transition"
              title="Dismiss plan"
              aria-label="Dismiss plan"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      <div className="text-xs space-y-2">
        <div>
          <span className="text-neutral-400">Context: </span>
          <span className="text-neutral-200 font-medium">{plan.understanding}</span>
        </div>

        {plan.resources && plan.resources.length > 0 && (
          <div className="flex flex-wrap items-center gap-1.5 pt-1">
            <span className="text-neutral-400 text-[11px]">Resources: </span>
            {plan.resources.map((res, i) => (
              <span
                key={i}
                className="px-2 py-0.5 bg-[#0a0a0a] border border-neutral-800 text-[10px] text-neutral-300 rounded font-mono"
              >
                {res}
              </span>
            ))}
          </div>
        )}

        <div className="space-y-1.5 pt-1">
          <span className="text-neutral-400 block font-medium text-[11px]">
            {plan.isBackendPlan ? "Planned Steps:" : "Workflow Steps:"}
          </span>
          {plan.steps.map((st) => {
            const isComplete = st.status === "completed" || (!st.status && st.done);
            const isFailed = st.status === "failed";
            const isRunning = st.status === "running";
            const isUnverified = st.status === "unverified";

            return (
              <div
                key={st.id}
                className={`p-2 rounded-lg border flex items-start gap-2.5 transition text-[11px] ${
                  isComplete
                    ? "bg-[#161616] border-green-900/40 text-neutral-200"
                    : isFailed
                    ? "bg-red-950/20 border-red-900/40 text-red-200"
                    : isUnverified
                    ? "bg-amber-950/20 border-amber-900/40 text-amber-200"
                    : isRunning
                    ? "bg-[#181818] border-neutral-700 text-neutral-100"
                    : "bg-[#111111] border-neutral-800 text-neutral-400"
                }`}
              >
                <div className="mt-0.5 shrink-0">
                  {isComplete ? (
                    <CheckCircle2 className="w-3.5 h-3.5 text-green-400" />
                  ) : isFailed ? (
                    <AlertCircle className="w-3.5 h-3.5 text-red-400" />
                  ) : isUnverified ? (
                    <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                  ) : isRunning ? (
                    <Clock className="w-3.5 h-3.5 text-neutral-300 animate-spin" />
                  ) : (
                    <div className="w-3.5 h-3.5 rounded-full border border-neutral-600" />
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="font-medium">{st.text}</div>
                  {st.evidence && (
                    <div className="text-[10px] text-neutral-400 font-mono mt-0.5 break-words">
                      Evidence: {st.evidence}
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {plan.accomplished && plan.accomplished.length > 0 && (
          <div className="pt-2 border-t border-neutral-800/80">
            <span className="text-[11px] font-semibold text-neutral-300 block mb-1">
              Accomplished Actions:
            </span>
            <ul className="list-disc list-inside text-[11px] text-neutral-400 space-y-0.5">
              {plan.accomplished.map((item, i) => (
                <li key={i}>{item}</li>
              ))}
            </ul>
          </div>
        )}

        {plan.remaining && plan.remaining.length > 0 && (
          <div className="pt-1">
            <span className="text-[11px] font-semibold text-neutral-400 block mb-1">
              Remaining / Attention Required:
            </span>
            <ul className="list-disc list-inside text-[11px] text-neutral-400 space-y-0.5">
              {plan.remaining.map((item, i) => (
                <li key={i}>{item}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
};
