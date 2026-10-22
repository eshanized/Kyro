import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { MyToolsView } from "../components/MyToolsView";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { TodayDashboardView } from "../components/TodayDashboardView";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { CommandPalette } from "../components/CommandPalette";
import { TeammateRosterModal } from "../components/TeammateRosterModal";
import { DocumentAssistantModal } from "../components/DocumentAssistantModal";
import { ScreenExplainerModal } from "../components/ScreenExplainerModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("DevWorkbenchModal Comprehensive Integration Tests", () => {
  const mockDiscovery = {
    searched_root: "/home/snigdha/Desktop/Kyro",
    candidates: [
      {
        name: "kyro-web",
        path: "/workspace/web-app",
        kind: "frontend",
        is_git_repo: true,
        scripts: ["dev", "build", "test"],
        name_matched: true,
        package_manager: "npm",
      },
    ],
  };

  const mockDevServers = [
    {
      id: "srv_web_1",
      pid: 1234,
      working_directory: "/workspace/web-app",
      command: "npm run dev",
      status: "ready",
      ready_url: "http://localhost:3000",
      port: 3000,
      framework: "Next.js",
      package_manager: "npm",
      started_at: 1000,
    },
  ];

  const mockCheckpoints = [
    {
      checkpoint_id: "cp_101",
      workspace_path: "/home/snigdha/Desktop/Kyro",
      description: "Pre-refactor checkpoint",
      created_at: 1000,
      pre_existing_dirty_files: [],
      modified_files: [
        {
          file_path: "src/main.rs",
          new_content: "fn main() {}",
          additions: 1,
          deletions: 0,
          unified_diff: "@@ -0,0 +1 @@\n+fn main() {}",
          risk_assessment: "low",
        },
      ],
      status: "active",
    },
  ];

  const mockDiagnostic = {
    root_cause_error: "cannot find value in scope",
    file: "src/main.rs",
    line_number: 42,
    error_code: "E0425",
    category: "type_error",
    suggested_repair: "use crate::models::User;",
    reproduced: true,
  };

  const mockBaseline = {
    project_path: "/home/snigdha/Desktop/Kyro",
    route: "/",
    viewport: "desktop",
    base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    created_at: 1000,
  };

  const mockWorkspace = {
    workspace_id: "ws_multi_1",
    parent_run_id: "run_1",
    project_path: "/home/snigdha/Desktop/Kyro",
    tasks: [
      {
        task_id: "t_1",
        role: "implementation_engineer",
        objective: "Add endpoints",
        dependencies: [],
        status: "completed",
      },
    ],
    status: "active",
  };

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_dev_servers") return Promise.resolve(mockDevServers);
      if (cmd === "discover_projects") return Promise.resolve(mockDiscovery);
      if (cmd === "start_dev_server") return Promise.resolve(mockDevServers[0]);
      if (cmd === "stop_dev_server") return Promise.resolve(true);
      if (cmd === "get_dev_server_logs") return Promise.resolve(["Ready in 250ms", "Compiled /dashboard"]);
      if (cmd === "create_code_checkpoint") return Promise.resolve(mockCheckpoints[0]);
      if (cmd === "rollback_code_checkpoint") {
        return Promise.resolve({
          checkpoint_id: "cp_101",
          files_reverted: ["src/main.rs"],
          conflicts_detected: [],
          status: "success",
        });
      }
      if (cmd === "diagnose_build_failure") return Promise.resolve(mockDiagnostic);
      if (cmd === "get_audit_baseline") return Promise.resolve(mockBaseline);
      if (cmd === "set_audit_baseline") return Promise.resolve(mockBaseline);
      if (cmd === "compare_visual_baseline") {
        return Promise.resolve({
          has_regression: false,
          diff_percent: 0.1,
          route: "/",
          viewport: "desktop",
          description: "No visual defects found",
        });
      }
      if (cmd === "create_multi_agent_workspace") return Promise.resolve(mockWorkspace);
      if (cmd === "reconcile_multi_agent_workspace") {
        return Promise.resolve({
          workspace_id: "ws_multi_1",
          changes_detected: true,
          modified_files: ["src/lib.rs"],
          unified_diff: "@@ -1 +1 @@",
          conflicts_detected: [],
          can_safely_merge: true,
          reconciliation_summary: "Merged 1 clean commit",
        });
      }
      return Promise.resolve(null);
    });
  });

  it("navigates Projects & Dev Servers tab and controls server lifecycle", async () => {
    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="projects" />);

    expect(screen.getByText("Kyro Developer Workbench")).toBeInTheDocument();

    const discoverBtn = screen.getByRole("button", { name: /Discover Projects/i });
    fireEvent.click(discoverBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "discover_projects",
        expect.objectContaining({
          searchRoot: "/home/snigdha/Desktop/Kyro",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText("kyro-web")).toBeInTheDocument();
    });

    const logsBtn = screen.getByRole("button", { name: /Logs/i });
    fireEvent.click(logsBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_dev_server_logs", expect.objectContaining({ serverId: "srv_web_1" }));
    });

    const stopBtn = screen.getByRole("button", { name: /Stop/i });
    fireEvent.click(stopBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("stop_dev_server", { serverId: "srv_web_1" });
    });
  });

  it("navigates Code Checkpoints tab and creates/rollbacks snapshots", async () => {
    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="checkpoints" />);

    const createBtn = screen.getByRole("button", { name: /Save Checkpoint/i });
    fireEvent.click(createBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_code_checkpoint",
        expect.objectContaining({
          workspacePath: "/home/snigdha/Desktop/Kyro",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText("Pre-refactor checkpoint")).toBeInTheDocument();
    });

    const rollbackBtn = screen.getByRole("button", { name: /1-Click Rollback/i });
    fireEvent.click(rollbackBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "rollback_code_checkpoint",
        expect.objectContaining({
          checkpoint: expect.objectContaining({
            checkpoint_id: "cp_101",
          }),
        })
      );
    });
  });

  it("navigates Build Doctor tab and parses compiler diagnostics", async () => {
    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="doctor" />);

    const diagnoseBtn = screen.getByRole("button", { name: /Run Build Doctor/i });
    fireEvent.click(diagnoseBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "diagnose_build_failure",
        expect.objectContaining({
          workingDir: "/home/snigdha/Desktop/Kyro",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText(/cannot find value in scope/i)).toBeInTheDocument();
      expect(screen.getByText(/use crate::models::User;/i)).toBeInTheDocument();
    });
  });

  it("navigates Visual Baselines tab and compares screenshot regressions", async () => {
    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="audit" />);

    const inspectBtn = screen.getByRole("button", { name: /Inspect Stored Baseline/i });
    fireEvent.click(inspectBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "get_audit_baseline",
        expect.objectContaining({
          route: "/",
        })
      );
    });

    const compareBtn = screen.getByRole("button", { name: /Compare Against Baseline/i });
    fireEvent.click(compareBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "compare_visual_baseline",
        expect.objectContaining({
          route: "/",
        })
      );
    });
  });

  it("navigates Multi-Agent Workspace tab and initializes worktrees", async () => {
    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="workspace" />);

    const initBtn = screen.getByRole("button", { name: /Initialize Specialist Workspace/i });
    fireEvent.click(initBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_multi_agent_workspace",
        expect.objectContaining({
          projectPath: "/home/snigdha/Desktop/Kyro",
        })
      );
    });

    const reconcileBtn = screen.getByRole("button", { name: /Reconcile Worktree Changes/i });
    fireEvent.click(reconcileBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "reconcile_multi_agent_workspace",
        expect.objectContaining({
          workspaceId: "ws_multi_1",
        })
      );
    });
  });
});

