import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import React from "react";
import { ToolCallCard } from "../components/ToolCallCard";
import { TaskProgress, TaskPlan } from "../components/TaskProgress";
import { redactSensitiveFields, parseApprovalArgs } from "../App";
import { ToolCall, ToolResult } from "../types/agent";

describe("Trust, Authorization, and Execution Integrity Frontend Tests", () => {
  describe("Credential Redaction and Argument Parsing (Scenario F)", () => {
    it("redacts sensitive fields like passwords, tokens, API keys in nested arguments", () => {
      const sensitiveObj = {
        command: "curl -H 'Authorization: Bearer my-secret' https://api.example.com",
        credentials: {
          password: "super_secret_password_123",
          api_key: "ak_live_xyz987654",
          authToken: "jwt_token_payload",
        },
        public_param: "safe_value",
      };

      const redacted = redactSensitiveFields(sensitiveObj);

      expect(redacted.credentials.password).toBe("[REDACTED]");
      expect(redacted.credentials.api_key).toBe("[REDACTED]");
      expect(redacted.credentials.authToken).toBe("[REDACTED]");
      expect(redacted.public_param).toBe("safe_value");
    });

    it("parses and separates target details from metadata in approval arguments", () => {
      const shellArgs = JSON.stringify({ command: "rm -rf /var/cache/app" });
      const parsedShell = parseApprovalArgs(shellArgs);
      expect(parsedShell.targetDetail).toBe("Command: rm -rf /var/cache/app");

      const fileArgs = JSON.stringify({ path: "/etc/hosts", content: "127.0.0.1 custom.domain" });
      const parsedFile = parseApprovalArgs(fileArgs);
      expect(parsedFile.targetDetail).toBe("File: /etc/hosts");

      const hotkeyArgs = JSON.stringify({ action: "hotkey", keys: ["Alt", "F4"] });
      const parsedHotkey = parseApprovalArgs(hotkeyArgs);
      expect(parsedHotkey.targetDetail).toBe("Action: hotkey");
    });
  });

  describe("ToolCallCard Truthful Execution Status (Scenario C)", () => {
    it("renders Uncertain badge when tool reports success but visual change is inconclusive", () => {
      const toolCall: ToolCall = {
        id: "call_computer_1",
        name: "computer_action",
        arguments: { action: "left_click", x: 450, y: 320 },
      };

      const uncertainResult: ToolResult = {
        tool_call_id: "call_computer_1",
        name: "computer_action",
        output: JSON.stringify({
          status: "uncertain",
          note: "Click delivered but visual difference was 1.2% (below 5% threshold)",
          verification: {
            status: "uncertain",
            condition_checked: "UI visual state transition",
            resource: "action 'left_click' at (450, 320)",
            evidence: "Visual change (1.20%) was inconclusive",
            human_intervention_required: true,
          },
        }),
        is_error: false,
      };

      render(<ToolCallCard toolCall={toolCall} result={uncertainResult} defaultExpanded={true} />);

      // Must render Uncertain badge, NOT Done or blind success
      expect(screen.getByText("Uncertain")).toBeInTheDocument();
      expect(screen.queryByText("Done")).not.toBeInTheDocument();

      // Displays explanation and condition checked
      expect(screen.getByText(/Outcome Inconclusive \/ Uncertain/i)).toBeInTheDocument();
      expect(screen.getAllByText(/UI visual state transition/i).length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByText(/action 'left_click' at \(450, 320\)/i).length).toBeGreaterThanOrEqual(1);
    });

    it("renders Verified badge when postcondition is conclusively verified", () => {
      const toolCall: ToolCall = {
        id: "call_file_1",
        name: "write_file",
        arguments: { path: "/tmp/test.txt" },
      };

      const verifiedResult: ToolResult = {
        tool_call_id: "call_file_1",
        name: "write_file",
        output: JSON.stringify({
          verification: {
            status: "succeeded",
            condition_checked: "File existence and SHA-256 match",
            resource: "/tmp/test.txt",
            evidence: "File verified on disk (size: 42 bytes)",
          },
        }),
        is_error: false,
      };

      render(<ToolCallCard toolCall={toolCall} result={verifiedResult} defaultExpanded={true} />);

      expect(screen.getByText("Verified")).toBeInTheDocument();
      expect(screen.getByText(/Postcondition Verified/i)).toBeInTheDocument();
      expect(screen.getAllByText(/File existence and SHA-256 match/i).length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("TaskProgress Truthful Uncertainty Representation", () => {
    it("renders uncertain plan status badge and distinct amber warning styling", () => {
      const plan: TaskPlan = {
        id: "plan_test_1",
        runId: "run_test_1",
        conversationId: "conv_1",
        isBackendPlan: true,
        goal: "Deploy configuration and verify services",
        understanding: "Deploying config files",
        resources: ["/etc/nginx/nginx.conf"],
        steps: [
          {
            id: "step_inspect",
            text: "Inspect configuration",
            status: "completed",
            done: true,
          },
          {
            id: "step_execute",
            text: "Apply configuration",
            status: "uncertain",
            done: false,
            evidence: "Service reload dispatched but state change inconclusive",
          },
        ],
        status: "uncertain",
        accomplished: ["Inspected configuration"],
        remaining: ["Apply configuration (outcome uncertain)"],
      };

      render(<TaskProgress plan={plan} />);

      expect(screen.getByText("uncertain")).toBeInTheDocument();
      expect(screen.getAllByText(/Apply configuration/i).length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText(/Service reload dispatched but state change inconclusive/i)).toBeInTheDocument();
    });
  });
});
