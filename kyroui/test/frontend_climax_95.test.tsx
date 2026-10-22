import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { ConnectionsModal, parseArgsSafe } from "../components/ConnectionsModal";
import { MyRoutinesView, formatCronDescription } from "../components/MyRoutinesView";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { SettingsModal } from "../components/SettingsModal";

// Mock Tauri APIs
const mockedInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: any[]) => mockedInvoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn(),
}));

// Mock window.matchMedia
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

describe("Frontend Climax to >= 95% Coverage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_conversations") return [{ id: "c1", title: "Test Conv", model: "nemotron" }];
      if (cmd === "get_messages") return [];
      if (cmd === "get_api_keys") return { openai: "sk-...", anthropic: "" };
      if (cmd === "get_governance_policy") return { shell_mode: "ask", computer_use_mode: "ask", blocked_domains: [] };
      if (cmd === "list_authorized_folders") return [{ id: "f1", path: "/test/docs", added_at: 1000 }];
      if (cmd === "list_connector_catalog" || cmd === "get_connector_catalog") {
        return [
          {
            id: "github",
            name: "GitHub Native",
            description: "Code and repos",
            connector_type: "native",
            auth_type: "token",
            capabilities: ["git", "issues"],
            is_available: true,
          },
          {
            id: "remote_mcp",
            name: "Remote SSE MCP",
            description: "Remote cloud server",
            connector_type: "remote_mcp",
            auth_type: "token",
            capabilities: ["custom_mcp"],
            is_available: true,
          },
        ];
      }
      if (cmd === "list_connected_accounts") {
        return [
          {
            id: "acc_1",
            connector_id: "github",
            display_name: "Work GitHub",
            account_identifier: "octocat",
            status: "connected",
            connected_at: 1000,
            last_used_at: null,
            scopes: ["repo"],
            metadata_json: null,
          },
        ];
      }
      if (cmd === "list_mcp_servers") return [];
      if (cmd === "get_mcp_server_statuses") return {};
      if (cmd === "list_bots") {
        return [
          {
            id: "apex",
            name: "Apex Orchestrator",
            avatar: "bot",
            role: "Autonomous orchestrator",
            system_prompt: "You are Kyro Apex",
            is_default: true,
          },
        ];
      }
      if (cmd === "get_roster_statuses") return [];
      if (cmd === "list_routines") return [];
      if (cmd === "list_routine_runs") return [];
      if (cmd === "list_skills") return [];
      if (cmd === "get_today_dashboard") {
        return {
          date: "2026-10-22",
          pending_routines: [],
          recent_files: [],
          active_projects: [],
          metrics: { completed_today: 0, pending_approvals: 0, time_saved_minutes: 0 },
        };
      }
      if (cmd === "get_browser_capabilities") {
        return {
          browsers: [
            { name: "Firefox", path: "/usr/bin/firefox", available: true, automatable: true },
          ],
          default_browser: "Firefox",
        };
      }
      if (cmd === "get_system_health") {
        return {
          status: "healthy",
          components: { db: "ok", vault: "ok" },
          version: "0.1.1",
        };
      }
      return {};
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  /* --------------------------------------------------------------------------
     1. ConnectionsModal: parseArgsSafe Unit Tests & Component Flows
     -------------------------------------------------------------------------- */
  it("thoroughly tests parseArgsSafe parsing branches", () => {
    expect(parseArgsSafe("")).toEqual([]);
    expect(parseArgsSafe("   ")).toEqual([]);
    expect(parseArgsSafe('["--port", "8080", "app.js"]')).toEqual(["--port", "8080", "app.js"]);
    expect(parseArgsSafe("[invalid json")).toEqual(["[invalid", "json"]);
    expect(parseArgsSafe('--flag "double quoted value" \'single quoted value\' plain')).toEqual([
      "--flag",
      "double quoted value",
      "single quoted value",
      "plain",
    ]);
  });

  it("exercises ConnectionsModal filtering, token connect, policies, and disconnect", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_connector_catalog" || cmd === "get_connector_catalog") {
        return [
          {
            id: "github",
            name: "GitHub Native",
            description: "Code and repos",
            connector_type: "native",
            auth_type: "token",
            capabilities: ["git", "issues"],
            is_available: true,
          },
          {
            id: "remote_mcp",
            name: "Remote SSE MCP",
            description: "Remote cloud server",
            connector_type: "remote_mcp",
            auth_type: "token",
            capabilities: ["custom_mcp"],
            is_available: true,
          },
        ];
      }
      if (cmd === "list_connected_accounts") {
        return [
          {
            id: "acc_gh",
            connector_id: "github",
            display_name: "GitHub Main",
            account_identifier: "user1",
            status: "connected",
            connected_at: 1000,
            last_used_at: null,
            scopes: ["repo"],
            metadata_json: null,
          },
        ];
      }
      if (cmd === "list_mcp_servers") {
        return [
          {
            id: "mcp1",
            name: "Test MCP",
            command: "npx",
            args_json: '["--port", "3000"]',
            env_json: "{}",
            enabled: true,
          },
        ];
      }
      if (cmd === "get_mcp_server_statuses") return { mcp1: "running" };
      if (cmd === "list_account_tool_policies") {
        return [{ id: "p1", account_id: "acc_gh", tool_name: "create_issue", policy: "ask", updated_at: 1000 }];
      }
      if (cmd === "get_account_tools") {
        return [
          {
            name: "github__create_issue",
            original_name: "create_issue",
            description: "Create an issue",
            risk_level: "safe",
            default_policy: "ask",
            can_auto_approve: true,
            is_read_only: false,
          },
        ];
      }
      if (cmd === "test_connector_connection") return "Connection OK (200ms)";
      if (cmd === "disconnect_account") return {};
      if (cmd === "set_account_tool_policy") return {};
      if (cmd === "create_mcp_server") return {};
      return {};
    });

    const onClose = vi.fn();
    render(<ConnectionsModal isOpen={true} onClose={onClose} initialTab="discover" />);

    // Search query & filtering
    const searchInput = screen.getByPlaceholderText(/Search integrations/i);
    await waitFor(() => {
      expect(screen.getByText("GitHub Native")).toBeInTheDocument();
    });
    fireEvent.change(searchInput, { target: { value: "issues" } });
    expect(screen.getByText("GitHub Native")).toBeInTheDocument();

    // Switch categories
    fireEvent.click(screen.getByRole("button", { name: "Native API" }));
    fireEvent.click(screen.getByRole("button", { name: "MCP Protocols" }));
    fireEvent.click(screen.getByRole("button", { name: "All" }));
    fireEvent.change(searchInput, { target: { value: "" } });

    // Switch to connected tab
    fireEvent.click(screen.getByRole("button", { name: /Connected Accounts/i }));
    await waitFor(() => {
      expect(screen.getByText("GitHub Main")).toBeInTheDocument();
    });

    // Test connection
    fireEvent.click(screen.getByRole("button", { name: /^Test$/i }));

    // Manage policies drawer
    fireEvent.click(screen.getByRole("button", { name: /^Policies$/i }));
    await waitFor(() => {
      expect(screen.getByText("create_issue")).toBeInTheDocument();
    });
    // Set policy to Allow
    const allowBtn = screen.getByRole("button", { name: "Allow" });
    fireEvent.click(allowBtn);

    // Close manage policies dialog
    fireEvent.click(screen.getByTitle("Close governance"));
    await waitFor(() => {
      expect(screen.queryByTitle("Close governance")).not.toBeInTheDocument();
    });

    // Disconnect account
    vi.spyOn(window, "confirm").mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: /^Disconnect$/i }));

    // Switch to Local MCP Servers tab
    fireEvent.click(screen.getByRole("button", { name: /Local MCP Servers/i }));
    await waitFor(() => {
      expect(screen.getByText("Test MCP")).toBeInTheDocument();
    });

    // Add local MCP server with parseArgsSafe inputs
    fireEvent.click(screen.getByRole("button", { name: /Add Local MCP Server/i }));
    fireEvent.change(screen.getByPlaceholderText(/e\.g\. Local Filesystem Server/i), {
      target: { value: "Advanced MCP" },
    });
    fireEvent.change(screen.getByPlaceholderText(/npx or python/i), {
      target: { value: "node" },
    });
    fireEvent.change(screen.getByPlaceholderText(/-y @modelcontextprotocol/i), {
      target: { value: '["--port", "8080", "server.js"]' },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save & Register/i }));
  });

  /* --------------------------------------------------------------------------
     2. MyRoutinesView: Complete formatCronDescription Unit Tests & Suggestion
     -------------------------------------------------------------------------- */
  it("thoroughly tests all formatCronDescription branches", () => {
    expect(formatCronDescription("* *")).toBe("* *");
    expect(formatCronDescription("* * * * *")).toBe("Every minute");
    expect(formatCronDescription("0 * * * *")).toBe("Every hour at minute 0");
    expect(formatCronDescription("0 9 * * 5")).toBe("Every Friday at 9:00 AM");
    expect(formatCronDescription("0 0 1 * *")).toBe("First day of every month at midnight");
    expect(formatCronDescription("0 9 1 * *")).toBe("First day of every month at 9:00 AM");
    expect(formatCronDescription("0 0 1 1 *")).toBe("Single manual trigger / run once on request");
    expect(formatCronDescription("30 8 * * 1-5")).toBe("Weekdays (Mon-Fri) at 08:30");
    expect(formatCronDescription("15 14 * * *")).toBe("Every day at 14:15");
    expect(formatCronDescription("0 12 15 * *")).toBe("Day 15 of the month at 12:00");
    expect(formatCronDescription("0 10 * * 1")).toBe("Every Monday at 10:00");
    expect(formatCronDescription("5 4 * * 2")).toBe("Custom schedule (5 4 * * 2) at 04:05");
  });

  it("exercises MyRoutinesView suggestion error notice and empty prompt", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_routines") return [];
      if (cmd === "list_routine_runs") return [];
      if (cmd === "natural_language_to_routine") {
        throw new Error("Unable to parse ambiguity");
      }
      return [];
    });

    render(<MyRoutinesView onAskPrompt={vi.fn()} />);

    // Test empty prompt click does not trigger error
    fireEvent.click(screen.getByRole("button", { name: "Build Routine" }));

    // Test prompt with error
    const promptInput = screen.getByPlaceholderText(/Every Friday at 9am/i);
    fireEvent.change(promptInput, { target: { value: "invalid prompt that fails" } });
    fireEvent.click(screen.getByRole("button", { name: "Build Routine" }));

    await waitFor(() => {
      expect(screen.getByText(/Suggestion error:/i)).toBeInTheDocument();
    });
  });

  /* --------------------------------------------------------------------------
     3. TeachTaskModal: Highlighted item finding, errors on start/stop/replay
     -------------------------------------------------------------------------- */
  it("exercises TeachTaskModal highlight navigation and failure error notices", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_skills") {
        return [
          {
            id: "sk_highlight",
            name: "Grounded Skill",
            description: "Targeted skill",
            steps_json: "[]",
            created_at: 1000,
          },
        ];
      }
      if (cmd === "list_routines") {
        return [
          {
            id: "rt_highlight",
            name: "Target Routine",
            cron_expression: "0 0 * * *",
            enabled: true,
            created_at: 1000,
          },
        ];
      }
      if (cmd === "start_teach_task") {
        throw new Error("X11 recorder failed");
      }
      if (cmd === "replay_skill") {
        throw new Error("Replay verification failed");
      }
      return [];
    });

    const { rerender } = render(
      <TeachTaskModal
        isOpen={true}
        onClose={vi.fn()}
        highlightTaskId="sk_highlight"
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Grounded Skill")).toBeInTheDocument();
    });

    // Replay error test
    const replayButtons = screen.getAllByRole("button", { name: /^Replay$/i });
    fireEvent.click(replayButtons[0]);
    await waitFor(() => {
      expect(screen.getByText(/Error replaying skill:/i)).toBeInTheDocument();
    });

    // Start recording with empty name error
    fireEvent.click(screen.getByRole("button", { name: /Record Workflow/i }));
    fireEvent.click(screen.getByRole("button", { name: /Start Recording Workflow/i }));
    await waitFor(() => {
      expect(screen.getByText("Please provide a name for this workflow.")).toBeInTheDocument();
    });

    // Start recording with failure
    fireEvent.change(screen.getByPlaceholderText(/e\.g\. Export Daily Analytics Spreadsheet/i), {
      target: { value: "Record Sales" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Start Recording Workflow/i }));
    await waitFor(() => {
      expect(screen.getByText(/Error starting recording:/i)).toBeInTheDocument();
    });

    // Highlight routine tab
    rerender(
      <TeachTaskModal
        isOpen={true}
        onClose={vi.fn()}
        highlightTaskId="rt_highlight"
      />
    );
    await waitFor(() => {
      expect(screen.getAllByText("Target Routine").length).toBeGreaterThanOrEqual(1);
    });
  });

  /* --------------------------------------------------------------------------
     4. SettingsModal: Refresh browser capabilities & authorized folder errors
     -------------------------------------------------------------------------- */
  it("exercises SettingsModal browser discovery refresh and folder validation", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_api_keys") return { openai: "key" };
      if (cmd === "get_governance_policy") return { shell_mode: "ask", computer_use_mode: "ask" };
      if (cmd === "list_authorized_folders") return [{ id: "f1", path: "/var/log" }];
      if (cmd === "add_authorized_folder") throw new Error("Permission denied for folder");
      if (cmd === "remove_authorized_folder") throw new Error("Folder locked");
      if (cmd === "get_browser_capabilities") {
        return {
          browsers: [
            { name: "Chromium", path: "/usr/bin/chromium", available: true, automatable: true },
          ],
          default_browser: "Chromium",
        };
      }
      return {};
    });

    render(<SettingsModal isOpen={true} onClose={vi.fn()} initialTab="browser" />);

    // Browser discovery refresh
    await waitFor(() => {
      expect(screen.getByText("Browser Automation & Capability Matrix")).toBeInTheDocument();
    });
    const refreshBtn = screen.getByRole("button", { name: /^Refresh$/i });
    fireEvent.click(refreshBtn);
    await waitFor(() => {
      expect(screen.getByText("Discovered browser capabilities refreshed.")).toBeInTheDocument();
    });

    // Privacy & Folders errors
    fireEvent.click(screen.getByRole("button", { name: /Privacy & Folders/i }));
    await waitFor(() => {
      expect(screen.getByText("Authorized Folders")).toBeInTheDocument();
    });

    // Authorize folder empty check
    const addBtn = screen.getByRole("button", { name: /Authorize Folder/i });
    fireEvent.click(addBtn);

    // Authorize folder error notice
    const folderInput = screen.getByPlaceholderText(/Folder path \(e\.g\. \/home\/user\/Documents\)/i);
    fireEvent.change(folderInput, { target: { value: "/root/secret" } });
    fireEvent.click(addBtn);
    await waitFor(() => {
      expect(screen.getByText(/Failed to authorize folder:/i)).toBeInTheDocument();
    });

    // Remove folder error notice
    const revokeBtn = screen.getByTitle("Revoke Folder Authorization");
    fireEvent.click(revokeBtn);
    await waitFor(() => {
      expect(screen.getByText(/Failed to remove folder:/i)).toBeInTheDocument();
    });
  });

  /* --------------------------------------------------------------------------
     5. App: Session deletion edge cases & sidebar modal launchers
     -------------------------------------------------------------------------- */
  it("exercises App session deletion error handling and sidebar launchers", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_conversations") {
        return [
          { id: "c1", title: "Conversation 1", model: "nemotron" },
          { id: "c2", title: "Conversation 2", model: "nemotron" },
        ];
      }
      if (cmd === "get_messages") return [];
      if (cmd === "list_bots") {
        return [
          {
            id: "apex",
            name: "Apex Orchestrator",
            avatar: "bot",
            role: "Autonomous orchestrator",
            system_prompt: "You are Kyro Apex",
            is_default: true,
          },
        ];
      }
      if (cmd === "get_roster_statuses") return [];
      if (cmd === "delete_conversation") {
        throw new Error("Database lock contention");
      }
      return {};
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Conversation 1")).toBeInTheDocument();
    });

    // Delete session failure notice
    const deleteBtn = screen.getAllByTitle("Delete session")[0];
    fireEvent.click(deleteBtn);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    const dialog = screen.getByRole("dialog");
    const confirmDeleteBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmDeleteBtn);

    await waitFor(() => {
      expect(screen.getByText("Database lock contention")).toBeInTheDocument();
    });

    // Sidebar Team modal trigger
    const teamBtn = screen.getByRole("button", { name: "Change" });
    fireEvent.click(teamBtn);
    await waitFor(() => {
      expect(screen.getByText("Teammate Roster & Multi-Agent State")).toBeInTheDocument();
    });
    // Close team modal
    const closeBtns = screen.getAllByRole("button");
    const closeDialogBtn = closeBtns.find((b) => b.getAttribute("aria-label") === "Close" || b.getAttribute("title") === "Close");
    if (closeDialogBtn) fireEvent.click(closeDialogBtn);
  });
});