describe("TeachTaskModal Comprehensive Integration Tests", () => {
  const mockSkills = [
    {
      id: "skill_1",
      name: "Organize Invoices Flow",
      description: "Moves PDF invoices into Fiscal Year folders",
      steps: [
        { action_type: "click", target: "invoices_folder" },
        { action_type: "move", target: "fiscal_2026" },
      ],
      created_at: 1000,
    },
  ];

  const mockRoutines = [
    {
      id: "rt_1",
      name: "Weekly Invoice Sorter",
      cron_expr: "0 9 * * 5",
      target_type: "skill",
      target_id: "skill_1",
      enabled: true,
      requires_confirmation: false,
      last_run_at: 1000,
      next_run_at: 2000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_skills") return Promise.resolve(mockSkills);
      if (cmd === "list_routines") return Promise.resolve(mockRoutines);
      if (cmd === "replay_skill") return Promise.resolve(true);
      if (cmd === "delete_skill") return Promise.resolve(true);
      if (cmd === "start_teach_task") return Promise.resolve(true);
      if (cmd === "stop_teach_task") return Promise.resolve(mockSkills[0]);
      if (cmd === "create_routine") return Promise.resolve(mockRoutines[0]);
      if (cmd === "delete_routine") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("lists skills, searches, and replays recorded workflow", async () => {
    render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="skills" />);

    await waitFor(() => {
      expect(screen.getByText("Organize Invoices Flow")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Organize Invoices Flow"));

    const replayBtns = screen.getAllByRole("button", { name: /^Replay$/i });
    fireEvent.click(replayBtns[0]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("replay_skill", { skillId: "skill_1" });
    });
  });

  it("records new desktop automation skill", async () => {
    render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="record" />);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Export Daily Analytics Spreadsheet/i);
    fireEvent.change(nameInput, { target: { value: "Automate Payroll Export" } });

    const startBtn = screen.getByRole("button", { name: /Start Recording Workflow/i });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "start_teach_task",
        expect.objectContaining({
          name: "Automate Payroll Export",
        })
      );
    });

    const stopBtn = screen.getByRole("button", { name: /Save & Finish Skill/i });
    fireEvent.click(stopBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("stop_teach_task");
    });
  });

  it("schedules routine bound to recorded skill", async () => {
    render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="routines" />);

    await waitFor(() => {
      expect(screen.getByText("Weekly Invoice Sorter")).toBeInTheDocument();
    });

    const nameInput = screen.getByPlaceholderText(/Routine Name \(e\.g\. Morning Scrape\)/i);
    fireEvent.change(nameInput, { target: { value: "Daily System Health Check" } });

    const createBtn = screen.getByRole("button", { name: /Schedule Routine/i });
    fireEvent.click(createBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_routine",
        expect.objectContaining({
          routine: expect.objectContaining({
            name: "Daily System Health Check",
          }),
        })
      );
    });
  });
});

