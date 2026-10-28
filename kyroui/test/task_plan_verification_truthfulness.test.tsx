import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import {
  evaluateVerificationOutcome,
  TaskProgress,
  TaskPlan,
} from "../components/TaskProgress";
import { ToolResult } from "../types/agent";

describe("Phase 1.2 Regression Tests: Task Plans and Truthful Verification", () => {
  it("never marks verification complete solely because tool name is inspect_project", () => {
    const inspectionResult: ToolResult = {
      tool_call_id: "tc_inspect_1",
      name: "inspect_project",
      output: JSON.stringify({
        project_type: "nextjs",
        has_tests: true,
        package_manager: "npm",
      }),
      is_error: false,
    };

    const outcome = evaluateVerificationOutcome(inspectionResult);
    expect(outcome.verified).toBe(false);
    expect(outcome.attempted).toBe(false);
    expect(outcome.failed).toBe(false);
    expect(outcome.reason).toContain("does not verify task outcome");
  });

  it("never presents a failed test as successful verification", () => {
    // 1. Tool result with explicit is_error = true
    const errorResult: ToolResult = {
      tool_call_id: "tc_test_1",
      name: "run_tests",
      output: "FAIL: test_component_renders - expected true but received false",
      is_error: true,
    };

    const outcome1 = evaluateVerificationOutcome(errorResult);
    expect(outcome1.verified).toBe(false);
    expect(outcome1.attempted).toBe(true);
    expect(outcome1.failed).toBe(true);
    expect(outcome1.reason).toContain("Verification failed in run_tests");

    // 2. Tool result with is_error = false but test failure text in output
    const failedOutputResult: ToolResult = {
      tool_call_id: "tc_test_2",
      name: "execute_shell",
      output: "test result: FAILED. 2 passed; 1 failed; finished in 0.2s\nexit status: 1",
      is_error: false,
    };

    const outcome2 = evaluateVerificationOutcome(failedOutputResult, "cargo test");
    expect(outcome2.verified).toBe(false);
    expect(outcome2.attempted).toBe(true);
    expect(outcome2.failed).toBe(true);
    expect(outcome2.reason).toContain("Test execution reported failures");
  });

  it("marks verification as passed when real test tool succeeds without failures", () => {
    const successTestResult: ToolResult = {
      tool_call_id: "tc_test_3",
      name: "run_tests",
      output: "test result: ok. 45 passed; 0 failed; finished in 0.33s",
      is_error: false,
      verification: {
        status: "succeeded",
        condition_checked: "Test suite exit status and assertions",
        task_postcondition_verified: true,
        action_dispatched: true,
        action_effect_observed: true,
        evidence: "Verification check passed via run_tests",
        trusted_verifier: true,
      },
    };

    const outcome = evaluateVerificationOutcome(successTestResult);
    expect(outcome.verified).toBe(true);
    expect(outcome.attempted).toBe(true);
    expect(outcome.failed).toBe(false);
    expect(outcome.reason).toContain("Verification check passed");
  });

  it("isolates task plan to its own conversation and hides it on conversation switch", () => {
    const plan: TaskPlan = {
      id: "plan_1",
      runId: "run_alpha",
      conversationId: "conv_task_a",
      isBackendPlan: false,
      goal: "Refactor database migrations",
      understanding: "Migrating schema safely",
      resources: ["SQLite DB"],
      steps: [
        { id: "step_1", text: "Apply migration script", status: "completed", done: true },
        { id: "step_2", text: "Verify database integrity", status: "unverified", done: false },
      ],
      status: "unverified",
      accomplished: ["Executed migration"],
      remaining: ["Integrity checks not run"],
    };

    // 1. Rendered inside active conversation 'conv_task_a': plan is visible
    const { rerender } = render(
      <TaskProgress plan={plan} activeConversationId="conv_task_a" />
    );
    expect(screen.getByTestId("task-progress-card")).toBeInTheDocument();
    expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
    expect(screen.getByText(/unverified/i)).toBeInTheDocument();

    // 2. Switch conversation to 'conv_task_b': plan from 'conv_task_a' MUST be hidden
    rerender(<TaskProgress plan={plan} activeConversationId="conv_task_b" />);
    expect(screen.queryByTestId("task-progress-card")).not.toBeInTheDocument();

    // 3. Switch back to 'conv_task_a': plan is visible again
    rerender(<TaskProgress plan={plan} activeConversationId="conv_task_a" />);
    expect(screen.getByTestId("task-progress-card")).toBeInTheDocument();
  });

  it("displays cancelled state accurately when run is cancelled", () => {
    const cancelledPlan: TaskPlan = {
      id: "plan_cancel_1",
      runId: "run_cancel_1",
      conversationId: "conv_1",
      isBackendPlan: false,
      goal: "Long running automation",
      understanding: "Stopping upon user request",
      resources: [],
      steps: [
        { id: "step_1", text: "Start operation", status: "completed", done: true },
        { id: "step_2", text: "Execute automation", status: "failed", done: false, evidence: "Cancelled" },
      ],
      status: "cancelled",
      accomplished: ["Started initial step"],
      remaining: ["Execution cancelled by user request"],
    };

    render(<TaskProgress plan={cancelledPlan} activeConversationId="conv_1" />);
    expect(screen.getByText(/^cancelled$/i)).toBeInTheDocument();
    expect(screen.getByText(/Execution cancelled by user request/i)).toBeInTheDocument();
  });
});
