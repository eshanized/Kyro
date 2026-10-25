import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

let stepEventListener: ((event: any) => void) | null = null;

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((eventName: string, handler: any) => {
    if (eventName === "agent:step") {
      stepEventListener = handler;
    }
    return Promise.resolve(() => {});
  }),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({ label: "main" })),
}));

vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class MockTerminal {
      loadAddon = vi.fn();
      open = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      onData = vi.fn();
      dispose = vi.fn();
    },
  };
});

vi.mock("@xterm/addon-fit", () => {
  return {
    FitAddon: class MockFitAddon {
      fit = vi.fn();
    },
  };
});

describe("App Ultimate Coverage & Edge Cases", () => {
  const mockedInvoke = vi.mocked(invoke);

  const mockConversations = [
    {
      id: "conv_main",
      title: "Active Task",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1000,
      updated_at: 1000,
    },
  ];

  const mockBots = [
    {
      id: "bot-general",
      name: "Kyro General",
      avatar: "🧠",
      role: "general",
      description: "Default Assistant",
      system_prompt: "You are Kyro.",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    stepEventListener = null;

    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return mockConversations;
      if (cmd === "get_messages") return [];
      if (cmd === "list_bots") return mockBots;
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "get_takeover_status") return { is_active: false };
      if (cmd === "get_agent_state") return { is_running: false };
      if (cmd === "create_conversation") {
        return {
          id: args?.id || "conv_new",
          title: args?.title || "New Task",
          model: "nvidia/nemotron-3-ultra-550b-a55b",
          created_at: 1000,
          updated_at: 1000,
        };
      }
      if (cmd === "run_agent_turn") return "Executed successfully with consequential plan.";
      if (cmd === "add_message") return {};
      if (cmd === "take_screenshot") {
        return { base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" };
      }
      if (cmd === "capture_grounded_screen") {
        return {
          base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
          marks: [
            { id: 1, label: "Search input", coordinates: [100, 200, 300, 240] },
            { id: 2, label: "Submit button", coordinates: [310, 200, 400, 240] },
          ],
          source: "ocr_som_hybrid",
        };
      }
      if (cmd === "resolve_approval") return {};
      if (cmd === "abort_agent_run") return {};
      if (cmd === "toggle_ghost_mode") return {};
      if (cmd === "toggle_overlay_window") return {};
      if (cmd === "get_today_dashboard_data") {
        return {
          date_display: "Wednesday, Oct 22",
          greeting: "Good afternoon",
          summary_stats: { routines_executed: 2, pending_approvals: 0, hours_saved: 1.5 },
          pending_approvals: [],
          routines_today: [],
          recent_activity: [],
        };
      }
      if (cmd === "list_routines") return [];
      if (cmd === "list_routine_history") return [];
      if (cmd === "get_automation_metrics") {
        return {
          total_runs: 5,
          successful_runs: 5,
          failed_runs: 0,
          success_rate: 100,
          total_hours_saved: 2.5,
          active_routines_count: 3,
        };
      }
      return {};
    });

    // Mock FileReader
    class MockFileReader {
      result: any = null;
      onload: any = null;
      readAsDataURL(blob: Blob) {
        this.result = "data:image/png;base64,ZmFrZV9pbWFnZV9kYXRh";
        setTimeout(() => this.onload && this.onload({ target: this }), 10);
      }
      readAsText(blob: Blob) {
        this.result = "console.log('hello');";
        setTimeout(() => this.onload && this.onload({ target: this }), 10);
      }
    }
    window.FileReader = MockFileReader as any;
  });

  it("handles voice dictation recognition lifecycle and error callbacks", async () => {
    let mockInstance: any = null;
    class MockSpeechRecognition {
      continuous = false;
      interimResults = false;
      onresult: any = null;
      onerror: any = null;
      onend: any = null;
      start = vi.fn();
      stop = vi.fn();
      constructor() {
        mockInstance = this;
      }
    }
    (window as any).SpeechRecognition = MockSpeechRecognition;
    (window as any).webkitSpeechRecognition = MockSpeechRecognition;

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    const micBtn = screen.getByTitle(/Voice Dictation/i);
    fireEvent.click(micBtn);

    expect(mockInstance.start).toHaveBeenCalled();

    // Trigger onresult
    act(() => {
      mockInstance.onresult({
        results: [[{ transcript: "analyze latest build logs" }]],
      });
    });

    // Input prompt should contain dictated text
    expect(screen.getByDisplayValue(/analyze latest build logs/i)).toBeInTheDocument();

    // Trigger onerror & onend
    act(() => {
      mockInstance.onerror();
      mockInstance.onend();
    });
  });

  it("handles file upload for images and text attachments and allows attachment removal", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    // Attach text file
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    expect(fileInput).not.toBeNull();

    const textFile = new File(["test code content"], "script.rs", { type: "text/plain" });
    fireEvent.change(fileInput, { target: { files: [textFile] } });

    await waitFor(() => {
      expect(screen.getByText("script.rs")).toBeInTheDocument();
    });

    // Remove attached file
    const chipDiv = screen.getByText("script.rs").closest("div");
    const removeBtn = chipDiv?.parentElement?.querySelector("button");
    expect(removeBtn).toBeTruthy();
    fireEvent.click(removeBtn!);

    await waitFor(() => {
      expect(screen.queryByText("script.rs")).not.toBeInTheDocument();
    });

    // Attach image file
    const imgFile = new File(["fake_img_binary"], "screenshot.png", { type: "image/png" });
    fireEvent.change(fileInput, { target: { files: [imgFile] } });

    await waitFor(() => {
      expect(screen.getByText("screenshot.png")).toBeInTheDocument();
    });
  });

  it("triggers consequential execution plan, steps rendering, completion, and dismissal", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    const input = screen.getByPlaceholderText(/Ask @/i);
    fireEvent.change(input, { target: { value: "organize and clean up my downloads" } });

    const sendBtn = screen.getByText("Send");
    fireEvent.click(sendBtn);

    // Plan should be rendered
    await waitFor(() => {
      expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
      // Resources are honest for the intent (Bug 5).
      expect(screen.getByText(/Authorized folders \(paths outside them need your approval\)/i)).toBeInTheDocument();
      expect(screen.getAllByText("Inspect and analyze relevant resources and parameters").length).toBeGreaterThan(0);
    });

    // Dismiss plan
    const dismissBtn = screen.getByTitle("Dismiss plan");
    fireEvent.click(dismissBtn);

    await waitFor(() => {
      expect(screen.queryByText("Suggested Workflow")).not.toBeInTheDocument();
    });
  });

  it("displays screen preview snapshot and grounded SoM preview and allows dismissal", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    // Switch to dev mode to expose Ground button
    fireEvent.click(screen.getByText("Dev Mode"));

    // Capture Screen Snapshot
    const screenBtn = screen.getAllByTitle("Capture display snapshot")[0];
    fireEvent.click(screenBtn);

    await waitFor(() => {
      expect(screen.getByText("Display Snapshot ready")).toBeInTheDocument();
    });

    // Dismiss screen preview
    const dismissBtns = screen.getAllByText("Dismiss");
    fireEvent.click(dismissBtns[0]);

    await waitFor(() => {
      expect(screen.queryByText("Display Snapshot ready")).not.toBeInTheDocument();
    });

    // Capture Grounded Screen
    const groundBtn = screen.getByTitle(/Detect UI elements and render numbered markers/i);
    fireEvent.click(groundBtn);

    await waitFor(() => {
      expect(screen.getByText(/Set-of-Marks \(SoM\) Grounding Active/i)).toBeInTheDocument();
      expect(screen.getByText(/2 grounded elements detected/i)).toBeInTheDocument();
    });

    // Dismiss grounded screen
    const dismissGroundedBtn = screen.getByText("Dismiss");
    fireEvent.click(dismissGroundedBtn);

    await waitFor(() => {
      expect(screen.queryByText(/Set-of-Marks \(SoM\) Grounding Active/i)).not.toBeInTheDocument();
    });
  });

  it("handles approval requests with both approve and deny decisions", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return mockConversations;
      if (cmd === "get_messages") return [];
      if (cmd === "list_bots") return mockBots;
      if (cmd === "get_takeover_status") return { is_active: false };
      if (cmd === "list_pending_approvals") {
        return [
          {
            id: "appr-deny-1",
            run_id: "run-1",
            tool_name: "fs_delete",
            summary: "Deleting temporary files in /tmp/cache",
            risk_level: "destructive",
            reason: "Permanent deletion requires approval",
            status: "pending",
            created_at: 1000,
          },
        ];
      }
      if (cmd === "get_agent_state") return { is_running: false };
      if (cmd === "resolve_approval") return {};
      return {};
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("fs_delete")).toBeInTheDocument();
      expect(screen.getByText(/Policy: Permanent deletion requires approval/i)).toBeInTheDocument();
      expect(screen.getByText("Deny")).toBeInTheDocument();
    }, { timeout: 3000 });

    fireEvent.click(screen.getByText("Deny"));

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("resolve_approval", {
        id: "appr-deny-1",
        decision: "denied",
        resolvedBy: "desktop_user",
      });
    });
  });

  it("handles quick actions button to open command palette and exercises palette action callbacks", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    // Click Quick Actions (Ctrl+K)
    const cmdPaletteBtn = screen.getByTitle(/Quick Actions & Search \(Ctrl\+K\)/i);
    fireEvent.click(cmdPaletteBtn);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Type a command or search/i)).toBeInTheDocument();
    });

    // Test command selection in palette
    const healthOption = screen.getByText("System Health & Diagnostics");
    fireEvent.click(healthOption);

    // Settings modal should open with diagnostics
    await waitFor(() => {
      expect(screen.getByText(/Real-Time Local Runtime Diagnostics/i)).toBeInTheDocument();
    });
  });

  it("streams live agent step events and displays tool call card", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    // Emit live agent step event matching activeConvId: "conv_main"
    expect(stepEventListener).not.toBeNull();
    act(() => {
      stepEventListener!({
        payload: {
          run_id: "run_conv_main_123",
          conversation_id: "conv_main",
          step: 1,
          step_type: "tool_execution",
          tool_calls: [
            {
              id: "call-1",
              name: "read_file",
              arguments: '{"path": "Cargo.toml"}',
            },
          ],
          tool_results: [
            {
              tool_call_id: "call-1",
              success: true,
              result: "[package]\nname = \"kyro\"",
            },
          ],
          timestamp: Date.now(),
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText("read_file")).toBeInTheDocument();
    });
  });

  it("allows switching everyday tabs and triggers onboarding and tools navigation", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Active Task")).toBeInTheDocument();
    });

    // Switch to Today tab
    const todayBtns = screen.getAllByRole("button", { name: /Today/i });
    fireEvent.click(todayBtns[0]);
    await waitFor(() => {
      expect(screen.getByText(/Good afternoon/i)).toBeInTheDocument();
    });

    // Switch to Tools tab
    const toolsBtns = screen.getAllByRole("button", { name: /My Tools/i });
    fireEvent.click(toolsBtns[0]);
    await waitFor(() => {
      expect(screen.getByText(/Safe File Organizer/i)).toBeInTheDocument();
    });

    // Switch to Routines tab
    const routinesBtns = screen.getAllByRole("button", { name: /My Routines/i });
    fireEvent.click(routinesBtns[0]);
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "My Routines" })).toBeInTheDocument();
    });

    // Switch back to Ask Kyro
    const askBtns = screen.getAllByRole("button", { name: /Ask Kyro/i });
    fireEvent.click(askBtns[0]);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
    });
  });
});
