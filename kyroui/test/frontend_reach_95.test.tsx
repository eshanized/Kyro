import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { MyToolsView } from "../components/MyToolsView";
import { TodayDashboardView } from "../components/TodayDashboardView";
import { FileOrganizerModal } from "../components/FileOrganizerModal";
import { OnboardingModal } from "../components/OnboardingModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: vi.fn(),
  isPermissionGranted: vi.fn(() => Promise.resolve(true)),
  requestPermission: vi.fn(() => Promise.resolve("granted")),
}));

// Mock SpeechSynthesisUtterance for JSDOM
(global as any).SpeechSynthesisUtterance = class {
  text: string;
  onend: any;
  onerror: any;
  constructor(text: string) {
    this.text = text;
  }
};

describe("Frontend Reach 95% Coverage Test Suite", () => {
  const mockedInvoke = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
    window.confirm = vi.fn(() => true);
    localStorage.clear();
    localStorage.setItem("kyro_onboarding_completed", "true");
  });

  /* -------------------------------------------------------------
     1. App.tsx: Session Deletion Lifecycle & Keyboard Escape
     ------------------------------------------------------------- */
  describe("App.tsx Session Deletion Dialog", () => {
    const mockConversations = [
      {
        id: "conv-del-1",
        title: "Session To Delete",
        model: "nvidia/llama-3.1-nemotron-70b-instruct",
        created_at: Date.now(),
      },
    ];

    it("cancels session deletion via Cancel button and Escape key", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") return mockConversations;
        if (cmd === "get_messages") return [];
        if (cmd === "list_personas") return [];
        if (cmd === "get_takeover_status") return { active: false };
        if (cmd === "list_pending_approvals") return [];
        return [];
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("Session To Delete")).toBeInTheDocument();
      });

      // Find the session delete trigger button
      const deleteTrigger = screen.getByTitle("Delete session");
      fireEvent.click(deleteTrigger);

      await waitFor(() => {
        expect(screen.getByText(/Are you sure you want to delete/i)).toBeInTheDocument();
      });

      // Cancel with cancel button
      const cancelBtn = screen.getByTestId("cancel-delete-session-button");
      fireEvent.click(cancelBtn);

      await waitFor(() => {
        expect(screen.queryByText(/Are you sure you want to delete/i)).not.toBeInTheDocument();
      });

      // Open dialog again and cancel via Escape key
      fireEvent.click(screen.getByTitle("Delete session"));
      await waitFor(() => {
        expect(screen.getByText(/Are you sure you want to delete/i)).toBeInTheDocument();
      });

      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() => {
        expect(screen.queryByText(/Are you sure you want to delete/i)).not.toBeInTheDocument();
      });
    });

    it("handles deletion errors and displays error message", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") return mockConversations;
        if (cmd === "get_messages") return [];
        if (cmd === "list_personas") return [];
        if (cmd === "get_takeover_status") return { active: false };
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "delete_conversation") {
          throw new Error("Database lock conflict");
        }
        return [];
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("Session To Delete")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTitle("Delete session"));
      await waitFor(() => {
        expect(screen.getByTestId("confirm-delete-session-button")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("confirm-delete-session-button"));

      await waitFor(() => {
        expect(screen.getByText(/Database lock conflict/i)).toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     2. App.tsx: Message Speech, Copy, and Turn Abort
     ------------------------------------------------------------- */
  describe("App.tsx Message Speech, Copy, and Turn Abort", () => {
    const mockMessages = [
      {
        id: "msg-assistant-1",
        conversation_id: "conv-main",
        role: "assistant",
        content: "Here is your system status report.",
        created_at: 1000,
      },
    ];

    it("copies assistant message to clipboard and speaks aloud with speech cancellation", async () => {
      const speakMock = vi.fn((utterance) => {});
      const cancelMock = vi.fn(() => {});

      (window as any).speechSynthesis = {
        speak: speakMock,
        cancel: cancelMock,
      };

      const writeTextMock = vi.fn(() => Promise.resolve());
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") {
          return [{ id: "conv-main", title: "Main Chat", model: "llama", created_at: 1000 }];
        }
        if (cmd === "get_messages") return mockMessages;
        if (cmd === "list_personas") return [];
        if (cmd === "get_takeover_status") return { active: false };
        if (cmd === "list_pending_approvals") return [];
        return [];
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("Here is your system status report.")).toBeInTheDocument();
      });

      // Test Copy button
      const copyBtn = screen.getByTitle("Copy response");
      fireEvent.click(copyBtn);
      expect(writeTextMock).toHaveBeenCalledWith("Here is your system status report.");

      // Test Speak button
      const speakBtn = screen.getByTitle("Read message aloud (Local TTS)");
      fireEvent.click(speakBtn);
      expect(speakMock).toHaveBeenCalled();

      // Test toggle stop speak
      fireEvent.click(screen.getByTitle("Read message aloud (Local TTS)"));
      expect(cancelMock).toHaveBeenCalled();
    });

    it("aborts active running turn and handles turn execution failure", async () => {
      let turnReject: ((reason?: any) => void) | null = null;

      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") {
          return [{ id: "conv-main", title: "Main Chat", model: "llama", created_at: 1000 }];
        }
        if (cmd === "get_messages") return [];
        if (cmd === "list_personas") return [];
        if (cmd === "get_takeover_status") return { active: false };
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "add_message") return {};
        if (cmd === "cancel_agent_run") return {};
        if (cmd === "run_agent_turn") {
          return new Promise((res, rej) => {
            turnReject = rej;
          });
        }
        return [];
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Ask @/i);
      fireEvent.change(input, { target: { value: "Perform heavy analysis" } });
      fireEvent.submit(input.closest("form")!);

      // Verify Stop button appears and can be clicked to abort
      await waitFor(() => {
        expect(screen.getByRole("button", { name: /Stop/i })).toBeInTheDocument();
      });

      const stopBtn = screen.getByRole("button", { name: /Stop/i });
      fireEvent.click(stopBtn);
      expect(mockedInvoke).toHaveBeenCalledWith("cancel_agent_run", expect.objectContaining({
        runId: expect.any(String),
      }));

      // Reject the turn to test error path
      if (turnReject) {
        (turnReject as Function)(new Error("Agent execution aborted by user"));
      }

      await waitFor(() => {
        expect(screen.getByText(/Agent execution aborted by user/i)).toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     3. App.tsx: Empty State Shortcuts and Floating HUD / Overlay Triggers
     ------------------------------------------------------------- */
  describe("App.tsx Dev Mode Shortcuts and Window Toggles", () => {
    it("interacts with Dev Mode suggestion cards and window triggers", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_conversations") return [];
        if (cmd === "get_messages") return [];
        if (cmd === "list_personas") return [];
        if (cmd === "get_takeover_status") return { active: false };
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "toggle_hud_window") return {};
        if (cmd === "toggle_overlay_window") return {};
        if (cmd === "list_dev_servers") return [];
        if (cmd === "discover_projects") return { candidates: [] };
        if (cmd === "list_checkpoints") return [];
        return [];
      });

      render(<App />);

      // Switch to Dev Mode
      const devModeBtn = screen.getByTitle(/Kyro Dev: Autonomous developer agent/i);
      fireEvent.click(devModeBtn);

      await waitFor(() => {
        expect(screen.getByText("Kyro Autonomous Workspace")).toBeInTheDocument();
      });

      // Click Dev suggestion cards
      fireEvent.click(screen.getByText("Dev Servers"));
      await waitFor(() => {
        expect(screen.getByText("Projects & Dev Servers")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByLabelText("Close"));

      // Click Code Checkpoints card
      fireEvent.click(screen.getByText("Code Checkpoints"));
      await waitFor(() => {
        expect(screen.getByText("Checkpoints & Diffs")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByLabelText("Close"));

      // Click Build Doctor card
      const doctorCards = screen.getAllByText("Build Doctor");
      fireEvent.click(doctorCards[0]);
      await waitFor(() => {
        expect(screen.getAllByText("Build Doctor").length).toBeGreaterThan(0);
      });
      fireEvent.click(screen.getByLabelText("Close"));

      // Click Visual Baselines card
      fireEvent.click(screen.getByText("Visual Baselines"));
      await waitFor(() => {
        expect(screen.getByText("UI Audit & Baselines")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByLabelText("Close"));

      // Click HUD Trigger
      const hudBtn = screen.getByTitle("Toggle Floating HUD Widget");
      fireEvent.click(hudBtn);
      expect(mockedInvoke).toHaveBeenCalledWith("toggle_hud_window");
    });
  });

  /* -------------------------------------------------------------
     4. ConnectionsModal: Token Connect Submit, Test, Disconnect, Policies
     ------------------------------------------------------------- */
  describe("ConnectionsModal Account Management & Policy Governance", () => {
    const mockCatalog = [
      {
        id: "github",
        name: "GitHub",
        description: "Repositories, issues, pull requests",
        connector_type: "native",
        auth_type: "pat_or_oauth",
        is_available: true,
        status: "available",
        capabilities: ["read", "write"],
      },
    ];

    const mockConnected = [
      {
        id: "acc-101",
        connector_id: "github",
        display_name: "Personal GitHub",
        account_identifier: "octocat",
        is_healthy: true,
        status: "active",
        auth_type: "token",
        connected_at: Date.now() - 3600000,
      },
    ];

    const mockTools = [
      {
        original_name: "create_pull_request",
        name: "create_pull_request",
        description: "Creates a pull request on GitHub",
        risk_level: "safe",
        is_read_only: false,
        can_auto_approve: true,
        default_policy: "ask",
      },
    ];

    const mockPolicies = [
      {
        account_id: "acc-101",
        tool_name: "create_pull_request",
        policy: "ask",
      },
    ];

    it("connects with personal token, tests connection, manages policies, and disconnects", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_connector_catalog") return mockCatalog;
        if (cmd === "list_connected_accounts") return mockConnected;
        if (cmd === "list_mcp_servers") return [];
        if (cmd === "get_mcp_server_statuses") return {};
        if (cmd === "connect_account_token") return {};
        if (cmd === "test_connector_connection") return "OK 200";
        if (cmd === "disconnect_account") return {};
        if (cmd === "list_account_tool_policies") return mockPolicies;
        if (cmd === "get_account_tools") return mockTools;
        if (cmd === "save_account_tool_policy") return {};
        return {};
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("GitHub")).toBeInTheDocument();
      });

      // Click Connect Account for GitHub
      fireEvent.click(screen.getByText("Connect Account"));

      await waitFor(() => {
        expect(screen.getByText("Personal Access Token")).toBeInTheDocument();
      });

      // Switch auth mode to Personal Access Token
      fireEvent.click(screen.getByText("Personal Access Token"));

      await waitFor(() => {
        expect(screen.getByPlaceholderText("ghp_...")).toBeInTheDocument();
      });

      // Fill in token connection form
      const tokenInput = screen.getByPlaceholderText("ghp_...");
      fireEvent.change(tokenInput, { target: { value: "ghp_mocktoken12345" } });

      fireEvent.click(screen.getByText("Save & Connect"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("connect_account_token", expect.objectContaining({
          connectorId: "github",
          tokenSecret: "ghp_mocktoken12345",
        }));
      });

      // Switch to Connected Accounts tab
      fireEvent.click(screen.getByText(/Connected Accounts/i));
      await waitFor(() => {
        expect(screen.getByText("Personal GitHub")).toBeInTheDocument();
      });

      // Test connection
      fireEvent.click(screen.getByTitle("Test live connection to API"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("test_connector_connection", { accountId: "acc-101" });
      });

      // Manage Policies
      fireEvent.click(screen.getByTitle("Configure action approval rules and risk policies"));
      await waitFor(() => {
        expect(screen.getByText(/Action Governance: Personal GitHub/i)).toBeInTheDocument();
        expect(screen.getByText("create_pull_request")).toBeInTheDocument();
      });

      // Change policy to Allow (auto-approve)
      const allowBtn = screen.getByText("Allow");
      fireEvent.click(allowBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("save_account_tool_policy", {
          accountId: "acc-101",
          toolName: "create_pull_request",
          policy: "allow",
          riskLevel: "safe",
        });
      });

      // Close Policy Dialog only (not main modal)
      const closePolicyBtn = screen.getByTitle("Close governance");
      fireEvent.click(closePolicyBtn);

      await waitFor(() => {
        expect(screen.queryByTitle("Close governance")).not.toBeInTheDocument();
      });

      // Disconnect Account
      await waitFor(() => {
        expect(screen.getByTitle("Safely remove account and purge vault credentials")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByTitle("Safely remove account and purge vault credentials"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("disconnect_account", { accountId: "acc-101" });
      });
    });
  });

  /* -------------------------------------------------------------
     5. MyToolsView: Authorized Folders Revocation, Study Notes, and Web Research
     ------------------------------------------------------------- */
  describe("MyToolsView Specialized Actions", () => {
    const mockFolders = [
      { id: "f-1", display_name: "Downloads", path: "/home/user/Downloads" },
    ];

    it("revokes authorized folders, generates study notes, and runs web research", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_authorized_folders") return mockFolders;
        if (cmd === "remove_authorized_folder") return {};
        if (cmd === "generate_study_notes") {
          return {
            doc_name: "syllabus.pdf",
            overview: "Key exam concepts",
            key_dates_and_milestones: ["2026-11-01"],
            revision_flashcards: [{ question: "Q1", answer: "A1" }],
          };
        }
        return {};
      });

      const onAskPromptMock = vi.fn();
      const onOpenScreenExplainerMock = vi.fn();
      const onOpenDocAssistantMock = vi.fn();

      render(
        <MyToolsView
          onAskPrompt={onAskPromptMock}
          onOpenScreenExplainer={onOpenScreenExplainerMock}
          onOpenDocAssistant={onOpenDocAssistantMock}
        />
      );

      // Category 1: Authorized folders
      await waitFor(() => {
        expect(screen.getByText("Downloads")).toBeInTheDocument();
      });

      // Revoke folder
      const revokeBtn = screen.getByTitle("Revoke folder authorization");
      fireEvent.click(revokeBtn);
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("remove_authorized_folder", { id: "f-1" });
      });

      // Category 2: Study Notes
      fireEvent.click(screen.getByText(/Study & Revision/i));
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter path to document or PDF file/i)).toBeInTheDocument();
      });

      const docInput = screen.getByPlaceholderText(/Enter path to document or PDF file/i);
      fireEvent.change(docInput, { target: { value: "/home/user/syllabus.pdf" } });

      const topicInput = screen.getByPlaceholderText(/Focus topic/i);
      fireEvent.change(topicInput, { target: { value: "Chapter 4" } });

      fireEvent.click(screen.getByText("Generate Notes"));
      await waitFor(() => {
        expect(mockedInvoke).toHaveBeenCalledWith("generate_study_notes", {
          path: "/home/user/syllabus.pdf",
          topic: "Chapter 4",
        });
        expect(screen.getByText("Key exam concepts")).toBeInTheDocument();
      });

      // Category 3: Screen Diagnostics
      fireEvent.click(screen.getByText(/Screen & Error Help/i));
      await waitFor(() => {
        expect(screen.getByText('"Help me fix this error"')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText('"Help me fix this error"'));
      expect(onAskPromptMock).toHaveBeenCalledWith("Explain the error on my screen and help me fix it");

      // Category 4: Web Research & Comparison
      fireEvent.click(screen.getByText(/Web Research & Compare/i));
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/What topic or question would you like to research/i)).toBeInTheDocument();
      });

      const webInput = screen.getByPlaceholderText(/What topic or question would you like to research/i);
      fireEvent.change(webInput, { target: { value: "Kyro Architecture" } });
      fireEvent.click(screen.getByText("Research"));
      expect(onAskPromptMock).toHaveBeenCalledWith(expect.stringContaining("Kyro Architecture"));
    });
  });

  /* -------------------------------------------------------------
     6. TodayDashboardView: Empty States & Navigation Callbacks
     ------------------------------------------------------------- */
  describe("TodayDashboardView Empty States and Nav", () => {
    it("handles empty folders and routines callbacks", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_authorized_folders") return [];
        if (cmd === "list_routines") return [];
        if (cmd === "get_automation_metrics") {
          return {
            total_runs: 0,
            successful_runs: 0,
            failed_runs: 0,
            success_rate: 0,
            total_hours_saved: 0,
            active_routines_count: 0,
          };
        }
        return {};
      });

      const onOpenToolMock = vi.fn();
      const onAskPromptMock = vi.fn();

      render(
        <TodayDashboardView
          onOpenTool={onOpenToolMock}
          onAskPrompt={onAskPromptMock}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Authorize Folders")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Authorize Folders"));
      expect(onOpenToolMock).toHaveBeenCalledWith("files", "authorized");

      fireEvent.click(screen.getByText(/Organize & Duplicates/i));
      expect(onOpenToolMock).toHaveBeenCalledWith("files");

      fireEvent.click(screen.getByText("Create a Routine"));
      expect(onOpenToolMock).toHaveBeenCalledWith("routines");
    });
  });

  /* -------------------------------------------------------------
     7. FileOrganizerModal: Duplicate and Recursive Flags & Cancel
     ------------------------------------------------------------- */
  describe("FileOrganizerModal Options and Cancel Plan", () => {
    const mockPlan = {
      operations: [
        {
          source_path: "/tmp/a.pdf",
          destination_path: "/tmp/Documents/a.pdf",
          category: "Documents",
          action: "move",
          status: "pending",
          is_duplicate: false,
          conflict: null,
          size_bytes: 1024,
        },
      ],
      conflicts_count: 0,
      total_size: 1024,
    };

    it("toggles scan options and cancels previewed organization plan", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_authorized_folders") return [];
        if (cmd === "preview_file_organization") return mockPlan;
        return {};
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText(/Detect & group duplicate files via SHA-256 hash/i)).toBeInTheDocument();
      });

      const duplicateCheckbox = screen.getByLabelText(/Detect & group duplicate files via SHA-256 hash/i);
      fireEvent.click(duplicateCheckbox);

      const recursiveCheckbox = screen.getByLabelText(/Scan subfolders recursively/i);
      fireEvent.click(recursiveCheckbox);

      // Generate preview plan
      const pathInput = screen.getByPlaceholderText(/Enter absolute directory path/i);
      fireEvent.change(pathInput, { target: { value: "/tmp/downloads" } });
      fireEvent.click(screen.getByText("Scan & Preview"));

      await waitFor(() => {
        expect(screen.getByText("Execute Organization (1 files)")).toBeInTheDocument();
      });

      // Cancel plan
      fireEvent.click(screen.getByText("Cancel"));
      await waitFor(() => {
        expect(screen.queryByText("Execute Organization (1 files)")).not.toBeInTheDocument();
      });
    });
  });

  /* -------------------------------------------------------------
     8. OnboardingModal: Wizard Step Back & Step 6 Examples
     ------------------------------------------------------------- */
  describe("OnboardingModal Back Navigation & Example Tasks", () => {
    it("navigates back across wizard steps and triggers example tasks", async () => {
      mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "save_api_keys") return {};
        if (cmd === "authorize_folder") return {};
        return {};
      });

      const onTryExampleMock = vi.fn();
      const onCloseMock = vi.fn();

      render(
        <OnboardingModal
          isOpen={true}
          onClose={onCloseMock}
          onTryExample={onTryExampleMock}
          onOpenConnections={vi.fn()}
        />
      );

      // Step 1 -> Next (Click Continue)
      await waitFor(() => {
        expect(screen.getByText("Your Everyday Personal AI Assistant")).toBeInTheDocument();
      });
      const continueBtn = screen.getByRole("button", { name: /Continue/i });
      fireEvent.click(continueBtn);

      // Step 2: AI Model Setup -> Back
      await waitFor(() => {
        expect(screen.getByText("AI Model Setup")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /Back/i })).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole("button", { name: /Back/i }));

      // Back on Step 1
      await waitFor(() => {
        expect(screen.getByText("Your Everyday Personal AI Assistant")).toBeInTheDocument();
      });

      // Advance through Step 1 to Step 6
      fireEvent.click(screen.getByRole("button", { name: /Continue/i })); // to Step 2
      await waitFor(() => {
        expect(screen.getByText("AI Model Setup")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole("button", { name: /Continue/i })); // to Step 3

      await waitFor(() => {
        expect(screen.getByText("Choose Authorized Folders")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole("button", { name: /Continue/i })); // to Step 4

      await waitFor(() => {
        expect(screen.getByText("Optional Connected Services")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole("button", { name: /Continue/i })); // to Step 5

      await waitFor(() => {
        expect(screen.getByText("Privacy & Safety Principles")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole("button", { name: /Continue/i })); // to Step 6

      // Step 6: Ready to explore
      await waitFor(() => {
        expect(screen.getByText("Find an invoice or document")).toBeInTheDocument();
      });

      // Click "Find an invoice or document"
      fireEvent.click(screen.getByText("Find an invoice or document"));
      expect(onTryExampleMock).toHaveBeenCalledWith(
        "Find the invoice I downloaded last month and extract the total amount"
      );
      expect(onCloseMock).toHaveBeenCalled();
    });
  });
});