describe("MyToolsView Comprehensive Integration Tests", () => {
  const mockAuthorizedFolders = [
    {
      id: "auth_1",
      display_name: "Client Documents",
      path: "/home/snigdha/Documents/Clients",
      authorized_at: 1000,
    },
  ];

  const mockSearchResults = [
    {
      path: "/home/snigdha/Documents/Clients/invoice_2026.pdf",
      file_name: "invoice_2026.pdf",
      format: "pdf",
      preview_text: "Invoice #1042 for Software Consulting Services",
      extracted_date: "2026-03-15",
    },
  ];

  const mockDuplicates = [
    {
      evidence: "Exact hash match",
      file_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      size_bytes: 1048576,
      files: [
        "/home/snigdha/Documents/Clients/invoice_2026.pdf",
        "/home/snigdha/Documents/Clients/copy_invoice_2026.pdf",
      ],
    },
  ];

  const mockStudyNotes = {
    doc_name: "consensus.pdf",
    overview: "Comprehensive guide to distributed consensus algorithms.",
    key_dates_and_milestones: ["2026-11-01 Proposal deadline"],
    revision_flashcards: [
      {
        question: "What is Raft leader election timeout?",
        answer: "Randomized between 150ms and 300ms",
      },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_authorized_folders") return Promise.resolve(mockAuthorizedFolders);
      if (cmd === "add_authorized_folder") return Promise.resolve(mockAuthorizedFolders[0]);
      if (cmd === "remove_authorized_folder") return Promise.resolve(true);
      if (cmd === "search_authorized_documents") return Promise.resolve(mockSearchResults);
      if (cmd === "detect_folder_duplicates") return Promise.resolve(mockDuplicates);
      if (cmd === "generate_study_notes") return Promise.resolve(mockStudyNotes);
      if (cmd === "explain_document_passage") return Promise.resolve("This passage specifies the leader election protocol.");
      return Promise.resolve(null);
    });
  });

  it("searches documents and scans duplicate files in Files tab", async () => {
    const handleAskPrompt = vi.fn();
    const handleOpenOrganizer = vi.fn();

    render(
      <MyToolsView
        onAskPrompt={handleAskPrompt}
        onOpenDocAssistant={vi.fn()}
        onOpenScreenExplainer={vi.fn()}
        onOpenConnections={vi.fn()}
        onOpenFileOrganizer={handleOpenOrganizer}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Client Documents")).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/Search keywords, filenames, or content/i);
    fireEvent.change(searchInput, { target: { value: "consulting invoice" } });

    const searchBtn = screen.getByRole("button", { name: /^Search$/i });
    fireEvent.click(searchBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "search_authorized_documents",
        expect.objectContaining({
          query: "consulting invoice",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText("invoice_2026.pdf")).toBeInTheDocument();
    });

    const scanDupBtn = screen.getByRole("button", { name: /Detect Duplicate Files/i });
    fireEvent.click(scanDupBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("detect_folder_duplicates", {
        folderPath: "/home/snigdha/Documents/Clients",
      });
    });

    const organizerCard = screen.getByText("Safe File Organizer");
    fireEvent.click(organizerCard);
    expect(handleOpenOrganizer).toHaveBeenCalledTimes(1);
  });

  it("switches to Study tab and generates revision notes", async () => {
    render(
      <MyToolsView
        onAskPrompt={vi.fn()}
        onOpenDocAssistant={vi.fn()}
        onOpenScreenExplainer={vi.fn()}
        onOpenConnections={vi.fn()}
        onOpenOrganizer={vi.fn()}
      />
    );

    const studyTabBtn = screen.getByRole("button", { name: /Study & Revision Notes/i });
    fireEvent.click(studyTabBtn);

    const docPathInput = screen.getByPlaceholderText(/Enter path to document or PDF file/i);
    fireEvent.change(docPathInput, { target: { value: "/data/docs/consensus.pdf" } });

    const notesBtn = screen.getByRole("button", { name: /Generate Notes/i });
    fireEvent.click(notesBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "generate_study_notes",
        expect.objectContaining({
          path: "/data/docs/consensus.pdf",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText("What is Raft leader election timeout?")).toBeInTheDocument();
    });
  });

  it("switches to Web & Connected tabs and triggers research queries", async () => {
    const handleAskPrompt = vi.fn();

    render(
      <MyToolsView
        onAskPrompt={handleAskPrompt}
        onOpenDocAssistant={vi.fn()}
        onOpenScreenExplainer={vi.fn()}
        onOpenConnections={vi.fn()}
        onOpenOrganizer={vi.fn()}
      />
    );

    const webTabBtn = screen.getByRole("button", { name: /Web Research & Compare/i });
    fireEvent.click(webTabBtn);

    const researchInput = screen.getByPlaceholderText(/What topic or question would you like to research/i);
    fireEvent.change(researchInput, { target: { value: "Rust 2024 Edition Features" } });

    const researchBtn = screen.getByRole("button", { name: /^Research$/i });
    fireEvent.click(researchBtn);

    expect(handleAskPrompt).toHaveBeenCalledWith(
      expect.stringContaining("Rust 2024 Edition Features")
    );
  });
});

