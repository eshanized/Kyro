import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { invoke } from "@tauri-apps/api/core";

// Mock Tauri modules
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({ label: "main" })),
}));

// Components under test
import { TodayDashboardView } from "../components/TodayDashboardView";
import { CommandPalette } from "../components/CommandPalette";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { MyToolsView } from "../components/MyToolsView";
import { ScreenExplainerModal } from "../components/ScreenExplainerModal";
import { OnboardingModal } from "../components/OnboardingModal";
import { App } from "../App";

const mockedInvoke = vi.mocked(invoke);

describe("Phase C Frontend Coverage to >= 95%", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  /* -------------------------------------------------------------
     1. TodayDashboardView Comprehensive Branches
     ------------------------------------------------------------- */
  describe("TodayDashboardView Comprehensive", () => {
    const mockDashboardData = {
      greeting: "Good morning",
      date_display: "Wednesday, October 22, 2026",
      upcoming_events: [
        {
          id: "ev1",
          title: "Sprint Planning",
          time_display: "10:00 AM",
          location: "Room 101",
          html_link: "https://calendar.google.com/event1",
        },
      ],
      email_summary: {
        unread_count: 3,
        recent_subjects: ["Weekly Sync", "Project Update"],
        is_connected: true,
        account_email: "user@example.com",
      },
      recent_documents: [
        {
          file_name: "invoice_september.pdf",
          path: "/home/user/docs/invoice_september.pdf",
          modified_ago: "2 hours ago",
        },
      ],
      upcoming_routines: [
        {
          id: "rt1",
          name: "Clean Downloads",
          schedule: "Every Friday at 9am",
          next_run_display: "Friday at 9:00 AM",
        },
      ],
      recent_routine_results: [
        {
          id: "res1",
          routine_id: "rt1",
          routine_name: "Clean Downloads",
          status: "succeeded",
          started_at: Date.now() - 3600000,
          completed_at: Date.now() - 3590000,
          summary: "Moved 12 files",
        },
      ],
      pending_tasks_count: 1,
      pending_approvals: [
        {
          id: "appr-1",
          tool_name: "delete_file",
          risk_level: "destructive",
          summary: "Delete duplicate file",
          reason: "Free up disk space",
        },
      ],
      services: [],
    };

    it("renders dashboard, executes routine now with success and failure status", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_today_dashboard_data") return mockDashboardData;
        if (cmd === "execute_routine_now") {
          if (args.routineId === "rt1") {
            return {
              id: "run-success",
              routine_id: "rt1",
              status: "succeeded",
              started_at: Date.now(),
            };
          }
        }
        return {};
      });

      const handleOpenTool = vi.fn();
      const handleAskPrompt = vi.fn();
      const handleOpenConnections = vi.fn();
      const handleOpenSettings = vi.fn();

      render(
        <TodayDashboardView
          onOpenTool={handleOpenTool}
          onAskPrompt={handleAskPrompt}
          onOpenConnections={handleOpenConnections}
          onOpenSettings={handleOpenSettings}
        />
      );

      await waitFor(() => {
        expect(screen.getByText(/Good morning!/i)).toBeInTheDocument();
      });

      // Execute routine
      const runBtn = screen.getByText("Run");
      fireEvent.click(runBtn);

      await waitFor(() => {
        expect(screen.getByText(/Routine 'Clean Downloads' executed successfully/i)).toBeInTheDocument();
      });

      // Dismiss action notice
      const dismissBtn = screen.getByText("Dismiss");
      fireEvent.click(dismissBtn);
      expect(screen.queryByText(/Routine 'Clean Downloads' executed successfully/i)).not.toBeInTheDocument();

      // Quick prompt buttons
      fireEvent.click(screen.getByText("Find Downloaded Invoices"));
      expect(handleAskPrompt).toHaveBeenCalledWith("Find the invoice I downloaded last month");

      fireEvent.click(screen.getByText("Organize Downloaded Files"));
      expect(handleAskPrompt).toHaveBeenCalledWith("Organize my college documents into appropriate folders");

      fireEvent.click(screen.getByText("Study Notes & Summary"));
      expect(handleAskPrompt).toHaveBeenCalledWith("Explain this PDF in simple language and create revision notes");

      // Card prompts
      fireEvent.click(screen.getByText(/Analyze Schedule/i));
      expect(handleAskPrompt).toHaveBeenCalledWith(expect.stringContaining("calendar tomorrow"));

      fireEvent.click(screen.getByText(/Summarize Unread & Draft Replies/i));
      expect(handleAskPrompt).toHaveBeenCalledWith(expect.stringContaining("Summarize my unread emails"));

      fireEvent.click(screen.getByText("Summarize"));
      expect(handleAskPrompt).toHaveBeenCalledWith(expect.stringContaining("Summarize this document: /home/user/docs/invoice_september.pdf"));

      // Tool and Settings navigations
      fireEvent.click(screen.getByText("Browse All"));
      expect(handleOpenTool).toHaveBeenCalledWith("files");

      fireEvent.click(screen.getByText("Privacy & Settings"));
      expect(handleOpenSettings).toHaveBeenCalledWith("privacy");

      fireEvent.click(screen.getByText("Manage"));
      expect(handleOpenTool).toHaveBeenCalledWith("routines");

      fireEvent.click(screen.getByRole("button", { name: /Run History/i }));
      expect(handleOpenTool).toHaveBeenCalledWith("routines", "history");

      // Approve pending authorization
      const approveBtn = screen.getByText("Approve");
      fireEvent.click(approveBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("resolve_approval", {
          id: "appr-1",
          decision: "approved",
          resolvedBy: "today_dashboard",
        });
      });
    });

    it("handles error states, routine failure, approval rejection, and retry buttons", async () => {
      const errorDashboardData = {
        ...mockDashboardData,
        calendar_error: "Google Calendar token expired",
        email_error: "Gmail connection refused",
        upcoming_events: [],
        email_summary: { is_connected: false, unread_count: 0, recent_subjects: [] },
        recent_documents: [],
        upcoming_routines: [],
        pending_approvals: [
          {
            id: "appr-2",
            tool_name: "restart_service",
            risk_level: "moderate",
            summary: "Restart docker container",
            reason: "Apply changes",
          },
        ],
      };

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_today_dashboard_data") return errorDashboardData;
        if (cmd === "execute_routine_now") {
          return {
            id: "run-failed",
            routine_id: "rt1",
            status: "failed",
            error: "Process crashed",
          };
        }
        if (cmd === "resolve_approval") {
          return {};
        }
        return {};
      });

      render(
        <TodayDashboardView
          onOpenTool={vi.fn()}
          onAskPrompt={vi.fn()}
          onOpenConnections={vi.fn()}
          onOpenSettings={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText(/Google Calendar token expired/i)).toBeInTheDocument();
        expect(screen.getByText(/Gmail connection refused/i)).toBeInTheDocument();
      });

      // Retry calendar and inbox buttons
      fireEvent.click(screen.getByText("Retry Calendar"));
      fireEvent.click(screen.getByText("Retry Inbox"));

      // Refresh button
      fireEvent.click(screen.getByTitle("Refresh dashboard data"));

      // Reject approval
      const rejectBtn = screen.getByText("Reject");
      fireEvent.click(rejectBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("resolve_approval", {
          id: "appr-2",
          decision: "denied",
          resolvedBy: "today_dashboard",
        });
      });
    });
  });

  /* -------------------------------------------------------------
     2. CommandPalette Comprehensive Navigation & Execution
     ------------------------------------------------------------- */
  describe("CommandPalette Comprehensive", () => {
    it("navigates actions with keyboard, search workspace, and triggers all callbacks", async () => {
      const onClose = vi.fn();
      const onNewSession = vi.fn();
      const onToggleGhost = vi.fn();
      const onToggleTerminal = vi.fn();
      const onOpenTeammates = vi.fn();
      const onOpenTeachTask = vi.fn();
      const onOpenMcp = vi.fn();
      const onOpenConnections = vi.fn();
      const onOpenTimeline = vi.fn();
      const onOpenArtifacts = vi.fn();
      const onOpenSettings = vi.fn();
      const onSelectConversation = vi.fn();
      const onSelectBot = vi.fn();

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "search_workspace") {
          return [
            { category: "conversation", id: "conv-101", title: "Refactor API", snippet: "Rust backend" },
            { category: "message", id: "msg_202", title: "Message in Session 1", snippet: "Found token" },
            { category: "artifact", id: "art-303", title: "architecture.md", snippet: "System diagram" },
            { category: "skill", id: "skill-404", title: "Backup DB", snippet: "Postgres dump" },
            { category: "routine", id: "rt-505", title: "Daily Sync", snippet: "Every morning" },
            { category: "memory", id: "mem-606", title: "User preference", snippet: "Dark mode" },
            { category: "unknown", id: "unk-707", title: "Other resource", snippet: "Generic note" },
          ];
        }
        if (cmd === "list_agent_profiles") {
          return [
            {
              id: "apex-orchestrator",
              name: "Apex",
              avatar: "zap",
              role: "General Workspace & Strategic Orchestrator",
              description: "",
              model_id: null,
              origin: "builtin",
              enabled: true,
              runtime_disabled: false,
              schema_version: 1,
              version_label: "v1:aaa",
              aliases: [],
              validation_error: null,
              source_path: null,
              instructions: "",
              tools_mode: "inherit",
              tools_allow: [],
              delegation_enabled: true,
              delegation_max_depth: 3,
              delegation_max_children: 4,
              max_steps: 25,
              is_builtin: true,
            },
            {
              id: "forge-devops",
              name: "Forge",
              avatar: "wrench",
              role: "DevOps, Code & Terminal Specialist",
              description: "",
              model_id: null,
              origin: "builtin",
              enabled: true,
              runtime_disabled: false,
              schema_version: 1,
              version_label: "v1:bbb",
              aliases: [],
              validation_error: null,
              source_path: null,
              instructions: "",
              tools_mode: "inherit",
              tools_allow: [],
              delegation_enabled: true,
              delegation_max_depth: 3,
              delegation_max_children: 4,
              max_steps: 25,
              is_builtin: true,
            },
            {
              id: "scout-web",
              name: "Scout",
              avatar: "globe",
              role: "Browser Automation & Web Researcher",
              description: "",
              model_id: null,
              origin: "builtin",
              enabled: true,
              runtime_disabled: false,
              schema_version: 1,
              version_label: "v1:ccc",
              aliases: [],
              validation_error: null,
              source_path: null,
              instructions: "",
              tools_mode: "inherit",
              tools_allow: [],
              delegation_enabled: true,
              delegation_max_depth: 3,
              delegation_max_children: 4,
              max_steps: 25,
              is_builtin: true,
            },
          ];
        }
        return [];
      });

      render(
        <CommandPalette
          isOpen={true}
          onClose={onClose}
          onNewSession={onNewSession}
          onToggleGhost={onToggleGhost}
          onToggleTerminal={onToggleTerminal}
          onOpenTeammates={onOpenTeammates}
          onOpenTeachTask={onOpenTeachTask}
          onOpenMcp={onOpenMcp}
          onOpenConnections={onOpenConnections}
          onOpenTimeline={onOpenTimeline}
          onOpenArtifacts={onOpenArtifacts}
          onOpenSettings={onOpenSettings}
          onSelectConversation={onSelectConversation}
          onSelectBot={onSelectBot}
        />
      );

      // Keyboard arrow navigation on default actions
      const input = screen.getByPlaceholderText(/Type a command or search/i);
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "ArrowUp" });
      fireEvent.keyDown(input, { key: "Enter" });
      expect(onNewSession).toHaveBeenCalled();

      // Test each default action by clicking
      fireEvent.click(screen.getByText("Toggle Ghost Mode"));
      expect(onToggleGhost).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Toggle PTY Host Terminal"));
      expect(onToggleTerminal).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Open Artifacts Registry"));
      expect(onOpenArtifacts).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Teach-a-Task & Routines"));
      expect(onOpenTeachTask).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Teammate Roster"));
      expect(onOpenTeammates).toHaveBeenCalled();

      fireEvent.click(screen.getByText("App Connections & Integrations"));
      expect(onOpenConnections).toHaveBeenCalled();

      fireEvent.click(screen.getByText("MCP Tool Servers"));
      expect(onOpenMcp).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Step Timeline & Visual Diffs"));
      expect(onOpenTimeline).toHaveBeenCalled();

      fireEvent.click(screen.getByText("System Health & Diagnostics"));
      expect(onOpenSettings).toHaveBeenCalledWith("diagnostics");

      fireEvent.click(screen.getByText("Settings & Vault"));
      expect(onOpenSettings).toHaveBeenCalledWith("vault");

      // Teammate switch actions load from the registry asynchronously.
      await waitFor(() => {
        expect(screen.getByText("Switch to @Apex")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText("Switch to @Apex"));
      expect(onSelectBot).toHaveBeenCalledWith(expect.objectContaining({ name: "Apex" }));

      fireEvent.click(screen.getByText("Switch to @Forge"));
      expect(onSelectBot).toHaveBeenCalledWith(expect.objectContaining({ name: "Forge" }));

      fireEvent.click(screen.getByText("Switch to @Scout"));
      expect(onSelectBot).toHaveBeenCalledWith(expect.objectContaining({ name: "Scout" }));

      // Workspace search
      fireEvent.change(input, { target: { value: "test query" } });
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("search_workspace", { query: "test query" });
        expect(screen.getByText("Refactor API")).toBeInTheDocument();
      });

      // Arrow navigation on search results
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "Enter" }); // selects msg_202

      // Click each search result category
      fireEvent.click(screen.getByText("Refactor API"));
      expect(onSelectConversation).toHaveBeenCalledWith("conv-101");

      fireEvent.click(screen.getByText("architecture.md"));
      expect(onOpenArtifacts).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Backup DB"));
      expect(onOpenTeachTask).toHaveBeenCalledWith("skills", "skill-404");

      fireEvent.click(screen.getByText("Daily Sync"));
      expect(onOpenTeachTask).toHaveBeenCalledWith("routines", "rt-505");

      fireEvent.click(screen.getByText("User preference"));
      expect(onOpenSettings).toHaveBeenCalledWith("memory");

      // Global keyboard shortcuts (Ctrl+K and Escape)
      fireEvent.keyDown(window, { key: "Escape" });
      expect(onClose).toHaveBeenCalled();

      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      expect(onClose).toHaveBeenCalled();
    });
  });

  /* -------------------------------------------------------------
     3. MyRoutinesView Cron & Routine Management
     ------------------------------------------------------------- */
  describe("MyRoutinesView Cron & Automation Management", () => {
    it("covers cron descriptions, natural language routine suggestions, saving, and editing", async () => {
      const mockRoutines = [
        {
          id: "r1",
          name: "Daily Sync",
          cron_expression: "0 9 * * *",
          action_type: "script",
          target_id: "sync.sh",
          enabled: true,
        },
        {
          id: "r2",
          name: "Weekly Backup",
          cron_expression: "0 9 * * 5",
          action_type: "teach_task",
          target_id: "task-1",
          enabled: false,
        },
      ];

      const mockRuns = [
        {
          id: "run1",
          routine_id: "r1",
          routine_name: "Daily Sync",
          status: "succeeded",
          started_at: Date.now() - 3600000,
          completed_at: Date.now() - 3590000,
          summary: "All systems synced",
        },
      ];

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_routines") return mockRoutines;
        if (cmd === "list_routine_runs") return mockRuns;
        if (cmd === "natural_language_to_routine") {
          return {
            name: "Summarize Downloads",
            cron_expression: "0 9 * * 1-5",
            action_type: "prompt",
            target_id: "Summarize downloads",
            explanation: "Runs weekdays at 9am",
            required_resources: ["Downloads folder access"],
          };
        }
        if (cmd === "create_routine") return { id: "new-routine" };
        if (cmd === "execute_routine_now") {
          return {
            id: "run-exec",
            routine_id: args.routineId,
            status: "succeeded",
            started_at: Date.now(),
          };
        }
        if (cmd === "update_routine") return {};
        if (cmd === "delete_routine") return {};
        return {};
      });

      render(<MyRoutinesView onAskPrompt={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("Daily Sync")).toBeInTheDocument();
        expect(screen.getByText("Weekly Backup")).toBeInTheDocument();
      });

      // Suggest a routine via natural language
      const nlInput = screen.getByPlaceholderText(/Every Friday at 9am/i);
      fireEvent.change(nlInput, { target: { value: "Summarize downloads every weekday" } });
      fireEvent.keyDown(nlInput, { key: "Enter" });

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("natural_language_to_routine", {
          prompt: "Summarize downloads every weekday",
        });
        expect(screen.getByDisplayValue("Summarize Downloads")).toBeInTheDocument();
      });

      // Save suggested routine
      const saveBtn = screen.getByText("Confirm & Schedule Routine");
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("create_routine", expect.anything());
      });

      // Run routine immediately
      const runBtns = await screen.findAllByText("Run Now");
      fireEvent.click(runBtns[0]);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("execute_routine_now", { routineId: "r1" });
      });

      // Switch to history tab
      fireEvent.click(screen.getByText(/Run History/i));
      await waitFor(() => {
        expect(screen.getByText("All systems synced")).toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     4. MyToolsView Personal Documents, Duplicates, and Study Notes
     ------------------------------------------------------------- */
  describe("MyToolsView Comprehensive", () => {
    it("handles folder management, search, duplicate scanning, and study flashcards", async () => {
      const mockFolders = [{ id: "f1", path: "/home/user/Documents", display_name: "Documents" }];
      const mockDocs = [
        {
          file_name: "lecture_1.pdf",
          path: "/home/user/Documents/lecture_1.pdf",
          format: "pdf",
          snippet: "Introduction to Operating Systems",
        },
      ];
      const mockDuplicates = [
        {
          file_hash: "abcdef1234567890abcdef",
          evidence: "Exact duplicate content (4.2 MB)",
          files: [
            { path: "/home/user/Documents/file_a.png", is_original: true },
            { path: "/home/user/Documents/copy_a.png", is_original: false },
          ],
        },
      ];
      const mockStudyNotes = {
        doc_name: "lecture_1.pdf",
        overview: "Foundations of concurrent processes and semaphores.",
        key_dates_and_milestones: ["Midterm exam: Nov 15", "Lab 2 due: Oct 30"],
        revision_flashcards: [
          {
            question: "What is a race condition?",
            answer: "A situation where concurrent execution order affects program output.",
          },
        ],
      };

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_authorized_folders") return mockFolders;
        if (cmd === "add_authorized_folder") return { id: "f2", path: args.path, display_name: args.displayName };
        if (cmd === "remove_authorized_folder") return {};
        if (cmd === "search_authorized_documents") return mockDocs;
        if (cmd === "detect_folder_duplicates") return mockDuplicates;
        if (cmd === "generate_study_notes") return mockStudyNotes;
        if (cmd === "explain_document_passage") return "Simplified explanation of the complex passage.";
        return {};
      });

      const handleAskPrompt = vi.fn();
      const handleOpenConnections = vi.fn();
      const handleOpenScreenExplainer = vi.fn();
      const handleOpenFileOrganizer = vi.fn();
      const handleOpenDocAssistant = vi.fn();

      render(
        <MyToolsView
          onAskPrompt={handleAskPrompt}
          onOpenConnections={handleOpenConnections}
          onOpenScreenExplainer={handleOpenScreenExplainer}
          onOpenFileOrganizer={handleOpenFileOrganizer}
          onOpenDocAssistant={handleOpenDocAssistant}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Personal Files & Docs")).toBeInTheDocument();
      });

      // Add authorized folder
      const folderPathInput = screen.getByPlaceholderText(/Folder path/i);
      const folderNameInput = screen.getByPlaceholderText(/Optional Label/i);
      fireEvent.change(folderPathInput, { target: { value: "/home/user/Downloads" } });
      fireEvent.change(folderNameInput, { target: { value: "Downloads" } });
      fireEvent.click(screen.getByText("Authorize"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("add_authorized_folder", {
          path: "/home/user/Downloads",
          displayName: "Downloads",
        });
      });

      // Search documents with filter
      const searchInput = screen.getByPlaceholderText(/Search keywords, filenames, or content/i);
      fireEvent.change(searchInput, { target: { value: "lecture" } });
      fireEvent.click(screen.getByRole("button", { name: /^Search$/i }));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("search_authorized_documents", expect.anything());
        expect(screen.getByText("lecture_1.pdf")).toBeInTheDocument();
      });

      // Detect duplicates
      fireEvent.click(screen.getByText("Detect Duplicate Files"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("detect_folder_duplicates", {
          folderPath: "/home/user/Documents",
        });
        expect(screen.getByText(/Exact duplicate content/i)).toBeInTheDocument();
      });

      // Switch to Study tab
      fireEvent.click(screen.getByText("Study & Revision Notes"));
      const docPathInput = screen.getByPlaceholderText(/Enter path to document or PDF file/i);
      fireEvent.change(docPathInput, { target: { value: "/home/user/Documents/lecture_1.pdf" } });
      fireEvent.click(screen.getByText("Generate Notes"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("generate_study_notes", {
          path: "/home/user/Documents/lecture_1.pdf",
          topic: null,
        });
        expect(screen.getByText(/Foundations of concurrent processes/i)).toBeInTheDocument();
        expect(screen.getByText("What is a race condition?")).toBeInTheDocument();
      });

      // Reveal flashcard answer
      fireEvent.click(screen.getByText("Reveal Answer"));
      expect(screen.getByText(/A situation where concurrent execution order/i)).toBeInTheDocument();

      // Explain passage
      const passageInput = screen.getByPlaceholderText(/Paste the confusing sentence or paragraph here/i);
      fireEvent.change(passageInput, { target: { value: "Mutually exclusive semaphores prevent non-deterministic updates." } });
      fireEvent.click(screen.getByText("Explain Simply"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("explain_document_passage", expect.anything());
        expect(screen.getByText("Simplified explanation of the complex passage.")).toBeInTheDocument();
      });

      // Switch to Screen tab
      fireEvent.click(screen.getByText("Screen & Error Help"));
      fireEvent.click(screen.getByText("Launch Screen Explainer"));
      expect(handleOpenScreenExplainer).toHaveBeenCalled();

      // Switch to Web Research tab
      fireEvent.click(screen.getByText("Web Research & Compare"));
      const webQueryInput = screen.getByPlaceholderText(/What topic or question would you like to research/i);
      fireEvent.change(webQueryInput, { target: { value: "NASA Artemis mission" } });
      fireEvent.click(screen.getByText("Research"));
      expect(handleAskPrompt).toHaveBeenCalledWith(expect.stringContaining("NASA Artemis mission"));

      const compareItemsInput = screen.getByPlaceholderText(/Products to compare/i);
      fireEvent.change(compareItemsInput, { target: { value: "Sony WH-1000XM5 vs Bose QC Ultra" } });
      fireEvent.click(screen.getByText("Build Comparison Table"));
      expect(handleAskPrompt).toHaveBeenCalledWith(expect.stringContaining("Sony WH-1000XM5 vs Bose QC Ultra"));

      // Switch to Connected Services tab
      fireEvent.click(screen.getByText("Connected Services"));
      const manageBtns = screen.getAllByText(/Configure \/ Connect/i);
      fireEvent.click(manageBtns[0]);
      expect(handleOpenConnections).toHaveBeenCalled();
    });
  });

  /* -------------------------------------------------------------
     5. ScreenExplainerModal Interactive Visual Analysis
     ------------------------------------------------------------- */
  describe("ScreenExplainerModal Visual Analysis", () => {
    it("captures screen, analyzes display, and runs recommended action", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "take_screenshot") {
          return { base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" };
        }
        if (cmd === "explain_screen_capture") {
          return {
            summary: "Error dialog: Disk is 98% full",
            supporting_evidence: "Local storage warning on volume /dev/nvme0n1p2",
            visible_defects: [{ defect_type: "Storage Warning", description: "Empty Trash or organize files" }],
          };
        }
        if (cmd === "run_agent_turn") {
          return "Turn started";
        }
        return {};
      });

      const onClose = vi.fn();

      render(
        <ScreenExplainerModal
          isOpen={true}
          onClose={onClose}
        />
      );

      // Capture screenshot
      const captureBtn = screen.getByText("Capture Fresh Display");
      fireEvent.click(captureBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("take_screenshot", { monitorIndex: 0 });
      });

      // Analyze screenshot
      const analyzeBtn = screen.getByText("Explain Screen");
      fireEvent.click(analyzeBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("explain_screen_capture", expect.anything());
        expect(screen.getByText(/Disk is 98% full/i)).toBeInTheDocument();
        expect(screen.getByText(/Storage Warning:/i)).toBeInTheDocument();
      });

      // Execute recommended step
      const executeBtn = screen.getByText("Execute Grounded Action");
      fireEvent.click(executeBtn);

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("run_agent_turn", expect.anything());
        expect(screen.getByText(/Action initiated through Kyro's governed execution engine/i)).toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     6. OnboardingModal Step Progression & Configuration
     ------------------------------------------------------------- */
  describe("OnboardingModal Multi-Step Flow", () => {
    it("progresses through all 6 steps, toggles folders, and saves configuration", async () => {
      const mockSuggestedFolders = [
        { id: "downloads", name: "Downloads", path: "/home/user/Downloads", exists: true },
        { id: "documents", name: "Documents", path: "/home/user/Documents", exists: true },
      ];

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_suggested_folders") return mockSuggestedFolders;
        if (cmd === "save_api_keys") return {};
        if (cmd === "add_authorized_folder") return {};
        return {};
      });

      const onClose = vi.fn();
      const onTryExample = vi.fn();
      const onOpenConnections = vi.fn();

      render(
        <OnboardingModal
          isOpen={true}
          onClose={onClose}
          onTryExample={onTryExample}
          onOpenConnections={onOpenConnections}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Your Everyday Personal AI Assistant")).toBeInTheDocument();
      });

      // Step 1 -> Step 2
      fireEvent.click(screen.getByText("Continue"));

      // Step 2: Enter API key and submit
      const keyInput = screen.getByPlaceholderText(/nvapi-.../i);
      fireEvent.change(keyInput, { target: { value: "nvapi-testkey-12345" } });
      fireEvent.click(screen.getByText("Continue"));

      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("save_api_keys", {
          keys: { nvidia: "nvapi-testkey-12345" },
        });
      });

      // Step 3: Authorize folders
      await waitFor(() => {
        expect(screen.getByText("Choose Authorized Folders")).toBeInTheDocument();
      });

      // Add custom folder
      const customPathInput = screen.getByPlaceholderText(/Absolute directory path/i);
      fireEvent.change(customPathInput, { target: { value: "/home/user/Projects" } });
      fireEvent.click(screen.getByText("Add"));

      fireEvent.click(screen.getByText("Continue"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("add_authorized_folder", expect.anything());
      });

      // Step 4: Connected services
      await waitFor(() => {
        expect(screen.getByText("Optional Connected Services")).toBeInTheDocument();
      });
      const connectBtns = screen.getAllByText("Connect");
      fireEvent.click(connectBtns[0]);
      expect(onOpenConnections).toHaveBeenCalled();

      // Step 5: Privacy
      fireEvent.click(screen.getByText("Continue"));
      await waitFor(() => {
        expect(screen.getByText("Privacy & Safety Principles")).toBeInTheDocument();
      });

      // Step 6: Ready & Examples
      fireEvent.click(screen.getByText("Continue"));
      await waitFor(() => {
        expect(screen.getByText("Ready to Get Started!")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Find an invoice or document"));
      expect(onTryExample).toHaveBeenCalledWith(expect.stringContaining("Find the invoice"));

      fireEvent.click(screen.getByText("Get Started"));
      expect(onClose).toHaveBeenCalled();
    });
  });

  /* -------------------------------------------------------------
     7. App.tsx Plan Execution, Attachments, and Mode Toggles
     ------------------------------------------------------------- */
  describe("App.tsx Comprehensive Integration", () => {
    it("handles consequential task plan generation, dismissal, and file attachments", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") return [{ id: "c1", title: "Test Conv", model: "nemotron" }];
        if (cmd === "get_messages") return [];
        if (cmd === "list_routines") return [];
        if (cmd === "list_routine_runs") return [];
        if (cmd === "list_authorized_folders") return [];
        if (cmd === "list_teammate_bots") {
          return [
            {
              id: "apex-orchestrator",
              name: "Apex",
              role: "Strategic Orchestrator",
              avatar: "zap",
              system_prompt: "",
              is_default: true,
            },
          ];
        }
        if (cmd === "get_today_dashboard_data") {
          return {
            greeting: "Good afternoon",
            date_display: "Wednesday, October 22",
            upcoming_events: [],
            email_summary: { unread_count: 0, is_connected: false },
            recent_documents: [],
            upcoming_routines: [],
            recent_routine_results: [],
            services: [],
          };
        }
        if (cmd === "run_agent_turn") {
          return "I have organized your downloads folder into neat categories.";
        }
        if (cmd === "add_message") return {};
        if (cmd === "create_conversation") return { id: "c-new", title: "Organize files", model: "nemotron" };
        return {};
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("KYRO")).toBeInTheDocument();
      });

      // Enter consequential task prompt (triggers ActivePlan)
      const input = screen.getByPlaceholderText(/Ask @Apex to automate/i);
      fireEvent.change(input, { target: { value: "Please organize my downloaded files into category folders" } });

      const sendBtn = screen.getByText("Send");
      fireEvent.click(sendBtn);

      await waitFor(() => {
        // Bug 5 contract: the client template renders as a Suggested
        // Workflow, never an authoritative "Execution Plan".
        expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
        expect(screen.getByText(/Suggested workflow for: "Please organize my downloaded files/i)).toBeInTheDocument();
      });

      // Dismiss execution plan
      const dismissPlanBtn = screen.getByTitle("Dismiss plan");
      fireEvent.click(dismissPlanBtn);
      expect(screen.queryByText("Suggested Workflow")).not.toBeInTheDocument();

      // Everyday destination tabs navigation
      fireEvent.click(screen.getByText("Today", { selector: "span" }).closest("button")!);
      await waitFor(() => {
        expect(screen.getByText(/Good afternoon!/i)).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("My Tools", { selector: "span" }).closest("button")!);
      await waitFor(() => {
        expect(screen.getByText("Personal Files & Docs")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("My Routines", { selector: "span" }).closest("button")!);
      await waitFor(() => {
        expect(screen.getByText(/Automate recurring computer tasks without coding/i)).toBeInTheDocument();
      });
    });
  });
});
