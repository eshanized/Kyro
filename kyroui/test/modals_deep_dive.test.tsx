import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { invoke } from "@tauri-apps/api/core";

// Mock Tauri modules
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Components under test
import { SettingsModal } from "../components/SettingsModal";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { TeachTaskModal } from "../components/TeachTaskModal";

const mockedInvoke = vi.mocked(invoke);

describe("Modals Deep Dive & Edge Cases Coverage to >= 95%", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /* -------------------------------------------------------------
     1. SettingsModal Templates, Telegram, Sandbox, Diagnostics
     ------------------------------------------------------------- */
  describe("SettingsModal Advanced Tabs", () => {
    const mockHealth = {
      db_connected: true,
      display_server: "X11 (:0)",
      docker_available: true,
      ghost_xvfb_available: true,
      tools: { git: true, node: true, cargo: true, docker: true },
    };

    const mockStarters = [
      {
        name: "DevOps Starter",
        description: "Standard terminal tools and GitHub integration",
        schema_version: 1,
        bots: [],
        skills: [],
        routines: [],
      },
    ];

    it("handles export, import, starter pack install, telegram purge, and diagnostics", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_api_keys") {
          return {
            nvidia_configured: true,
            telegram_configured: true,
            telegram_chat_id: "987654321",
          };
        }
        if (cmd === "get_governance_policy") {
          return {
            require_approval_for_destructive: true,
            network_mode: "allow_all",
            blocked_domains: ["malicious.com"],
          };
        }
        if (cmd === "list_authorized_folders") {
          return [{ id: "f1", path: "/home/user/Documents", display_name: "Docs" }];
        }
        if (cmd === "get_browser_capabilities") {
          return { chrome_available: true, chromium_available: true, default_profile_exists: true };
        }
        if (cmd === "get_browser_settings") {
          return { headless: true, persistent_profile: true, stealth_mode: true };
        }
        if (cmd === "get_sandbox_status") {
          return { available: true, engine: "docker", message: "Docker 24.0.5 running" };
        }
        if (cmd === "get_system_health") return mockHealth;
        if (cmd === "get_starter_templates") return mockStarters;
        if (cmd === "export_workspace_templates") {
          return JSON.stringify({ schema_version: 1, bots: [] });
        }
        if (cmd === "import_workspace_templates") {
          return { bots_imported: 2, skills_imported: 3, routines_imported: 1 };
        }
        if (cmd === "delete_secret_key") return {};
        if (cmd === "save_api_keys") return {};
        if (cmd === "save_governance_policy") return {};
        return {};
      });

      // Mock URL.createObjectURL and URL.revokeObjectURL
      window.URL.createObjectURL = vi.fn(() => "blob:http://localhost/dummy");
      window.URL.revokeObjectURL = vi.fn();

      const onClose = vi.fn();
      const onSelectModel = vi.fn();

      render(
        <SettingsModal
          isOpen={true}
          onClose={onClose}
          selectedModel="nvidia/nemotron-3-ultra-550b-a55b"
          onSelectModel={onSelectModel}
          initialTab="templates"
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Export Workspace Pack")).toBeInTheDocument();
      });

      // Export template pack
      fireEvent.click(screen.getByText("Export Workspace Pack"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("export_workspace_templates", expect.anything());
        expect(screen.getByText(/Export completed/i)).toBeInTheDocument();
      });

      // Custom JSON Import
      const importTextarea = screen.getByPlaceholderText(/Paste {"schema_version": 1/i);
      fireEvent.change(importTextarea, {
        target: { value: '{"schema_version": 1, "bots": [{"name": "CustomBot"}]}' },
      });
      fireEvent.click(screen.getByText("Import Template Pack"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("import_workspace_templates", {
          jsonStr: '{"schema_version": 1, "bots": [{"name": "CustomBot"}]}',
        });
        expect(screen.getByText(/Imported: 2 bot\(s\)/i)).toBeInTheDocument();
      });

      // Starter Template Install
      fireEvent.click(screen.getByText("Install Pack"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("import_workspace_templates", expect.anything());
      });

      // Switch to Telegram tab
      fireEvent.click(screen.getByText("Telegram Bridge"));
      await waitFor(() => {
        expect(screen.getByText(/Telegram Remote Notification Bridge/i)).toBeInTheDocument();
      });

      // Purge Telegram Token
      const purgeBtn = await screen.findByText("Purge");
      fireEvent.click(purgeBtn);

      await waitFor(() => {
        expect(screen.getByText(/The telegram_bot_token key will be purged/i)).toBeInTheDocument();
      });

      // Confirm Purge
      fireEvent.click(screen.getByRole("button", { name: "Purge key" }));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("delete_secret_key", { key: "telegram_bot_token" });
      });

      // Switch to Container Sandbox tab
      fireEvent.click(screen.getByText("Container Sandbox"));
      await waitFor(() => {
        expect(screen.getByText(/Docker 24.0.5 running/i)).toBeInTheDocument();
      });

      // Switch to Diagnostics tab
      fireEvent.click(screen.getByText("Diagnostics"));
      await waitFor(() => {
        expect(screen.getByText("X11 (:0)")).toBeInTheDocument();
        expect(screen.getByText("Connected & WAL Active")).toBeInTheDocument();
        expect(screen.getByText("Xvfb Binary Ready (:99)")).toBeInTheDocument();
      });

      // Save settings
      fireEvent.click(screen.getByText("Save Settings"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("save_api_keys", expect.anything());
        expect(mockedInvoke).toHaveBeenCalledWith("save_governance_policy", expect.anything());
      });
    });
  });

  /* -------------------------------------------------------------
     2. ConnectionsModal Remote MCP and Policy Governance
     ------------------------------------------------------------- */
  describe("ConnectionsModal Advanced", () => {
    const mockCatalog = [
      {
        id: "remote_mcp",
        name: "Remote MCP Server",
        description: "Connect remote server via SSE / HTTP",
        connector_type: "mcp",
        auth_type: "token",
        category: "mcp",
        capabilities: ["Remote Tools", "SSE Streaming"],
        is_available: true,
      },
      {
        id: "github",
        name: "GitHub",
        description: "GitHub Issues and PRs",
        connector_type: "native",
        auth_type: "oauth",
        category: "developer",
        capabilities: ["Issues", "Repos"],
        is_available: true,
      },
    ];

    const mockAccount = {
      id: "acc-gh-1",
      connector_id: "github",
      display_name: "Personal GitHub",
      account_identifier: "user-gh",
      is_healthy: true,
      status: "connected",
      auth_type: "token",
      connected_at: Date.now() - 86400000,
    };

    const mockTools = [
      {
        name: "github_create_issue",
        original_name: "create_issue",
        description: "Create a GitHub issue",
        risk_level: "destructive",
        is_read_only: false,
        can_auto_approve: false,
        default_policy: "ask",
      },
      {
        name: "github_list_repos",
        original_name: "list_repos",
        description: "List public repositories",
        risk_level: "safe",
        is_read_only: true,
        can_auto_approve: true,
        default_policy: "allow",
      },
    ];

    it("connects remote MCP, manages policy governance choices, and handles disconnection", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_connector_catalog") return mockCatalog;
        if (cmd === "list_connected_accounts") return [mockAccount];
        if (cmd === "list_mcp_servers") return [];
        if (cmd === "get_mcp_server_statuses") return {};
        if (cmd === "connect_account_token") return { id: "new-acc" };
        if (cmd === "list_account_tool_policies") {
          return [
            {
              account_id: "acc-gh-1",
              tool_name: "create_issue",
              policy: "ask",
              risk_level: "destructive",
            },
          ];
        }
        if (cmd === "get_account_tools") return mockTools;
        if (cmd === "save_account_tool_policy") return {};
        if (cmd === "test_connector_connection") return "Connection valid (200 OK)";
        if (cmd === "disconnect_account") return {};
        return {};
      });

      // Window confirm mock
      window.confirm = vi.fn(() => true);

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("Discover Catalog")).toBeInTheDocument();
      });

      // Open connect modal for Remote MCP
      const connectBtns = screen.getAllByRole("button", { name: /Connect Account/i });
      fireEvent.click(connectBtns[0]);

      await waitFor(() => {
        expect(screen.getByText("Connect Remote MCP Server")).toBeInTheDocument();
      });

      // Fill Remote MCP form
      const nameInput = screen.getByPlaceholderText(/Work GitHub or Personal/i);
      fireEvent.change(nameInput, { target: { value: "My Remote Server" } });

      const urlInput = screen.getByPlaceholderText(/http:\/\/127.0.0.1:8000\/mcp/i);
      fireEvent.change(urlInput, { target: { value: "https://mcp.internal.net/sse" } });

      const privateCheck = screen.getByLabelText(/Allow private \/ local intranet network hosts/i);
      fireEvent.click(privateCheck);

      const bearerInput = screen.getByPlaceholderText(/Secret bearer token/i);
      fireEvent.change(bearerInput, { target: { value: "bearer_secret_12345" } });

      fireEvent.click(screen.getByText("Save & Connect"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("connect_account_token", {
          connectorId: "remote_mcp",
          displayName: "My Remote Server",
          accountIdentifier: "default",
          tokenSecret: "bearer_secret_12345",
          config: {
            server_url: "https://mcp.internal.net/sse",
            allow_private_network: true,
          },
        });
      });

      // Switch to Connected Accounts tab
      fireEvent.click(screen.getByText(/Connected Accounts/i));
      await waitFor(() => {
        expect(screen.getByText("Personal GitHub")).toBeInTheDocument();
      });

      // Test Connection
      fireEvent.click(screen.getByTitle("Test live connection to API"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("test_connector_connection", { accountId: "acc-gh-1" });
      });

      // Manage Tool Policies
      fireEvent.click(screen.getByTitle("Configure action approval rules and risk policies"));
      await waitFor(() => {
        expect(screen.getByText(/Action Governance: Personal GitHub/i)).toBeInTheDocument();
        expect(screen.getByText("create_issue")).toBeInTheDocument();
      });

      // Set tool policies (Deny, Ask Always)
      const denyBtns = screen.getAllByText("Deny");
      fireEvent.click(denyBtns[0]);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("save_account_tool_policy", {
          accountId: "acc-gh-1",
          toolName: "create_issue",
          policy: "deny",
          riskLevel: "destructive",
        });
      });

      const askBtns = screen.getAllByText("Ask Always");
      fireEvent.click(askBtns[0]);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("save_account_tool_policy", {
          accountId: "acc-gh-1",
          toolName: "create_issue",
          policy: "ask",
          riskLevel: "destructive",
        });
      });

      // Close policy modal
      const closeButtons = screen.getAllByRole("button", { name: "" }); // close X icon
      if (closeButtons.length > 0) {
        fireEvent.click(closeButtons[0]);
      }

      // Disconnect account
      const disconnectBtn = screen.getByTitle("Safely remove account and purge vault credentials");
      fireEvent.click(disconnectBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("disconnect_account", { accountId: "acc-gh-1" });
      });
    });
  });

  /* -------------------------------------------------------------
     3. DevWorkbenchModal Checkpoints, Build Doctor, and Audits
     ------------------------------------------------------------- */
  describe("DevWorkbenchModal Advanced Features", () => {
    const mockCheckpoints = [
      {
        checkpoint_id: "chk-1",
        description: "Feature Checkpoint",
        created_at: Math.floor(Date.now() / 1000) - 3600,
        status: "active" as const,
        modified_files: [
          {
            file_path: "src/server.rs",
            additions: 12,
            deletions: 2,
            risk_assessment: "medium",
            diff_patch: "@@ -1,3 +1,5 @@",
          },
        ],
      },
    ];

    const mockDiagnostic = {
      category: "type_error" as const,
      error_code: "E0308",
      root_cause_error: "mismatched types: expected u64, found i32",
      suggested_repair: "use `as u64`",
      file: "src/server.rs",
      line_number: 42,
      reproduced: true,
    };

    const mockBaseline = {
      project_path: "/home/snigdha/Desktop/Kyro",
      route: "/",
      viewport: "desktop",
      base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      created_at: Math.floor(Date.now() / 1000) - 7200,
      artifact_id: null,
    };

    it("creates checkpoints, rolls back code, parses compiler diagnostics, and audits baselines", async () => {
      Object.assign(navigator, {
        clipboard: {
          writeText: vi.fn().mockResolvedValue(undefined),
        },
      });

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_dev_servers") return [];
        if (cmd === "create_code_checkpoint") return mockCheckpoints[0];
        if (cmd === "rollback_code_checkpoint") {
          return {
            checkpoint_id: "chk-1",
            status: "success",
            files_reverted: ["src/server.rs"],
            conflicts_detected: [],
          };
        }
        if (cmd === "diagnose_build_failure") return mockDiagnostic;
        if (cmd === "get_audit_baseline") return mockBaseline;
        if (cmd === "set_audit_baseline") return mockBaseline;
        if (cmd === "compare_audit_baseline") {
          return {
            route: "/",
            viewport: "desktop",
            diff_score: 0.0,
            has_regression: false,
            diff_base64_png: "diff_png",
          };
        }
        return {};
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="checkpoints" />);

      await waitFor(() => {
        expect(screen.getByText("Create Reversible Code Checkpoint")).toBeInTheDocument();
      });

      // Create new checkpoint
      const descInput = screen.getByDisplayValue("Pre-refactor checkpoint");
      fireEvent.change(descInput, { target: { value: "Refactored Tauri handlers" } });
      fireEvent.click(screen.getByText("Save Checkpoint"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("create_code_checkpoint", expect.anything());
        expect(screen.getByText("Feature Checkpoint")).toBeInTheDocument();
      });

      // Rollback checkpoint
      const rollbackBtn = screen.getByText("1-Click Rollback");
      fireEvent.click(rollbackBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("rollback_code_checkpoint", expect.anything());
        expect(screen.getByText(/Checkpoint Rollback SUCCESS/i)).toBeInTheDocument();
      });

      // Switch to Build Doctor tab
      fireEvent.click(screen.getByText("Build Doctor"));
      await waitFor(() => {
        expect(screen.getByText("Run Build Doctor")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Run Build Doctor"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("diagnose_build_failure", expect.anything());
        expect(screen.getByText("TYPE_ERROR")).toBeInTheDocument();
        expect(screen.getByText("E0308")).toBeInTheDocument();
        expect(screen.getByText(/mismatched types: expected u64/i)).toBeInTheDocument();
      });

      // Copy Repair
      fireEvent.click(screen.getByText("Copy Repair"));
      await waitFor(() => {
        expect(screen.getByText("Copied")).toBeInTheDocument();
      });

      // Switch to UI Audit & Baselines tab
      fireEvent.click(screen.getByText("UI Audit & Baselines"));
      await waitFor(() => {
        expect(screen.getByText("Save As Baseline")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Save As Baseline"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("set_audit_baseline", expect.anything());
        expect(screen.getByText(/Saved baseline for \/ \[desktop\]/i)).toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     4. TeachTaskModal Export, Deletion, and Scheduling
     ------------------------------------------------------------- */
  describe("TeachTaskModal Extended", () => {
    const mockSkills = [
      {
        id: "skill-rec-1",
        name: "Export Invoices to Sheets",
        description: "Replays clicking invoices folder and copying totals",
        steps_json: JSON.stringify([
          { action_type: "click", x: 100, y: 200, delay_ms: 100 },
          { action_type: "type", text: "report.csv", delay_ms: 50 },
        ]),
        created_at: Date.now() - 7200000,
      },
    ];

    it("lists skills, searches, replays, and triggers deletion", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_skills") return mockSkills;
        if (cmd === "list_routines") return [];
        if (cmd === "replay_skill") return {};
        if (cmd === "delete_skill") return {};
        return {};
      });

      render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="skills" />);

      await waitFor(() => {
        expect(screen.getByText("Export Invoices to Sheets")).toBeInTheDocument();
      });

      // Search recorded skills
      const searchInput = screen.getByPlaceholderText(/Search saved skills/i);
      fireEvent.change(searchInput, { target: { value: "Invoices" } });
      expect(screen.getByText("Export Invoices to Sheets")).toBeInTheDocument();

      // Click the skill to view details
      fireEvent.click(screen.getByText("Export Invoices to Sheets"));
      await waitFor(() => {
        expect(screen.getByText("Recorded Steps (2)")).toBeInTheDocument();
      });

      // Replay skill
      const replayBtns = screen.getAllByRole("button", { name: /Replay/i });
      fireEvent.click(replayBtns[0]);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("replay_skill", { skillId: "skill-rec-1" });
      });

      // Delete skill
      const deleteBtns = screen.getAllByTitle("Delete task");
      fireEvent.click(deleteBtns[0]);
      await waitFor(() => {
        expect(screen.getByText("Delete task?")).toBeInTheDocument();
      });

      const confirmDeleteBtn = screen.getByTestId("confirm-delete-task-button");
      fireEvent.click(confirmDeleteBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("delete_skill", { id: "skill-rec-1" });
      });
    });
  });
});