describe("MyRoutinesView & TodayDashboard Integration Tests", () => {
  const mockRoutines = [
    {
      id: "rt_daily_backup",
      name: "Daily Backup S3",
      cron_expr: "0 2 * * *",
      target_type: "script",
      target_id: "backup_task",
      enabled: true,
      requires_confirmation: false,
      last_run_at: 1000,
      next_run_at: 2000,
    },
  ];

  const mockRuns = [
    {
      id: "run_rt_1",
      routine_id: "rt_daily_backup",
      started_at: 1000,
      completed_at: 1050,
      status: "completed",
      output: "Uploaded 142 files to s3://backup-vault",
    },
  ];

  const mockTodayData = {
    greeting: "Good morning, Engineer",
    today_date: "Thursday, October 22, 2026",
    pending_approvals: [
      {
        id: "appr_101",
        tool_name: "delete_directory",
        summary: "Delete /workspace/temp_build",
        reason: "Clean up temporary build artifacts",
        created_at: 1000,
      },
    ],
    upcoming_routines: mockRoutines,
    recent_runs: mockRuns,
    quick_stats: {
      completed_tasks_today: 8,
      active_connections: 5,
      saved_hours_estimate: 2.5,
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_routines") return Promise.resolve(mockRoutines);
      if (cmd === "list_routine_runs") return Promise.resolve(mockRuns);
      if (cmd === "execute_routine_now") return Promise.resolve(mockRuns[0]);
      if (cmd === "update_routine") return Promise.resolve(mockRoutines[0]);
      if (cmd === "delete_routine") return Promise.resolve(true);
      if (cmd === "get_today_dashboard_data") return Promise.resolve(mockTodayData);
      if (cmd === "resolve_approval") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("renders MyRoutinesView and executes a routine immediately", async () => {
    render(<MyRoutinesView onOpenTeachTask={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Backup S3")).toBeInTheDocument();
    });

    const runNowBtn = screen.getByRole("button", { name: /Run Now/i });
    fireEvent.click(runNowBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("execute_routine_now", {
        routineId: "rt_daily_backup",
      });
    });
  });

  it("renders TodayDashboardView and resolves pending security approval", async () => {
    const handleNavigate = vi.fn();

    render(
      <TodayDashboardView
        onNavigate={handleNavigate}
        onOpenTeachTask={vi.fn()}
        onOpenConnections={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Good morning, Engineer/i)).toBeInTheDocument();
      expect(screen.getByText("delete_directory")).toBeInTheDocument();
    });

    const approveBtn = screen.getByRole("button", { name: /Approve/i });
    fireEvent.click(approveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "resolve_approval",
        expect.objectContaining({
          requestId: "appr_101",
          approved: true,
        })
      );
    });
  });
});

