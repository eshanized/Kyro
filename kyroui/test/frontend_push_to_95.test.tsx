import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("Frontend Final Push to 95% Coverage", () => {
  const mockedInvoke = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
    window.confirm = vi.fn(() => true);
  });

  /* -------------------------------------------------------------
     1. ConnectionsModal Local MCP Server & Reauthorization
     ------------------------------------------------------------- */
  describe("ConnectionsModal Custom Local MCP & Reauth", () => {
    const mockMcpServers = [
      {
        id: "mcp-local-1",
        name: "Postgres Local Server",
        command: "npx",
        args_json: '["-y", "@modelcontextprotocol/server-postgres"]',
        env_json: '{"DATABASE_URL": {"value": "postgresql://...", "is_secret": true}}',
        enabled: true,
      },
    ];

    const mockAccountNeedsReauth = {
      id: "acc-reauth-1",
      connector_id: "github",
      display_name: "Work GitHub",
      account_identifier: "org-dev",
      is_healthy: false,
      status: "needs_reauth",
      auth_type: "oauth",
      connected_at: Date.now() - 86400000,
    };

    it("creates, configures env secrets, and deletes local MCP servers", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_connector_catalog") return [];
        if (cmd === "list_connected_accounts") return [mockAccountNeedsReauth];
        if (cmd === "list_mcp_servers") return mockMcpServers;
        if (cmd === "get_mcp_server_statuses") return { "mcp-local-1": "connected" };
        if (cmd === "create_mcp_server") return {};
        if (cmd === "delete_mcp_server") return {};
        return {};
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("Discover Catalog")).toBeInTheDocument();
      });

      // Switch to Local MCP Servers tab
      fireEvent.click(screen.getByText(/Local MCP Servers/i));
      await waitFor(() => {
        expect(screen.getByText("Postgres Local Server")).toBeInTheDocument();
      });

      // Open Add Server Form
      fireEvent.click(screen.getByText("Add Local MCP Server"));
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/e\.g\. Local Filesystem Server/i)).toBeInTheDocument();
      });

      // Fill Name and Command
      const nameInput = screen.getByPlaceholderText(/e\.g\. Local Filesystem Server/i);
      fireEvent.change(nameInput, { target: { value: "SQLite Tool Server" } });

      const cmdInput = screen.getByPlaceholderText(/npx or python3/i);
      fireEvent.change(cmdInput, { target: { value: "uvx" } });

      const argsInput = screen.getByPlaceholderText(/-y @modelcontextprotocol\/server-filesystem/i);
      fireEvent.change(argsInput, { target: { value: "mcp-server-sqlite --db /tmp/test.db" } });

      // Add Environment Secret
      const envKeyInput = screen.getByPlaceholderText(/KEY \(e\.g\. DATABASE_URL/i);
      fireEvent.change(envKeyInput, { target: { value: "DB_TOKEN" } });

      const envValInput = screen.getByPlaceholderText(/VALUE \(stored in Vault\)/i);
      fireEvent.change(envValInput, { target: { value: "secret_val_123" } });

      fireEvent.click(screen.getByText("Add Env"));

      // Click Save MCP Server
      fireEvent.click(screen.getByText("Save & Register Local Server"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("create_mcp_server", expect.objectContaining({
          server: expect.objectContaining({
            name: "SQLite Tool Server",
            command: "uvx",
          }),
        }));
      });

      // Delete existing local MCP server
      const deleteBtn = screen.getByTitle("Delete server");
      fireEvent.click(deleteBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("delete_mcp_server", { id: "mcp-local-1" });
      });

      // Switch to Connected Accounts tab and test Reauthorize button
      fireEvent.click(screen.getByText(/Connected Accounts/i));
      await waitFor(() => {
        expect(screen.getByText("NEEDS REAUTH")).toBeInTheDocument();
      });

      const reauthBtn = screen.getByTitle("Re-authenticate this account");
      fireEvent.click(reauthBtn);
    });
  });

  /* -------------------------------------------------------------
     2. MyRoutinesView Toggling, Editing, and Deleting
     ------------------------------------------------------------- */
  describe("MyRoutinesView Full Lifecycle", () => {
    const mockRoutines = [
      {
        id: "rt-1",
        name: "Morning Email Triage",
        cron_expression: "0 9 * * 1-5",
        action_type: "prompt",
        target_id: "Summarize top 5 emails and flag urgent actions",
        enabled: true,
        last_run: Date.now() - 3600000,
        next_run: Date.now() + 3600000,
      },
    ];

    it("toggles enabled state, edits schedule, and deletes routines", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_routines") return mockRoutines;
        if (cmd === "list_routine_history") return [];
        if (cmd === "get_automation_metrics") {
          return {
            total_runs: 10,
            successful_runs: 10,
            failed_runs: 0,
            success_rate: 100,
            total_hours_saved: 5,
            active_routines_count: 1,
          };
        }
        if (cmd === "update_routine") return {};
        if (cmd === "delete_routine") return {};
        return {};
      });

      render(<MyRoutinesView />);

      await waitFor(() => {
        expect(screen.getByText("Morning Email Triage")).toBeInTheDocument();
      });

      // Toggle Pause/Resume
      const pauseBtn = screen.getByTitle("Pause routine");
      fireEvent.click(pauseBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("update_routine", {
          routine: expect.objectContaining({
            id: "rt-1",
            enabled: false,
          }),
        });
      });

      // Open Edit Routine
      const editBtn = screen.getByTitle("Edit routine");
      fireEvent.click(editBtn);

      await waitFor(() => {
        expect(screen.getByDisplayValue("Morning Email Triage")).toBeInTheDocument();
      });

      // Modify routine name and target
      const nameInput = screen.getByDisplayValue("Morning Email Triage");
      fireEvent.change(nameInput, { target: { value: "Morning Email & Slack Triage" } });

      const saveEditBtn = screen.getByText("Save Changes");
      fireEvent.click(saveEditBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("update_routine", {
          routine: expect.objectContaining({
            id: "rt-1",
            name: "Morning Email & Slack Triage",
          }),
        });
      });

      // Delete routine
      const deleteBtn = screen.getByTitle("Delete routine");
      fireEvent.click(deleteBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("delete_routine", { id: "rt-1" });
      });
    });
  });

  /* -------------------------------------------------------------
     3. TeachTaskModal Recording and Routine Creation
     ------------------------------------------------------------- */
  describe("TeachTaskModal Recording and Scheduler", () => {
    const mockSkills = [
      {
        id: "skill-1",
        name: "Sample Skill",
        description: "Test skill",
        steps_json: "[]",
        created_at: 1000,
      },
    ];

    it("starts and stops recording and creates scheduled routine from skill", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_skills") return mockSkills;
        if (cmd === "list_routines") return [];
        if (cmd === "start_teach_task") return {};
        if (cmd === "stop_teach_task") {
          return {
            id: "skill-recorded-1",
            name: "Download PDF Statement",
            description: "Clicks account, downloads PDF",
            steps_json: "[]",
            created_at: Date.now(),
          };
        }
        if (cmd === "create_routine") return {};
        return {};
      });

      render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="record" />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/e\.g\. Export Daily Analytics/i)).toBeInTheDocument();
      });

      // Fill in skill recording name and description
      const nameInput = screen.getByPlaceholderText(/e\.g\. Export Daily Analytics/i);
      fireEvent.change(nameInput, { target: { value: "Download PDF Statement" } });

      const descInput = screen.getByPlaceholderText(/Opens browser, navigates to dashboard/i);
      fireEvent.change(descInput, { target: { value: "Clicks account, downloads PDF" } });

      // Start recording
      fireEvent.click(screen.getByText("Start Recording Workflow"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("start_teach_task", {
          name: "Download PDF Statement",
          description: "Clicks account, downloads PDF",
        });
        expect(screen.getByText("RECORDING ACTIVE")).toBeInTheDocument();
      });

      // Stop recording
      fireEvent.click(screen.getByText("Save & Finish Skill"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("stop_teach_task");
      });

      // Switch to Routine Schedules tab
      fireEvent.click(screen.getByText(/Routine Schedules/i));
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Routine Name \(e\.g\. Morning Scrape\)/i)).toBeInTheDocument();
      });

      const routineNameInput = screen.getByPlaceholderText(/Routine Name \(e\.g\. Morning Scrape\)/i);
      fireEvent.change(routineNameInput, { target: { value: "Nightly PDF Backup" } });

      // Target selection
      const targetSelects = screen.getAllByRole("combobox");
      fireEvent.change(targetSelects[targetSelects.length - 1], { target: { value: "skill-1" } });

      fireEvent.click(screen.getByText("Schedule Routine"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("create_routine", expect.objectContaining({
          routine: expect.objectContaining({
            name: "Nightly PDF Backup",
            action_type: "skill",
          }),
        }));
      });
    });
  });

  /* -------------------------------------------------------------
     4. DevWorkbenchModal Viewports & Dev Server Launcher
     ------------------------------------------------------------- */
  describe("DevWorkbenchModal Viewports & Servers", () => {
    const mockCandidates = [
      {
        path: "/home/user/project",
        name: "kyro-web",
        kind: "Node.js",
        package_manager: "npm",
        is_git_repo: true,
        scripts: ["dev", "build"],
      },
    ];

    it("switches audit viewports and launches dev servers", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_dev_servers") return [];
        if (cmd === "discover_projects") {
          return {
            searched_root: "/home/snigdha/Desktop/Kyro",
            candidates: mockCandidates,
          };
        }
        if (cmd === "start_dev_server") {
          return {
            id: "srv-1",
            project_path: "/home/user/project",
            port: 5173,
            url: "http://localhost:5173",
            status: "running",
            logs: [],
          };
        }
        return {};
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} />);

      // Switch to UI Audit & Baselines tab
      fireEvent.click(screen.getByText("UI Audit & Baselines"));

      await waitFor(() => {
        expect(screen.getByText("1280")).toBeInTheDocument();
      });

      // Switch viewports
      fireEvent.click(screen.getByText("768"));
      fireEvent.click(screen.getByText("375"));
      fireEvent.click(screen.getByText("1280"));

      // Switch to Projects & Dev Servers tab
      fireEvent.click(screen.getByText(/Projects & Dev Servers/i));
      await waitFor(() => {
        expect(screen.getByText("Discover Projects")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Discover Projects"));
      await waitFor(() => {
        expect(screen.getByText("kyro-web")).toBeInTheDocument();
      });

      // Launch candidate dev server
      fireEvent.click(screen.getByText("Start Dev Server"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("start_dev_server", { projectPath: "/home/user/project" });
      });
    });
  });

  /* -------------------------------------------------------------
     5. ArtifactsModal Compare & Comment Enter Key
     ------------------------------------------------------------- */
  describe("ArtifactsModal Compare Diffs", () => {
    const mockArtifacts = [
      {
        id: "art-1",
        name: "Baseline.png",
        file_path: "/tmp/baseline.png",
        artifact_type: "image",
        mime_type: "image/png",
        size_bytes: 1024,
        created_at: 1000,
        metadata_json: JSON.stringify({ comments: [] }),
      },
      {
        id: "art-2",
        name: "Current.png",
        file_path: "/tmp/current.png",
        artifact_type: "image",
        mime_type: "image/png",
        size_bytes: 2048,
        created_at: 2000,
        metadata_json: JSON.stringify({ comments: [] }),
      },
    ];

    it("compares two artifacts with side-by-side diff and adds comments on Enter key", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_artifacts") return mockArtifacts;
        if (cmd === "read_artifact_content") {
          return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        }
        if (cmd === "add_artifact_comment") return true;
        return {};
      });

      render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getAllByText("Baseline.png")[0]).toBeInTheDocument();
      });

      // Compare dropdown appears because 2 image artifacts exist
      await waitFor(() => {
        expect(screen.getByText("Compare:")).toBeInTheDocument();
      });

      const compareSelect = screen.getByRole("combobox");
      fireEvent.change(compareSelect, { target: { value: "art-2" } });

      // Add comment using Enter key
      const commentInput = screen.getByPlaceholderText(/Add comment or feedback/i);
      fireEvent.change(commentInput, { target: { value: "Verified visual diff" } });
      fireEvent.keyDown(commentInput, { key: "Enter" });

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("add_artifact_comment", {
          id: "art-1",
          comment: "Verified visual diff",
          feedToAgent: true,
        });
      });
    });
  });
});
