import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { App } from "../App";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { SettingsModal } from "../components/SettingsModal";
import { TodayDashboardView } from "../components/TodayDashboardView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({ label: "main" })),
}));

// Mock @xterm/xterm and addon-fit
vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class {
      open = vi.fn();
      loadAddon = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      dispose = vi.fn();
      rows = 24;
      cols = 80;
      onData = vi.fn(() => ({ dispose: vi.fn() }));
    },
  };
});

vi.mock("@xterm/addon-fit", () => {
  return {
    FitAddon: class {
      fit = vi.fn();
    },
  };
});

describe("Frontend 95%+ Target Finalization Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  describe("ConnectionsModal Governance Dialog and OAuth Form Cancellation", () => {
    const mockAccounts = [
      {
        id: "acc_gh_1",
        connector_id: "github",
        display_name: "octocat@github",
        account_identifier: "octocat",
        status: "connected",
        created_at: 1000,
        last_synced_at: 1000,
        metadata_json: "{}",
      },
    ];

    const mockDefinitions = [
      {
        id: "github",
        name: "GitHub",
        connector_type: "native" as const,
        auth_type: "pat_or_oauth" as const,
        description: "Pull requests, issues, repo management",
        icon: "github",
        status: "available" as const,
        is_first_party: true,
        is_available: true,
      },
    ];

    const mockTools = [
      {
        name: "github:list_issues",
        original_name: "list_issues",
        description: "Lists open repository issues",
        is_read_only: true,
        risk_level: "safe" as const,
        default_policy: "allow" as const,
        can_auto_approve: true,
      },
      {
        name: "github:delete_repo",
        original_name: "delete_repo",
        description: "Permanently deletes a repository",
        is_read_only: false,
        risk_level: "destructive" as const,
        default_policy: "ask" as const,
        can_auto_approve: false,
      },
    ];

    it("opens Action Governance dialog, switches tool policies, and clicks Done", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_connector_catalog") return Promise.resolve(mockDefinitions);
        if (cmd === "list_connected_accounts") return Promise.resolve(mockAccounts);
        if (cmd === "list_mcp_servers") return Promise.resolve([]);
        if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
        if (cmd === "get_account_tools") return Promise.resolve(mockTools);
        if (cmd === "list_account_tool_policies") return Promise.resolve([]);
        if (cmd === "save_account_tool_policy") return Promise.resolve();
        return Promise.resolve([]);
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="connected" />);

      await waitFor(() => {
        expect(screen.getByText("octocat@github")).toBeInTheDocument();
      });

      // Click Policies button
      const manageBtn = screen.getByTitle("Configure action approval rules and risk policies");
      fireEvent.click(manageBtn);

      await waitFor(() => {
        expect(screen.getByText(/Action Governance: octocat@github/i)).toBeInTheDocument();
        expect(screen.getByText("list_issues")).toBeInTheDocument();
        expect(screen.getByText("delete_repo")).toBeInTheDocument();
      });

      // List Issues is low risk -> Allow button is clickable
      const allowBtns = screen.getAllByText("Allow");
      fireEvent.click(allowBtns[0]);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("save_account_tool_policy", expect.objectContaining({
          policy: "allow",
        }));
      });

      // Deny button
      const denyBtns = screen.getAllByText("Deny");
      fireEvent.click(denyBtns[0]);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("save_account_tool_policy", expect.objectContaining({
          policy: "deny",
        }));
      });

      // Close Governance Dialog via Done button
      const doneBtn = screen.getByText("Done");
      fireEvent.click(doneBtn);

      await waitFor(() => {
        expect(screen.queryByText(/Action Governance: octocat@github/i)).not.toBeInTheDocument();
      });
    });

    it("opens GitHub OAuth form, inputs client secret, and cancels", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_connector_catalog") return Promise.resolve(mockDefinitions);
        if (cmd === "list_connected_accounts") return Promise.resolve([]);
        if (cmd === "list_mcp_servers") return Promise.resolve([]);
        if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
        return Promise.resolve([]);
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);

      await waitFor(() => {
        expect(screen.getByText("Connect Account")).toBeInTheDocument();
      });

      const connectBtn = screen.getByText("Connect Account");
      fireEvent.click(connectBtn);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Optional confidential client secret/i)).toBeInTheDocument();
      });

      const secretInput = screen.getByPlaceholderText(/Optional confidential client secret/i);
      fireEvent.change(secretInput, { target: { value: "secret_client_test" } });
      expect(secretInput).toHaveValue("secret_client_test");

      // Cancel form
      const cancelBtn = screen.getByText("Cancel");
      fireEvent.click(cancelBtn);

      await waitFor(() => {
        expect(screen.queryByPlaceholderText(/Optional confidential client secret/i)).not.toBeInTheDocument();
      });
    });
  });

  describe("DevWorkbenchModal Checkpoints & Dev Servers Additional Paths", () => {
    it("handles dev server start, view logs, and stop", async () => {
      const mockDiscovery = {
        detected_framework: "vite",
        suggested_commands: ["npm run dev"],
        candidate_projects: ["/home/snigdha/Desktop/Kyro"],
      };

      const mockDevServers = [
        {
          id: "srv_dev_1",
          project_path: "/home/snigdha/Desktop/Kyro",
          framework: "Vite",
          status: "ready",
          port: 5173,
          pid: 12345,
          started_at: 1000,
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "discover_dev_projects") return Promise.resolve(mockDiscovery);
        if (cmd === "list_dev_servers") return Promise.resolve(mockDevServers);
        if (cmd === "get_dev_server_logs") return Promise.resolve(["[vite] server running on port 5173", "[vite] ready in 150ms"]);
        if (cmd === "stop_dev_server") return Promise.resolve();
        return Promise.resolve([]);
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="projects" />);

      await waitFor(() => {
        expect(screen.getByText(/PID: 12345/i)).toBeInTheDocument();
      });

      // View Logs (button text is "Logs")
      const logsBtn = screen.getByText("Logs");
      fireEvent.click(logsBtn);

      await waitFor(() => {
        expect(screen.getByText("[vite] server running on port 5173")).toBeInTheDocument();
      });

      // Stop dev server
      const stopBtn = screen.getByText("Stop");
      fireEvent.click(stopBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("stop_dev_server", expect.objectContaining({ serverId: "srv_dev_1" }));
      });
    });

    it("handles code checkpoints rollback in Checkpoints tab", async () => {
      const mockCheckpoint = {
        checkpoint_id: "cp_test_123",
        workspace_path: "/home/snigdha/Desktop/Kyro",
        description: "Safety Checkpoint Before Refactor",
        created_at: 1700000000,
        status: "active",
        files_count: 5,
        modified_files: ["src/lib.rs", "src/main.rs"],
      };

      const mockRollback = {
        checkpoint_id: "cp_test_123",
        status: "success",
        files_reverted: ["src/lib.rs", "src/main.rs"],
        conflicts_detected: [],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "create_code_checkpoint") return Promise.resolve(mockCheckpoint);
        if (cmd === "rollback_code_checkpoint") return Promise.resolve(mockRollback);
        return Promise.resolve([]);
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="checkpoints" />);

      // Create checkpoint
      const saveCpBtn = screen.getByText("Save Checkpoint");
      fireEvent.click(saveCpBtn);

      await waitFor(() => {
        expect(screen.getByText("Safety Checkpoint Before Refactor")).toBeInTheDocument();
      });

      const rollbackBtn = screen.getByText("1-Click Rollback");
      fireEvent.click(rollbackBtn);

      await waitFor(() => {
        expect(screen.getByText(/Checkpoint Rollback SUCCESS/i)).toBeInTheDocument();
      });
    });
  });

  describe("TodayDashboardView Briefing & Action Triggers", () => {
    it("renders briefing stats, triggers refresh, and launches privacy settings", async () => {
      const mockToday = {
        greeting: "Good afternoon",
        date_display: "Wednesday, Oct 22, 2026",
        upcoming_events: [
          {
            title: "Sprint Planning",
            start_time: "14:00",
            end_time: "15:00",
            location: "Google Meet",
          },
        ],
        email_summary: {
          is_connected: true,
          unread_count: 3,
          recent_subjects: ["GitHub: CI passed", "Billing update"],
        },
        scheduled_routines: [],
        pending_approvals: [],
        automation_metrics: {
          total_runs: 10,
          successful_runs: 10,
          failed_runs: 0,
          success_rate: 100,
          total_hours_saved: 4.5,
          active_routines_count: 3,
        },
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_today_dashboard_data") return Promise.resolve(mockToday);
        return Promise.resolve(null);
      });

      const handleSettings = vi.fn();
      const handleAsk = vi.fn();
      const handleTool = vi.fn();

      render(
        <TodayDashboardView
          onOpenSettings={handleSettings}
          onAskPrompt={handleAsk}
          onOpenTool={handleTool}
          onOpenConnections={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Good afternoon!")).toBeInTheDocument();
        expect(screen.getByText("Sprint Planning")).toBeInTheDocument();
        expect(screen.getByText(/GitHub: CI passed/i)).toBeInTheDocument();
      });

      // Click Refresh
      const refreshBtn = screen.getByTitle("Refresh dashboard data");
      fireEvent.click(refreshBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("get_today_dashboard_data");
      });

      // Click Privacy & Settings
      const privacyBtn = screen.getByText("Privacy & Settings");
      fireEvent.click(privacyBtn);

      expect(handleSettings).toHaveBeenCalledWith("privacy");
    });
  });

  describe("App CommandPalette Global Short-Circuit Triggers", () => {
    it("handles quick actions button click and opens MCP modal from palette", async () => {
      (invoke as any).mockImplementation(async (cmd: string) => {
        if (cmd === "list_conversations") return [{ id: "c1", title: "Test Conv", model: "gpt-4o", created_at: 1000, updated_at: 1000 }];
        if (cmd === "get_messages") return [];
        if (cmd === "list_bots") return [{ id: "b1", name: "Bot 1", avatar: "🤖", role: "coder", description: "coder", system_prompt: "" }];
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "get_takeover_status") return { is_active: false };
        if (cmd === "get_agent_state") return { is_running: false };
        if (cmd === "search_workspace") return [];
        if (cmd === "list_mcp_servers") return [];
        if (cmd === "get_today_dashboard_data") {
          return {
            date_display: "Wednesday, Oct 22",
            greeting: "Good afternoon",
            summary_stats: { routines_executed: 0, pending_approvals: 0, hours_saved: 0 },
            pending_approvals: [],
            routines_today: [],
            recent_activity: [],
          };
        }
        if (cmd === "list_routines") return [];
        if (cmd === "list_routine_history") return [];
        if (cmd === "get_automation_metrics") {
          return { total_runs: 0, successful_runs: 0, failed_runs: 0, success_rate: 100, total_hours_saved: 0, active_routines_count: 0 };
        }
        return [];
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("Test Conv")).toBeInTheDocument();
      });

      // Click Quick Actions & Search button
      const quickActionBtn = screen.getByTitle("Quick Actions & Search (Ctrl+K)");
      fireEvent.click(quickActionBtn);

      await waitFor(() => {
        expect(screen.getByText("MCP Tool Servers")).toBeInTheDocument();
      });

      // Select MCP Tool Servers
      const mcpOption = screen.getByText("MCP Tool Servers");
      fireEvent.click(mcpOption);

      await waitFor(() => {
        expect(screen.getByText("Model Context Protocol (MCP) Tools")).toBeInTheDocument();
      });
    });
  });
});