describe("ArtifactsModal, CommandPalette, and Teammates Tests", () => {
  const mockArtifacts = [
    {
      id: "art_101",
      conversation_id: "conv_1",
      run_id: "run_1",
      name: "System Architecture Specification",
      artifact_type: "markdown",
      file_path: "/data/artifacts/spec.md",
      mime_type: "text/markdown",
      size_bytes: 2048,
      metadata_json: JSON.stringify({ comments: [], viewport: "1920x1080" }),
      created_at: 1000,
    },
  ];

  const mockBots = [
    {
      id: "bot_forge",
      name: "Forge",
      avatar: "wrench",
      role: "Engineering Implementation Specialist",
      system_prompt: "Write production Rust and TypeScript code.",
      is_default: false,
    },
  ];

  const mockStatuses = [
    {
      id: "bot_forge",
      state: "idle",
      current_task: null,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_artifacts") return Promise.resolve(mockArtifacts);
      if (cmd === "read_artifact_content") return Promise.resolve("# Architecture Specification\nKyro micro-kernel design.");
      if (cmd === "export_artifact") return Promise.resolve("/tmp/spec_exported.md");
      if (cmd === "delete_artifact") return Promise.resolve(true);
      if (cmd === "add_artifact_comment") return Promise.resolve(true);
      if (cmd === "list_bots") return Promise.resolve(mockBots);
      if (cmd === "get_roster_statuses") return Promise.resolve(mockStatuses);
      if (cmd === "create_bot") return Promise.resolve(mockBots[0]);
      if (cmd === "delete_bot") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("renders ArtifactsModal, previews markdown, and exports file", async () => {
    render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText("System Architecture Specification")[0]).toBeInTheDocument();
    });

    fireEvent.click(screen.getAllByText("System Architecture Specification")[0]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("read_artifact_content", { id: "art_101" });
    });

    const exportInput = screen.getByPlaceholderText(/Enter destination file path to export/i);
    fireEvent.change(exportInput, { target: { value: "/tmp/spec_exported.md" } });

    const exportBtn = screen.getByRole("button", { name: /Export/i });
    fireEvent.click(exportBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "export_artifact",
        expect.objectContaining({
          id: "art_101",
          destinationPath: "/tmp/spec_exported.md",
        })
      );
    });
  });

  it("filters and activates actions in CommandPalette", () => {
    const handleNewSession = vi.fn();
    const handleToggleGhost = vi.fn();

    render(
      <CommandPalette
        isOpen={true}
        onClose={vi.fn()}
        onNewSession={handleNewSession}
        onToggleGhost={handleToggleGhost}
        onToggleTerminal={vi.fn()}
        onOpenTeammates={vi.fn()}
        onOpenTeachTask={vi.fn()}
        onOpenMcp={vi.fn()}
        onOpenTimeline={vi.fn()}
        onOpenArtifacts={vi.fn()}
        onOpenSettings={vi.fn()}
        onSelectConversation={vi.fn()}
        onSelectBot={vi.fn()}
      />
    );

    const newSessionOption = screen.getByText("New Autonomous Session");
    fireEvent.click(newSessionOption);

    expect(handleNewSession).toHaveBeenCalledTimes(1);
  });

  it("renders TeammateRosterModal and selects specialist teammate", async () => {
    const handleSelectBot = vi.fn();

    render(
      <TeammateRosterModal
        isOpen={true}
        onClose={vi.fn()}
        selectedBotId="apex"
        onSelectBot={handleSelectBot}
      />
    );

    await waitFor(() => {
      expect(screen.getAllByText("@Forge").length).toBeGreaterThan(0);
    });

    const botBadges = screen.getAllByText("@Forge");
    const botCard = botBadges[botBadges.length - 1].closest("div");
    if (botCard) {
      fireEvent.click(botCard);
      expect(handleSelectBot).toHaveBeenCalledWith(
        expect.objectContaining({
          id: "bot_forge",
          name: "Forge",
        })
      );
    }
  });
});

