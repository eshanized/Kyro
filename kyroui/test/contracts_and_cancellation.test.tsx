import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { TerminalView } from "../components/TerminalView";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/plugin-shell", () => ({
  open: vi.fn(() => Promise.resolve()),
}));

let mockOnDataCallback: ((data: string) => void) | null = null;
let mockWriteln = vi.fn();

vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class MockTerminal {
      rows = 24;
      cols = 80;
      open = vi.fn();
      loadAddon = vi.fn();
      writeln = mockWriteln;
      write = vi.fn();
      dispose = vi.fn();
      onData = vi.fn((cb: any) => {
        mockOnDataCallback = cb;
        return { dispose: vi.fn() };
      });
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

describe("Phase 2 — Command Contract Verification", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockOnDataCallback = null;
    window.open = vi.fn();
  });

  describe("Issue A: TerminalView write_terminal_input contract", () => {
    it("invokes write_terminal_input with { input: data } payload and never uses legacy write_terminal", async () => {
      (invoke as any).mockResolvedValue(undefined);

      render(<TerminalView />);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("spawn_terminal");
      });

      expect(mockOnDataCallback).not.toBeNull();

      // Trigger user typing in terminal
      mockOnDataCallback!("ls -la\n");

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("write_terminal_input", { input: "ls -la\n" });
      });

      // Ensure deprecated/incorrect command name is never called
      expect(invoke).not.toHaveBeenCalledWith("write_terminal", expect.anything());
    });

    it("surfaces write_terminal_input failure visually in terminal output via writeln", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "spawn_terminal") return Promise.resolve();
        if (cmd === "write_terminal_input") return Promise.reject("PTY write broken pipe");
        return Promise.resolve();
      });

      render(<TerminalView />);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("spawn_terminal");
      });

      expect(mockOnDataCallback).not.toBeNull();
      mockOnDataCallback!("echo test\n");

      await waitFor(() => {
        expect(mockWriteln).toHaveBeenCalledWith(
          expect.stringContaining("Failed to write terminal input: PTY write broken pipe")
        );
      });
    });
  });

  describe("Issue B: OAuth Cancellation Contract", () => {
    const mockCatalog = [
      {
        id: "github",
        name: "GitHub",
        connector_type: "native" as const,
        auth_type: "pat_or_oauth" as const,
        description: "Pull requests, issues, repo management",
        icon: "github",
        status: "available" as const,
        is_first_party: true,
        is_available: true,
      },
    ];

    it("invokes cancel_oauth_flow with { sessionId } and handles success cleanly", async () => {
      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "list_connector_catalog") return Promise.resolve(mockCatalog);
        if (cmd === "list_connected_accounts") return Promise.resolve([]);
        if (cmd === "list_mcp_servers") return Promise.resolve([]);
        if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
        if (cmd === "start_oauth_flow") {
          return Promise.resolve({
            session_id: "oauth_sess_test_123",
            auth_url: "http://127.0.0.1:4567/auth",
            redirect_uri: "http://127.0.0.1:4567/callback",
            expires_at: Date.now() + 60000,
          });
        }
        if (cmd === "get_oauth_status") {
          return Promise.resolve({ status: "pending", error: null });
        }
        if (cmd === "cancel_oauth_flow") {
          return Promise.resolve();
        }
        return Promise.resolve();
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);

      await waitFor(() => {
        expect(screen.getByText("Connect Account")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("Connect Account"));

      // Click "Launch GitHub Sign-In"
      await waitFor(() => {
        expect(screen.getByText(/Launch GitHub Sign-In/i)).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText(/Launch GitHub Sign-In/i));

      // Modal should show OAuth in progress and the Cancel button
      await waitFor(() => {
        expect(screen.getByText(/Waiting for Browser Sign-In/i)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /^Cancel$/i })).toBeInTheDocument();
      });

      // Click cancel
      fireEvent.click(screen.getByRole("button", { name: /^Cancel$/i }));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("cancel_oauth_flow", { sessionId: "oauth_sess_test_123" });
      });

      // User notification displayed
      await waitFor(() => {
        expect(screen.getByText(/OAuth authorization cancelled\./i)).toBeInTheDocument();
      });
    });

    it("displays error and preserves state when cancel_oauth_flow rejects", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_connector_catalog") return Promise.resolve(mockCatalog);
        if (cmd === "list_connected_accounts") return Promise.resolve([]);
        if (cmd === "list_mcp_servers") return Promise.resolve([]);
        if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
        if (cmd === "start_oauth_flow") {
          return Promise.resolve({
            session_id: "oauth_sess_test_456",
            auth_url: "http://127.0.0.1:4567/auth",
            redirect_uri: "http://127.0.0.1:4567/callback",
            expires_at: Date.now() + 60000,
          });
        }
        if (cmd === "get_oauth_status") {
          return Promise.resolve({ status: "pending", error: null });
        }
        if (cmd === "cancel_oauth_flow") {
          return Promise.reject("Cannot cancel OAuth session: authorization already completed");
        }
        return Promise.resolve();
      });

      render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);

      await waitFor(() => {
        expect(screen.getByText("Connect Account")).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText("Connect Account"));

      await waitFor(() => {
        expect(screen.getByText(/Launch GitHub Sign-In/i)).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText(/Launch GitHub Sign-In/i));

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /^Cancel$/i })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: /^Cancel$/i }));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("cancel_oauth_flow", { sessionId: "oauth_sess_test_456" });
      });

      // Error surfaced
      await waitFor(() => {
        expect(
          screen.getByText(/Failed to cancel OAuth flow: Cannot cancel OAuth session: authorization already completed/i)
        ).toBeInTheDocument();
      });
    });
  });
});
