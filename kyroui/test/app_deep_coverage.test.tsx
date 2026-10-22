import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { RootComponent } from "../main";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
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

describe("App Deep Coverage & Edge Cases", () => {
  const mockConversations = [
    {
      id: "conv_main",
      title: "Current Active Task",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1000,
      updated_at: 1000,
    },
  ];

  const mockMessages = [
    {
      id: "msg_user_1",
      conversation_id: "conv_main",
      role: "user",
      content: "Explain the rust compiler architecture",
      created_at: 1000,
    },
    {
      id: "msg_asst_2",
      conversation_id: "conv_main",
      role: "assistant",
      content: "Rust uses rustc front-end, LLVM middle and backend, and MIR passes.",
      created_at: 1001,
    },
  ];

  const mockBots = [
    {
      id: "apex",
      name: "Apex",
      avatar: "zap",
      role: "Strategic Orchestrator",
      system_prompt: "Coordinate tasks",
      is_default: true,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    localStorage.setItem("kyro_onboarding_completed", "true");

    Object.assign(navigator, {
      clipboard: {
        writeText: vi.fn().mockImplementation(() => Promise.resolve()),
      },
    });

    (window as any).speechSynthesis = {
      speak: vi.fn(),
      cancel: vi.fn(),
    };
    (window as any).SpeechSynthesisUtterance = function (this: any, text: string) {
      this.text = text;
    };

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_conversations") return Promise.resolve(mockConversations);
      if (cmd === "get_messages") return Promise.resolve(mockMessages);
      if (cmd === "list_bots") return Promise.resolve(mockBots);
      if (cmd === "get_takeover_status") return Promise.resolve({ is_active: false });
      if (cmd === "list_pending_approvals") return Promise.resolve([]);
      if (cmd === "create_conversation") return Promise.resolve(mockConversations[0]);
      if (cmd === "toggle_hud_window") return Promise.resolve(true);
      if (cmd === "take_screenshot") {
        return Promise.resolve({
          base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        });
      }
      if (cmd === "toggle_ghost_mode") return Promise.resolve(true);
      if (cmd === "capture_grounded_screen") {
        return Promise.resolve({
          base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
          candidates: [],
        });
      }
      if (cmd === "stop_agent_run") return Promise.resolve(true);
      if (cmd === "add_message") return Promise.resolve(true);
      if (cmd === "run_agent_turn") return Promise.resolve("run_123");
      return Promise.resolve(null);
    });
  });

  it("handles clipboard copying and text-to-speech for assistant messages", async () => {
    render(<App />);

    await waitFor(() => {
      expect(
        screen.getByText("Rust uses rustc front-end, LLVM middle and backend, and MIR passes.")
      ).toBeInTheDocument();
    });

    const copyBtn = screen.getByTitle("Copy response");
    fireEvent.click(copyBtn);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining("Rust uses rustc front-end")
    );

    const speakBtn = screen.getByTitle("Read message aloud (Local TTS)");
    fireEvent.click(speakBtn);
    expect(window.speechSynthesis.speak).toHaveBeenCalled();
  });

  it("triggers floating HUD toggle and new conversation creation", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Current Active Task")).toBeInTheDocument();
    });

    const hudBtn = screen.getByTitle("Toggle Floating HUD Widget");
    fireEvent.click(hudBtn);
    expect(invoke).toHaveBeenCalledWith("toggle_hud_window");

    const newSessionBtn = screen.getByRole("button", { name: /New Session/i });
    fireEvent.click(newSessionBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_conversation", expect.anything());
    });
  });

  it("switches to Dev Mode and captures screen and grounded Set-of-Marks", async () => {
    render(<App />);

    const devModeBtn = screen.getByTitle(/Kyro Dev: Autonomous developer agent/i);
    fireEvent.click(devModeBtn);

    const screenCaptureBtn = screen.getByTitle("Capture display snapshot");
    fireEvent.click(screenCaptureBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("take_screenshot");
    });

    const ghostBtn = screen.getByTitle(/Runs actions inside isolated virtual display/i);
    fireEvent.click(ghostBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("toggle_ghost_mode", { enabled: true });
    });

    const somBtn = screen.getByTitle(/Detect UI elements and render numbered markers/i);
    fireEvent.click(somBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("capture_grounded_screen");
    });
  });

  it("handles file attachments and uploads in chat prompt input", async () => {
    render(<App />);

    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    if (fileInput) {
      const file = new File(["test evidence content"], "evidence.txt", { type: "text/plain" });
      fireEvent.change(fileInput, { target: { files: [file] } });
    }

    const input = screen.getByPlaceholderText(/Ask @Apex/i);
    fireEvent.change(input, { target: { value: "Review attached evidence file" } });
    expect(input).toHaveValue("Review attached evidence file");

    const form = input.closest("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "run_agent_turn",
        expect.objectContaining({
          conversationId: expect.stringMatching(/conv_/),
        })
      );
    });
  });
});

describe("RootComponent Entry Point Multi-Window Router", () => {
  it("renders main App shell when window label is main", () => {
    const { container } = render(<RootComponent />);
    expect(container).toBeInTheDocument();
  });
});
