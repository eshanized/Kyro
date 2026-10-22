import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { HudPanel } from "../hud/HudPanel";
import { Snipper } from "../hud/Snipper";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockHide = vi.fn();
const mockShow = vi.fn();
const mockSetFocus = vi.fn();

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    hide: mockHide,
  })),
  WebviewWindow: {
    getByLabel: vi.fn().mockImplementation((label: string) => {
      if (label === "main") {
        return Promise.resolve({
          show: mockShow,
          setFocus: mockSetFocus,
        });
      }
      return Promise.resolve(null);
    }),
  },
}));

describe("HUD and Snipper Components", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("HudPanel", () => {
    it("renders HUD initial state, polls takeover status and pending approvals", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_takeover_status") {
          return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        }
        if (cmd === "list_pending_approvals") {
          return Promise.resolve([]);
        }
        return Promise.resolve(null);
      });

      render(<HudPanel />);
      expect(screen.getByText("Kyro HUD")).toBeInTheDocument();
      expect(screen.getByText("Nemotron-3")).toBeInTheDocument();
      expect(screen.getByText("Ready")).toBeInTheDocument();

      // Advance interval
      await act(async () => {
        vi.advanceTimersByTime(900);
      });

      expect(invoke).toHaveBeenCalledWith("get_takeover_status");
      expect(invoke).toHaveBeenCalledWith("list_pending_approvals");
    });

    it("displays and handles human takeover banner when active", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_takeover_status") {
          return Promise.resolve({ is_active: true, reason: "Captcha detected", timestamp: 12345 });
        }
        if (cmd === "list_pending_approvals") {
          return Promise.resolve([]);
        }
        if (cmd === "resume_takeover") {
          return Promise.resolve(null);
        }
        return Promise.resolve(null);
      });

      render(<HudPanel />);

      await act(async () => {
        vi.advanceTimersByTime(900);
      });

      expect(screen.getByText("Captcha detected")).toBeInTheDocument();
      const resumeBtn = screen.getByRole("button", { name: /Resume/i });
      expect(resumeBtn).toBeInTheDocument();

      await act(async () => {
        fireEvent.click(resumeBtn);
      });

      expect(invoke).toHaveBeenCalledWith("resume_takeover");
    });

    it("displays pending approvals and resolves them (approve / deny)", async () => {
      const mockApprovals = [
        {
          id: "app-1",
          run_id: "run-101",
          tool_name: "delete_file",
          arguments_json: "{}",
          risk_level: "high",
          summary: "Delete database backup file",
          status: "pending",
          created_at: 1000,
          resolved_at: null,
          resolved_by: null,
        },
        {
          id: "app-2",
          run_id: "run-102",
          tool_name: "execute_shell",
          arguments_json: "{}",
          risk_level: "critical",
          summary: "Execute sudo reboot",
          status: "pending",
          created_at: 1001,
          resolved_at: null,
          resolved_by: null,
        },
      ];

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "get_takeover_status") {
          return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        }
        if (cmd === "list_pending_approvals") {
          return Promise.resolve(mockApprovals);
        }
        if (cmd === "resolve_approval") {
          return Promise.resolve(null);
        }
        return Promise.resolve(null);
      });

      render(<HudPanel />);

      await act(async () => {
        vi.advanceTimersByTime(900);
      });

      expect(screen.getByText(/Delete database backup file/)).toBeInTheDocument();
      expect(screen.getByText(/Execute sudo reboot/)).toBeInTheDocument();

      const approveBtns = screen.getAllByTitle("Approve Action");
      await act(async () => {
        fireEvent.click(approveBtns[0]);
      });

      expect(invoke).toHaveBeenCalledWith("resolve_approval", {
        id: "app-1",
        decision: "approved",
        resolvedBy: "hud_user",
      });

      // Also test deny button for app-2
      const denyBtn = screen.getByTitle("Deny Action");
      await act(async () => {
        fireEvent.click(denyBtn);
      });

      expect(invoke).toHaveBeenCalledWith("resolve_approval", {
        id: "app-2",
        decision: "denied",
        resolvedBy: "hud_user",
      });
    });

    it("runs agent turn and handles aborting a running turn", async () => {
      let turnResolve: any;
      const turnPromise = new Promise((resolve) => {
        turnResolve = resolve;
      });

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "get_takeover_status") {
          return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        }
        if (cmd === "list_pending_approvals") {
          return Promise.resolve([]);
        }
        if (cmd === "run_agent_turn") {
          return turnPromise;
        }
        if (cmd === "cancel_agent_run") {
          return Promise.resolve(null);
        }
        if (cmd === "abort_computer_use") {
          return Promise.resolve(null);
        }
        return Promise.resolve(null);
      });

      render(<HudPanel />);

      const input = screen.getByPlaceholderText("Direct Computer Use action...");
      fireEvent.change(input, { target: { value: "Click settings button" } });

      const form = input.closest("form")!;
      await act(async () => {
        fireEvent.submit(form);
      });

      expect(screen.getByText("Running with Nemotron-3 Ultra...")).toBeInTheDocument();
      expect(screen.getByText("STOP")).toBeInTheDocument();

      // Click STOP
      const stopBtn = screen.getByText("STOP");
      await act(async () => {
        fireEvent.click(stopBtn);
      });

      expect(invoke).toHaveBeenCalledWith("abort_computer_use");
      expect(invoke).toHaveBeenCalledWith("cancel_agent_run", expect.objectContaining({
        runId: expect.stringMatching(/^run_/),
      }));

      // Resolve turn
      await act(async () => {
        turnResolve(null);
      });
    });

    it("handles error during run_agent_turn", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_takeover_status") return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        if (cmd === "list_pending_approvals") return Promise.resolve([]);
        if (cmd === "run_agent_turn") return Promise.reject(new Error("Nemotron API rate limited"));
        return Promise.resolve(null);
      });

      render(<HudPanel />);

      const input = screen.getByPlaceholderText("Direct Computer Use action...");
      fireEvent.change(input, { target: { value: "Trigger error" } });

      await act(async () => {
        fireEvent.submit(input.closest("form")!);
      });

      expect(screen.getByText(/Error: Error: Nemotron API rate limited/)).toBeInTheDocument();
    });

    it("triggers snipper overlay, opens main workspace, and closes HUD", async () => {
      (invoke as any).mockResolvedValue(null);

      render(<HudPanel />);

      // Snip region button
      const snipBtn = screen.getByText("Snip Region");
      await act(async () => {
        fireEvent.click(snipBtn);
      });
      expect(invoke).toHaveBeenCalledWith("toggle_overlay_window", { show: true });

      // Open main window button
      const openMainBtn = screen.getByTitle("Open Full Workspace");
      await act(async () => {
        fireEvent.click(openMainBtn);
      });
      expect(mockShow).toHaveBeenCalled();
      expect(mockSetFocus).toHaveBeenCalled();

      // Close HUD button
      const closeBtn = screen.getByTitle("Close HUD");
      await act(async () => {
        fireEvent.click(closeBtn);
      });
      expect(mockHide).toHaveBeenCalled();
    });
  });

  describe("Snipper", () => {
    it("handles mouse dragging and takes region screenshot when dimensions > 5px", async () => {
      (invoke as any).mockResolvedValue(null);

      const { container } = render(<Snipper />);
      const overlay = container.firstChild as HTMLElement;

      // Mouse down at (10, 20)
      fireEvent.mouseDown(overlay, { clientX: 10, clientY: 20 });
      // Mouse move to (100, 150)
      fireEvent.mouseMove(overlay, { clientX: 100, clientY: 150 });

      // Rectangle indicator should be displayed with size 90 x 130
      expect(screen.getByText("90 x 130")).toBeInTheDocument();

      // Mouse up
      await act(async () => {
        fireEvent.mouseUp(overlay);
      });

      expect(invoke).toHaveBeenCalledWith("take_region_screenshot", {
        x: 10,
        y: 20,
        width: 90,
        height: 130,
      });
      expect(invoke).toHaveBeenCalledWith("toggle_overlay_window", { show: false });
    });

    it("ignores region screenshot when dimensions <= 5px", async () => {
      (invoke as any).mockResolvedValue(null);

      const { container } = render(<Snipper />);
      const overlay = container.firstChild as HTMLElement;

      fireEvent.mouseDown(overlay, { clientX: 10, clientY: 20 });
      fireEvent.mouseMove(overlay, { clientX: 12, clientY: 22 }); // 2x2 px

      await act(async () => {
        fireEvent.mouseUp(overlay);
      });

      expect(invoke).not.toHaveBeenCalledWith("take_region_screenshot", expect.anything());
      expect(invoke).toHaveBeenCalledWith("toggle_overlay_window", { show: false });
    });

    it("closes overlay on Escape key", async () => {
      (invoke as any).mockResolvedValue(null);

      const { container } = render(<Snipper />);
      const overlay = container.firstChild as HTMLElement;

      await act(async () => {
        fireEvent.keyDown(overlay, { key: "Escape" });
      });

      expect(invoke).toHaveBeenCalledWith("toggle_overlay_window", { show: false });
    });
  });
});
