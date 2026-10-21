import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { TodayDashboardView } from "../components/TodayDashboardView";
import { MyToolsView } from "../components/MyToolsView";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { OnboardingModal } from "../components/OnboardingModal";
import { SettingsModal } from "../components/SettingsModal";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("Kyro Everyday Destinations & Workflows", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    localStorage.setItem("kyro_onboarding_completed", "true");
  });

  describe("TodayDashboardView", () => {
    it("renders truthful empty states when Google accounts are not connected", async () => {
      const mockEmptyData = {
        greeting: "Good morning",
        date_display: "Wednesday, Oct 22",
        upcoming_events: [],
        email_summary: {
          is_connected: false,
          unread_count: 0,
          account_email: null,
          recent_snippets: [],
        },
        recent_documents: [],
        upcoming_routines: [],
        recent_routine_results: [],
        pending_tasks_count: 0,
        services: [],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_today_dashboard_data") {
          return Promise.resolve(mockEmptyData);
        }
        return Promise.resolve(null);
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
        expect(
          screen.getByText("Here is your daily briefing from your local computer and connected accounts.")
        ).toBeInTheDocument();
        expect(screen.getByText("No calendar events found")).toBeInTheDocument();
        expect(screen.getByText("No email account connected")).toBeInTheDocument();
      });

      expect(screen.getByText("Connect Google Calendar")).toBeInTheDocument();
      expect(screen.getByText("Connect Gmail")).toBeInTheDocument();
    });

    it("renders calendar events and Gmail summary when connected", async () => {
      const mockConnectedData = {
        greeting: "Welcome back",
        date_display: "Wednesday, Oct 22",
        upcoming_events: [
          {
            id: "evt-1",
            title: "College Project Review",
            time_display: "2:00 PM - 3:00 PM",
            location: "Room 302",
            html_link: "https://calendar.google.com/event?id=1",
            is_today: true,
          },
        ],
        email_summary: {
          is_connected: true,
          unread_count: 3,
          account_email: "student@example.com",
          recent_snippets: ["Invoice #1024 for tuition", "Meeting reschedule"],
        },
        recent_documents: [
          {
            id: "doc-1",
            path: "/home/user/Documents/syllabus.pdf",
            file_name: "syllabus.pdf",
            size_bytes: 4096,
            modified_at: 1700000000,
          },
        ],
        upcoming_routines: [
          {
            id: "rt-1",
            name: "Friday Downloads Cleanup",
            cron: "0 17 * * 5",
            next_run: 1700100000,
          },
        ],
        recent_routine_results: [],
        pending_tasks_count: 0,
        services: [],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_today_dashboard_data") {
          return Promise.resolve(mockConnectedData);
        }
        return Promise.resolve(null);
      });

      const handleAskPrompt = vi.fn();
      render(
        <TodayDashboardView
          onOpenTool={vi.fn()}
          onAskPrompt={handleAskPrompt}
          onOpenConnections={vi.fn()}
          onOpenSettings={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("College Project Review")).toBeInTheDocument();
        expect(screen.getByText("Connected to Gmail")).toBeInTheDocument();
        expect(screen.getByText("syllabus.pdf")).toBeInTheDocument();
      });

      const draftBtn = screen.getByText("Summarize Unread & Draft Replies");
      fireEvent.click(draftBtn);
      expect(handleAskPrompt).toHaveBeenCalledWith("Summarize my unread emails and draft replies");
    });
  });

  describe("MyToolsView", () => {
    it("searches authorized documents with content snippets", async () => {
      const mockSearchResults = [
        {
          path: "/home/user/Documents/invoice_september.pdf",
          file_name: "invoice_september.pdf",
          format: "pdf",
          size_bytes: 2048,
          modified_at: 1700000000,
          matched_by: "content",
          snippet: "Invoice #9823 Total Amount: $450.00 Due: Oct 15",
          relevance_score: 10,
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_authorized_folders") {
          return Promise.resolve([{ id: "f1", path: "/home/user/Documents", display_name: "Documents", added_at: 1700000000 }]);
        }
        if (cmd === "search_authorized_documents") {
          return Promise.resolve(mockSearchResults);
        }
        return Promise.resolve([]);
      });

      render(
        <MyToolsView
          onAskPrompt={vi.fn()}
          onOpenConnections={vi.fn()}
          onOpenScreenExplainer={vi.fn()}
          onOpenFileOrganizer={vi.fn()}
          onOpenDocAssistant={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Natural-Language Document Search")).toBeInTheDocument();
      });

      const searchInput = screen.getByPlaceholderText(/Search keywords, filenames, or content/i);
      fireEvent.change(searchInput, { target: { value: "invoice" } });

      const searchBtn = screen.getByRole("button", { name: "Search" });
      fireEvent.click(searchBtn);

      await waitFor(() => {
        expect(screen.getByText("invoice_september.pdf")).toBeInTheDocument();
        expect(screen.getByText(/Invoice #9823 Total Amount: \$450.00/)).toBeInTheDocument();
      });
    });

    it("runs duplicate detection with SHA-256 evidence", async () => {
      const mockDuplicates = [
        {
          file_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
          size_bytes: 1024,
          evidence: "Exact SHA-256 binary match across 2 files",
          files: [
            { path: "/home/user/Documents/report.docx", size_bytes: 1024, modified_at: 1700000000, is_original: true },
            { path: "/home/user/Downloads/report_copy.docx", size_bytes: 1024, modified_at: 1700001000, is_original: false },
          ],
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_authorized_folders") {
          return Promise.resolve([{ id: "f1", path: "/home/user/Downloads", display_name: "Downloads", added_at: 1700000000 }]);
        }
        if (cmd === "detect_folder_duplicates") {
          return Promise.resolve(mockDuplicates);
        }
        return Promise.resolve([]);
      });

      render(
        <MyToolsView
          onAskPrompt={vi.fn()}
          onOpenConnections={vi.fn()}
          onOpenScreenExplainer={vi.fn()}
          onOpenFileOrganizer={vi.fn()}
          onOpenDocAssistant={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Detect Duplicate Files")).toBeInTheDocument();
      });

      const scanBtn = screen.getByText("Detect Duplicate Files");
      fireEvent.click(scanBtn);

      await waitFor(() => {
        expect(screen.getByText("Duplicate Files Detected")).toBeInTheDocument();
        expect(screen.getByText("/home/user/Documents/report.docx")).toBeInTheDocument();
        expect(screen.getByText("/home/user/Downloads/report_copy.docx")).toBeInTheDocument();
        expect(screen.getByText(/Exact SHA-256 binary match/)).toBeInTheDocument();
      });
    });

    it("generates study revision notes and passage explanations", async () => {
      const mockNotes = {
        doc_name: "lecture.pdf",
        doc_path: "/home/user/Documents/lecture.pdf",
        format: "pdf",
        overview: "This document covers virtual memory, processes, and page tables.",
        core_concepts: ["Virtual memory isolates processes", "TLB caches page table entries"],
        key_dates_and_milestones: ["Midterm Exam: Nov 10", "Homework 2 Due: Oct 28"],
        summary_points: ["Paging isolates virtual addresses from physical memory"],
        revision_flashcards: [
          {
            question: "What is the purpose of page tables?",
            answer: "To map virtual addresses to physical frame addresses.",
          },
        ],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "generate_study_notes") {
          return Promise.resolve(mockNotes);
        }
        return Promise.resolve([]);
      });

      render(
        <MyToolsView
          initialTab="study"
          onAskPrompt={vi.fn()}
          onOpenConnections={vi.fn()}
          onOpenScreenExplainer={vi.fn()}
          onOpenFileOrganizer={vi.fn()}
          onOpenDocAssistant={vi.fn()}
        />
      );

      const docInput = screen.getByPlaceholderText(/Enter path to document or PDF file/i);
      fireEvent.change(docInput, { target: { value: "/home/user/Documents/lecture.pdf" } });

      const generateBtn = screen.getByText("Generate Notes");
      fireEvent.click(generateBtn);

      await waitFor(() => {
        expect(screen.getByText(/Overview: lecture\.pdf/)).toBeInTheDocument();
        expect(screen.getByText("Midterm Exam: Nov 10")).toBeInTheDocument();
        expect(screen.getByText("What is the purpose of page tables?")).toBeInTheDocument();
      });
    });
  });

  describe("MyRoutinesView", () => {
    it("converts natural language prompt to scheduled routine suggestion", async () => {
      const mockSuggestion = {
        name: "Weekly Document Organizer",
        cron_expression: "0 17 * * 5",
        action_type: "organize_files",
        target_id: "weekly_file_organization",
        explanation: "Runs every Friday at 5:00 PM to organize downloaded files.",
        required_resources: ["Downloads Folder"],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_routines") return Promise.resolve([]);
        if (cmd === "list_routine_runs") return Promise.resolve([]);
        if (cmd === "natural_language_to_routine") return Promise.resolve(mockSuggestion);
        return Promise.resolve(null);
      });

      render(<MyRoutinesView onAskPrompt={vi.fn()} />);

      const nlInput = screen.getByPlaceholderText(/e\.g\. 'Every Friday at 9am/i);
      fireEvent.change(nlInput, { target: { value: "Every Friday, organize my downloads" } });

      const planBtn = screen.getByText("Build Routine");
      fireEvent.click(planBtn);

      await waitFor(() => {
        expect(screen.getByText("Proposed Routine Review")).toBeInTheDocument();
        expect(screen.getByDisplayValue("Weekly Document Organizer")).toBeInTheDocument();
        expect(screen.getByText("0 17 * * 5")).toBeInTheDocument();
        expect(screen.getByText("Runs every Friday at 5:00 PM to organize downloaded files.")).toBeInTheDocument();
      });
    });
  });

  describe("OnboardingModal", () => {
    it("navigates through steps and saves completion in localStorage", async () => {
      localStorage.clear();
      (invoke as any).mockImplementation((_cmd: string) => {
        return Promise.resolve(null);
      });
      const handleClose = vi.fn();
      render(
        <OnboardingModal
          isOpen={true}
          onClose={handleClose}
          onTryExample={vi.fn()}
          onOpenConnections={vi.fn()}
        />
      );

      expect(screen.getByText("Your Everyday Personal AI Assistant")).toBeInTheDocument();

      // Step 1 -> 2
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
      await waitFor(() => {
        expect(screen.getByText("AI Model Setup")).toBeInTheDocument();
      });

      // Step 2 -> 3
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
      await waitFor(() => {
        expect(screen.getByText("Choose Authorized Folders")).toBeInTheDocument();
      });

      // Step 3 -> 4
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
      await waitFor(() => {
        expect(screen.getByText("Optional Connected Services")).toBeInTheDocument();
      });

      // Step 4 -> 5
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
      await waitFor(() => {
        expect(screen.getByText("Privacy & Safety Principles")).toBeInTheDocument();
      });

      // Step 5 -> 6
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
      await waitFor(() => {
        expect(screen.getByText("Ready to Get Started!")).toBeInTheDocument();
      });

      // Finish
      fireEvent.click(screen.getByRole("button", { name: /Get Started/i }));
      expect(localStorage.getItem("kyro_onboarding_completed")).toBe("true");
      expect(handleClose).toHaveBeenCalled();
    });
  });

  describe("SettingsModal Privacy & Folders Tab", () => {
    it("lists authorized folders and authorizes a new folder", async () => {
      const mockFolders = [
        { id: "f1", path: "/home/user/Documents", display_name: "Documents", added_at: 1700000000 },
      ];

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "get_api_keys") {
          return Promise.resolve({ nvidia_configured: false, telegram_configured: false, telegram_chat_id: "" });
        }
        if (cmd === "get_governance_policy") {
          return Promise.resolve({
            shell_mode: "SafeAutoApprove",
            computer_use_mode: "FullAutonomous",
            require_takeover_for_credentials: true,
            blocked_domains: [],
          });
        }
        if (cmd === "get_sandbox_status") return Promise.resolve(null);
        if (cmd === "get_browser_capabilities") return Promise.resolve(null);
        if (cmd === "get_browser_settings") return Promise.resolve(null);
        if (cmd === "get_system_health") return Promise.resolve(null);
        if (cmd === "get_starter_templates") return Promise.resolve([]);
        if (cmd === "list_authorized_folders") return Promise.resolve(mockFolders);
        if (cmd === "add_authorized_folder") {
          return Promise.resolve({ id: "f2", path: args.path, display_name: args.label || "Folder", added_at: 1700000000 });
        }
        return Promise.resolve(null);
      });

      render(
        <SettingsModal
          isOpen={true}
          onClose={vi.fn()}
          selectedModel="nvidia/nemotron-3-ultra-550b-a55b"
          onSelectModel={vi.fn()}
          initialTab="privacy"
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Local-First Architecture & Data Isolation")).toBeInTheDocument();
        expect(screen.getByText("Documents")).toBeInTheDocument();
        expect(screen.getByText("/home/user/Documents")).toBeInTheDocument();
      });

      // Add new folder
      const pathInput = screen.getByPlaceholderText(/Folder path \(e\.g\. \/home\/user\/Documents\)/i);
      fireEvent.change(pathInput, { target: { value: "/home/user/College" } });

      const labelInput = screen.getByPlaceholderText(/Label \(optional, e\.g\. College Work\)/i);
      fireEvent.change(labelInput, { target: { value: "College Work" } });

      const addBtn = screen.getByRole("button", { name: "Authorize Folder" });
      fireEvent.click(addBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("add_authorized_folder", expect.objectContaining({
          path: "/home/user/College",
          label: "College Work",
        }));
      });
    });
  });

  describe("App 4-Destination Navigation", () => {
    it("switches smoothly between Ask Kyro, Today, My Tools, and My Routines", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_conversations") return Promise.resolve([]);
        if (cmd === "get_default_bot") return Promise.resolve({ id: "apex", name: "Apex", avatar: "zap", role: "Assistant", is_default: true });
        if (cmd === "get_takeover_status") return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        if (cmd === "list_pending_approvals") return Promise.resolve([]);
        if (cmd === "get_today_dashboard_data") {
          return Promise.resolve({
            greeting: "Good morning",
            date_display: "Wednesday, Oct 22",
            upcoming_events: [],
            email_summary: { is_connected: false, unread_count: 0, account_email: null, recent_snippets: [] },
            recent_documents: [],
            upcoming_routines: [],
            recent_routine_results: [],
            pending_tasks_count: 0,
            services: [],
          });
        }
        if (cmd === "list_authorized_folders") return Promise.resolve([]);
        if (cmd === "list_routines") return Promise.resolve([]);
        if (cmd === "list_routine_runs") return Promise.resolve([]);
        return Promise.resolve(null);
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
      });

      // Click "Today" destination in sidebar
      const todayButtons = screen.getAllByText("Today");
      fireEvent.click(todayButtons[0]);

      await waitFor(() => {
        expect(
          screen.getByText("Here is your daily briefing from your local computer and connected accounts.")
        ).toBeInTheDocument();
      });

      // Click "My Tools" destination
      const toolsButtons = screen.getAllByText("My Tools");
      fireEvent.click(toolsButtons[0]);

      await waitFor(() => {
        expect(screen.getByText("Natural-Language Document Search")).toBeInTheDocument();
      });

      // Click "My Routines" destination
      const routinesButtons = screen.getAllByText("My Routines");
      fireEvent.click(routinesButtons[0]);

      await waitFor(() => {
        expect(screen.getByRole("heading", { name: "My Routines" })).toBeInTheDocument();
      });

      // Return to "Ask Kyro"
      const askButtons = screen.getAllByText("Ask Kyro");
      fireEvent.click(askButtons[0]);

      await waitFor(() => {
        expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
      });
    });
  });
});