describe("DocumentAssistantModal and ScreenExplainerModal Integration Tests", () => {
  const mockAnalysis = {
    metadata: {
      title: "Service Level Agreement 2026",
      page_count: 5,
      word_count: 2400,
      detected_language: "English",
      file_size_bytes: 45000,
    },
    summary: "Standard 99.99% uptime enterprise agreement.",
    key_facts: ["Uptime target: 99.99%", "Support response time: 15 minutes"],
    action_items: [
      {
        text: "Review penalty clauses",
        priority: "high",
      },
    ],
    extracted_dates: [{ raw_date: "2026-11-01", context: "Renewal deadline" }],
    missing_or_ambiguous: ["Data retention beyond 30 days is unspecified"],
    sections: [],
    privacy_disclosure: "Document contains confidential customer SLA terms.",
    raw_text: "Service Level Agreement...",
  };

  const mockScreenAnalysis = {
    summary: "Visual inspection detected an uncaught TypeError in DevTools console.",
    key_regions: [
      {
        label: "Error Notification",
        x: 100,
        y: 200,
        width: 300,
        height: 80,
      },
    ],
    visible_defects: [
      {
        defect_type: "console_error",
        description: "Uncaught TypeError: Cannot read property 'map' of undefined",
      },
    ],
    discrepancies: [],
    confidence: "high",
    supporting_evidence: "Call stack points to App.tsx line 45",
    uncertainty: "none",
    vision_supported: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "analyze_document") return Promise.resolve(mockAnalysis);
      if (cmd === "compare_documents") {
        return Promise.resolve({
          doc_a_path: "/a.pdf",
          doc_b_path: "/b.pdf",
          summary_differences: "Clause 4 increased uptime penalty to 5%.",
          common_topics: ["Uptime", "Disaster Recovery"],
          added_in_b: ["Added security audit requirement"],
          removed_from_a: [],
          action_items_comparison: [],
          markdown_report: "# SLA Comparison Report",
        });
      }
      if (cmd === "take_screenshot") {
        return Promise.resolve({
          base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        });
      }
      if (cmd === "explain_screen_capture") return Promise.resolve(mockScreenAnalysis);
      if (cmd === "run_agent_turn") return Promise.resolve("run_123");
      return Promise.resolve(null);
    });
  });

  it("analyzes and compares documents in DocumentAssistantModal", async () => {
    render(<DocumentAssistantModal isOpen={true} onClose={vi.fn()} />);

    const pathInput = screen.getByPlaceholderText(/Enter path to text document or PDF file/i);
    fireEvent.change(pathInput, { target: { value: "/data/legal/sla.pdf" } });

    const analyzeBtn = screen.getByRole("button", { name: /^Analyze$/i });
    fireEvent.click(analyzeBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "analyze_document",
        expect.objectContaining({
          path: "/data/legal/sla.pdf",
        })
      );
    });

    await waitFor(() => {
      expect(screen.getByText("Standard 99.99% uptime enterprise agreement.")).toBeInTheDocument();
      expect(screen.getByText("Review penalty clauses")).toBeInTheDocument();
    });

    // Switch to compare tab
    const compareTab = screen.getByRole("button", { name: /Compare/i });
    fireEvent.click(compareTab);

    const docA = screen.getByPlaceholderText(/Document A path \(original\)/i);
    const docB = screen.getByPlaceholderText(/Document B path \(revised\)/i);
    fireEvent.change(docA, { target: { value: "/data/legal/sla_v1.pdf" } });
    fireEvent.change(docB, { target: { value: "/data/legal/sla_v2.pdf" } });

    const runCompareBtn = screen.getByRole("button", { name: /Generate Comparison Report/i });
    fireEvent.click(runCompareBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("compare_documents", {
        pathA: "/data/legal/sla_v1.pdf",
        pathB: "/data/legal/sla_v2.pdf",
      });
    });

    await waitFor(() => {
      expect(screen.getByText(/Clause 4 increased uptime penalty to 5%/i)).toBeInTheDocument();
    });
  });

  it("takes screenshot and explains error in ScreenExplainerModal", async () => {
    render(
      <ScreenExplainerModal
        isOpen={true}
        onClose={vi.fn()}
        initialScreenshotBase64="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
      />
    );

    const captureBtn = screen.getByRole("button", { name: /Capture Fresh Display/i });
    fireEvent.click(captureBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("take_screenshot", { monitorIndex: 0 });
    });

    const explainBtn = screen.getByRole("button", { name: /Explain Screen/i });
    fireEvent.click(explainBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "explain_screen_capture",
        expect.objectContaining({
          question: "Explain what is happening on this screen and what I should do next.",
        })
      );
    });

    await waitFor(() => {
      expect(
        screen.getByText("Visual inspection detected an uncaught TypeError in DevTools console.")
      ).toBeInTheDocument();
    });
  });
});
