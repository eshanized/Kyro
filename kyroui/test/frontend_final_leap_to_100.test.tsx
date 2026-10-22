import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { App } from "../App";
import { SettingsModal } from "../components/SettingsModal";
import { MyToolsView } from "../components/MyToolsView";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { TeachTaskModal } from "../components/TeachTaskModal";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

let agentStepCb: ((ev: any) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: any) => {
    if (event === "agent:step") {
      agentStepCb = cb;
    }
    return Promise.resolve(() => {});
  }),
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

describe("Frontend Final Leap to 95%+ Lines and Branches", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    agentStepCb = null;
  });

  describe("App TTS, Takeover Resume, Approval Resolution, and Message Copy", () => {
    const mockMessages = [
      {
        id: "msg-1",
        conversation_id: "conv-1",
        role: "assistant",
        content: "Here is your system diagnostic summary.",
        created_at: 1000,
      },
    ];

    it("handles speech synthesis read aloud, message copying, and approval decision", async () => {
      // Mock window.speechSynthesis and SpeechSynthesisUtterance
      class MockUtterance {
        text: string;
        onend: (() => void) | null = null;
        onerror: (() => void) | null = null;
        constructor(text: string) {
          this.text = text;
        }
      }
      (global as any).SpeechSynthesisUtterance = MockUtterance;
      (window as any).SpeechSynthesisUtterance = MockUtterance;

      const speakMock = vi.fn((utt: any) => {
        if (utt && typeof utt.onend === "function") {
          setTimeout(utt.onend, 10);
        }
      });
      const cancelMock = vi.fn();
      Object.assign(window, {
        speechSynthesis: {
          speak: speakMock,
          cancel: cancelMock,
        },
      });

      // Mock navigator.clipboard
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      // Mock Notification
      const notificationSpy = vi.fn();
      (window as any).Notification = class MockNotification {
        static permission = "granted";
        static requestPermission = vi.fn().mockResolvedValue("granted");
        constructor(title: string, opts: any) {
          notificationSpy(title, opts);
        }
      };

      const mockApprovals = [
        {
          id: "req-1",
          run_id: "run-1",
          tool_name: "bash",
          description: "Execute rm -rf /tmp/cache",
          status: "pending",
          created_at: 1000,
        },
      ];

      (invoke as any).mockImplementation(async (cmd: string) => {
        if (cmd === "list_conversations") return [{ id: "conv-1", title: "Task 1", model: "gpt-4o", created_at: 1000, updated_at: 1000 }];
        if (cmd === "get_messages") return mockMessages;
        if (cmd === "list_bots") return [];
        if (cmd === "list_pending_approvals") return mockApprovals;
        if (cmd === "get_takeover_status") return { is_active: true, reason: "Manual user password input needed", timestamp: 1000 };
        if (cmd === "get_agent_state") return { is_running: false };
        if (cmd === "resume_takeover") return {};
        if (cmd === "resolve_approval") return {};
        if (cmd === "get_today_dashboard_data") {
          return {
            date_display: "Wednesday, Oct 22",
            greeting: "Good afternoon",
            summary_stats: { routines_executed: 0, pending_approvals: 1, hours_saved: 0 },
            pending_approvals: mockApprovals,
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
        expect(screen.getByText("Here is your system diagnostic summary.")).toBeInTheDocument();
      });

      // 1. Copy message
      const copyBtn = screen.getByTitle("Copy response");
      fireEvent.click(copyBtn);

      await waitFor(() => {
        expect(writeTextMock).toHaveBeenCalledWith("Here is your system diagnostic summary.");
      });

      // 2. Read message aloud (TTS)
      const readBtn = screen.getByTitle("Read message aloud (Local TTS)");
      fireEvent.click(readBtn);
      expect(speakMock).toHaveBeenCalled();

      // Click read aloud again to cancel
      fireEvent.click(readBtn);
      expect(cancelMock).toHaveBeenCalled();

      // 3. Resume Takeover
      const resumeBtn = await screen.findByText("Resume Autonomous Agent", {}, { timeout: 3000 });
      fireEvent.click(resumeBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("resume_takeover");
      });

      // 4. Resolve human approval (Approve button)
      const approveBtn = screen.getByText("Approve");
      fireEvent.click(approveBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("resolve_approval", expect.objectContaining({
          decision: "approved",
        }));
      });

      // 5. Emit live step to exercise step listener and notification
      if (agentStepCb) {
        act(() => {
          agentStepCb({
            payload: {
              step: {
                run_id: "run-live-1",
                step: 1,
                action: "click (150, 200)",
                thought: "Locating button",
                conversation_id: "conv-1",
                timestamp: 1200,
              },
            },
          });
        });
      }
    });

    it("displays error toast when speech recognition is not supported in window", async () => {
      // Unset speech recognition
      delete (window as any).SpeechRecognition;
      delete (window as any).webkitSpeechRecognition;

      (invoke as any).mockImplementation(async (cmd: string) => {
        if (cmd === "list_conversations") return [{ id: "conv-1", title: "Task 1", model: "gpt-4o", created_at: 1000, updated_at: 1000 }];
        if (cmd === "get_messages") return [];
        if (cmd === "list_bots") return [];
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "get_takeover_status") return { is_active: false };
        if (cmd === "get_agent_state") return { is_running: false };
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
        expect(screen.getByTitle(/Voice Dictation/i)).toBeInTheDocument();
      });

      const micBtn = screen.getByTitle(/Voice Dictation/i);
      fireEvent.click(micBtn);

      await waitFor(() => {
        expect(screen.getByText(/Speech recognition is not supported in this browser\/webview engine/i)).toBeInTheDocument();
      });
    });
  });

  describe("MyToolsView Folder Management and Duplicate Detection", () => {
    it("scans duplicates and adds a new authorized folder", async () => {
      const mockFolders = [
        {
          id: "fld-1",
          path: "/home/snigdha/Desktop",
          display_name: "Desktop",
          is_active: true,
          created_at: 1000,
        },
      ];

      const mockDuplicates = [
        {
          file_hash: "hash1234567890abcdef",
          evidence: "Exact identical PDF receipts",
          files: [
            { path: "/home/snigdha/Downloads/invoice.pdf", size: 1024, modified: 1000 },
            { path: "/home/snigdha/Desktop/invoice_copy.pdf", size: 1024, modified: 1000 },
          ],
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_authorized_folders") return Promise.resolve(mockFolders);
        if (cmd === "detect_folder_duplicates") return Promise.resolve(mockDuplicates);
        if (cmd === "add_authorized_folder") {
          return Promise.resolve({
            id: "fld-2",
            path: "/home/snigdha/Documents",
            display_name: "Documents",
            is_active: true,
            created_at: 2000,
          });
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

      // Wait for authorized folders to load
      await screen.findByText("/home/snigdha/Desktop");

      // Click Detect Duplicate Files
      const dupBtn = screen.getByText("Detect Duplicate Files");
      fireEvent.click(dupBtn);

      await waitFor(() => {
        expect(screen.getByText("Exact identical PDF receipts")).toBeInTheDocument();
        expect(screen.getByText("1 duplicate groups")).toBeInTheDocument();
      });

      // Add a folder
      const folderInput = screen.getByPlaceholderText(/Folder path \(e\.g\./i);
      fireEvent.change(folderInput, { target: { value: "/home/snigdha/Documents" } });

      const addBtn = screen.getByText("Authorize");
      fireEvent.click(addBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("add_authorized_folder", expect.objectContaining({
          path: "/home/snigdha/Documents",
        }));
      });
    });
  });

  describe("ArtifactsModal Filtering and Export", () => {
    it("filters artifacts by search and triggers artifact download/export", async () => {
      const mockArtifacts = [
        {
          id: "art-1",
          name: "Quarterly Audit Report",
          conversation_id: "conv-1",
          artifact_type: "report",
          file_path: "/tmp/kyro_artifacts/audit.md",
          size_bytes: 1024,
          created_at: 1000,
          summary: "Complete financial and compliance review",
          metadata: "{}",
          comments: [],
        },
        {
          id: "art-2",
          name: "Architecture Visual Baseline",
          conversation_id: "conv-1",
          artifact_type: "diagram",
          file_path: "/tmp/kyro_artifacts/arch.png",
          size_bytes: 2048,
          created_at: 2000,
          summary: "Component topology graph",
          metadata: "{}",
          comments: [],
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_artifacts") return Promise.resolve(mockArtifacts);
        if (cmd === "read_artifact_content") return Promise.resolve("# Audit Report\n\nAll pass.");
        if (cmd === "export_artifact") return Promise.resolve("/home/snigdha/Desktop/audit.md");
        return Promise.resolve([]);
      });

      render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getAllByText("Quarterly Audit Report").length).toBeGreaterThan(0);
        expect(screen.getByText("Architecture Visual Baseline")).toBeInTheDocument();
      });

      // Search filter
      const searchInput = screen.getByPlaceholderText(/Filter artifacts/i);
      fireEvent.change(searchInput, { target: { value: "Audit" } });

      expect(screen.getAllByText("Quarterly Audit Report").length).toBeGreaterThan(0);
      expect(screen.queryByText("Architecture Visual Baseline")).not.toBeInTheDocument();

      // Export artifact
      const exportInput = screen.getByPlaceholderText(/Enter destination file path to export/i);
      fireEvent.change(exportInput, { target: { value: "/home/snigdha/Desktop/audit.md" } });

      const exportBtn = screen.getByText("Export");
      fireEvent.click(exportBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("export_artifact", expect.objectContaining({
          id: "art-1",
          destinationPath: "/home/snigdha/Desktop/audit.md",
        }));
      });
    });
  });

  describe("TeachTaskModal Workflow Tabs", () => {
    it("switches to record tab and initiates recording countdown", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_learned_skills") return Promise.resolve([]);
        if (cmd === "start_teach_task") return Promise.resolve();
        if (cmd === "stop_teach_task") return Promise.resolve();
        return Promise.resolve([]);
      });

      render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="record" />);

      await waitFor(() => {
        expect(screen.getByText("Demonstration-Based Learning")).toBeInTheDocument();
      });

      // Name skill and start recording
      const skillNameInput = screen.getByPlaceholderText(/e\.g\. Export Daily Analytics Spreadsheet/i);
      fireEvent.change(skillNameInput, { target: { value: "Export Invoice PDF" } });

      const startRecBtn = screen.getByText("Start Recording Workflow");
      fireEvent.click(startRecBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("start_teach_task", expect.objectContaining({
          name: "Export Invoice PDF",
        }));
        expect(screen.getByText("RECORDING ACTIVE")).toBeInTheDocument();
      });

      // Stop recording
      const stopBtn = screen.getByText("Save & Finish Skill");
      fireEvent.click(stopBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("stop_teach_task");
      });
    });
  });
});
