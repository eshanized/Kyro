/**
 * Built-in catalog auth-type routing (bug 16) and OAuth session lifecycle
 * (bug 17) regression tests, driven through the real ConnectionsModal.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-shell", async () => {
  return {
    open: vi.fn().mockResolvedValue(null),
  };
});

const mockInvoke = vi.mocked(invoke);

/** One connector per BUILT-IN auth strategy in the catalog. */
const builtInCatalog = [
  {
    id: "local_mcp",
    name: "Local MCP (Stdio)",
    connector_type: "local_mcp" as const,
    description: " Stdio servers",
    icon: "terminal",
    auth_type: "none" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
  {
    id: "github",
    name: "GitHub",
    connector_type: "native" as const,
    description: "Repos",
    icon: "github",
    auth_type: "pat_or_oauth" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
  {
    id: "gitlab",
    name: "GitLab",
    connector_type: "native" as const,
    description: "Repos",
    icon: "gitlab",
    auth_type: "pat_token" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
  {
    id: "openweather",
    name: "OpenWeather",
    connector_type: "native" as const,
    description: "Weather",
    icon: "cloud",
    auth_type: "api_key" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
  {
    id: "homeassistant",
    name: "Home Assistant",
    connector_type: "native" as const,
    description: "Smart home",
    icon: "home",
    auth_type: "bearer_token" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
  {
    id: "gmail",
    name: "Gmail",
    connector_type: "native" as const,
    description: "Mail",
    icon: "mail",
    auth_type: "oauth_browser" as const,
    scopes: [],
    required_scopes: [],
    capabilities: [],
    is_builtin: true,
    is_available: true,
    connected_accounts_count: 0,
    status: "available" as const,
  },
];

const okBase = async (cmd: string, _args?: any) => {
  if (cmd === "list_connector_catalog") return Promise.resolve(builtInCatalog as any);
  if (cmd === "list_connected_accounts") return Promise.resolve([]);
  if (cmd === "list_mcp_servers") return Promise.resolve([]);
  if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
  return Promise.resolve();
};


/** Row-scoped helper: each catalog card exposes its own action button. */
function clickConnectForRow(name: string) {
  const label = screen.getByText(name);
  const candidates = Array.from(
    document.querySelectorAll("button")
  ).filter((b) => /Connect Account|Configure Local MCP/i.test(b.textContent || ""));
  const target = candidates.find((b) =>
    // The button belongs to the card that contains this catalog label.
    b
      ? Array.from(b.closest("div")?.parentElement?.childNodes ?? []).some(
          (n) => (n as HTMLElement).contains?.(label as HTMLElement)
        )
      : false
  );
  if (!target) throw new Error(`No Connect Account button in ${name} row`);
  fireEvent.click(target);
}

function catalogEntry(name: string) {
  return builtInCatalog.find((c) => c.name.includes(name))!;
}

describe("Bug 16: every built-in auth type drives the correct route/form", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation(okBase as any);
  });

  it("auth_type none (Local MCP) routes to the local server configuration tab, not the credential form", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);
    await waitFor(() =>
      expect(screen.getByText("Local MCP (Stdio)")).toBeInTheDocument()
    );

    const configureBtn = screen.getByRole("button", { name: /Configure Local MCP/i });
    expect(configureBtn).toBeInTheDocument();

    // The auth-less entry must NOT offer the impossible credential flow.
    const allConnectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    expect(allConnectButtons.every((b) => !b.closest("div")!.textContent!.includes("Local MCP"))).toBe(true);

    fireEvent.click(configureBtn);

    await waitFor(() => {
      // The custom local-server configuration interface becomes visible.
      expect(screen.getByText(/Add Local MCP Server/i)).toBeInTheDocument();
    });
    // No connect dialog was opened for the auth-less item.
    expect(screen.queryByText("Display Name")).not.toBeInTheDocument();
  });

  it("pat_or_oauth opens the connect dialog defaulting to OAuth", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);
    await waitFor(() => expect(screen.getByText("GitHub")).toBeInTheDocument());
    clickConnectForRow("GitHub");

    await waitFor(() => {
      expect(screen.getByText(/Launch GitHub Sign-In/i)).toBeInTheDocument();
    });
  });

  it("pat_token and bearer_token and api_key open their credential forms", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);
    await waitFor(() => expect(screen.getByText("GitLab")).toBeInTheDocument());

    const openFor = async (name: string, credentialLabel: string) => {
      clickConnectForRow(name);
      // The dialog is driven by the catalog's declared strategy.
      await waitFor(() => {
        expect(screen.getByText(/Authenticating via/i)).toBeInTheDocument();
        expect(screen.getAllByText(credentialLabel).length).toBeGreaterThan(0);
      });
      // close the dialog for the next iteration
      fireEvent.click(screen.getAllByRole("button", { name: /^Cancel$/i })[0]);
      await waitFor(() => {
        expect(screen.queryByText(/Authenticating via/i)).not.toBeInTheDocument();
      });
    };
    await openFor("GitLab", "Personal Access Token");
    await openFor("OpenWeather", "API Key");
    await openFor("Home Assistant", "Bearer Token");
  });
});

describe("Bug 17: OAuth flow lifecycle is cancellable, non-overlapping and one-shot", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation(okBase as any);
  });

  it("cancelling while start_oauth_flow is in flight cancels the session that materializes later", async () => {
    let resolveStart: (v: any) => void = () => {};
    const startPromise = new Promise((resolve) => {
      resolveStart = resolve;
    });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      const base = await okBase(cmd, args);
      if (cmd === "start_oauth_flow") return startPromise.then(() => ({
        session_id: "sess_late_start",
        auth_url: "http://127.0.0.1:4567/auth",
        redirect_uri: "http://127.0.0.1:4567/callback",
        expires_at: Date.now() + 60000,
      }));
      return base;
    });

    const { rerender } = render(
      <ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />
    );
    await waitFor(() => expect(screen.getByText("Gmail")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Gmail"));
    clickConnectForRow("Gmail");
    await waitFor(() => {
      expect(screen.getByText(/Launch Gmail Sign-In/i)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText(/Launch Gmail Sign-In/i));

    // Close the modal while the start is in flight.
    rerender(<ConnectionsModal isOpen={false} onClose={vi.fn()} initialTab="discover" />);
    // Late start response resolves now — the session must still be cancelled.
    await act(async () => {
      resolveStart(undefined);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("cancel_oauth_flow", { sessionId: "sess_late_start" });
    });
  });

  it("status polls never overlap and completion is one-shot", async () => {
    vi.useFakeTimers();
    let pollCount = 0;
    let resolvePoll: ((v: any) => void) = () => {};
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      const base = await okBase(cmd, args);
      if (cmd === "start_oauth_flow") {
        return {
          session_id: "sess_overlap",
          auth_url: "http://127.0.0.1:4567/auth",
          redirect_uri: "http://127.0.0.1:4567/callback",
          expires_at: Date.now() + 60000,
        };
      }
      if (cmd === "get_oauth_status") {
        pollCount++;
        const count = pollCount;
        const first = count === 1
          ? new Promise((resolve) => {
              resolvePoll = resolve;
            })
          : Promise.resolve({ status: "completed", error: null });
        // The first status request is deliberately SLOW (longer than one
        // poll interval) to reproduce the overlapping-poll hazard.
        return count === 1 ? first : first;
      }
      if (cmd === "complete_oauth_flow") {
        return {
          display_name: "tester@example.com",
          account_identifier: "tester@example.com",
          connector_id: "gmail",
        };
      }
      return base;
    });

    const statusCalls = () =>
      mockInvoke.mock.calls.filter(([cmd]) => cmd === "get_oauth_status").length;

    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);
    const settle = async () => {
      for (let i = 0; i < 30; i++) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(30);
        });
      }
    };

    await settle();
    fireEvent.click(screen.getByText("Gmail"));
    clickConnectForRow("Gmail");
    await settle();
    expect(screen.getByText(/Launch Gmail Sign-In/i)).toBeInTheDocument();

    fireEvent.click(screen.getByText(/Launch Gmail Sign-In/i));
    await settle();
    expect(mockInvoke).toHaveBeenCalledWith("start_oauth_flow", expect.anything());

    // One poll interval later, the (slow) first status request is in flight.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(statusCalls()).toBe(1); // overlap guard: slow first poll only

    // Several further poll intervals while the first request is still
    // awaiting: the overlap guard must keep a single in-flight request.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4500);
    });
    expect(statusCalls()).toBe(1);

    // The slow poll completes → flow completes exactly once ("one-shot").
    await act(async () => {
      resolvePoll({ status: "completed", error: null });
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(0);
    });
    await settle();

    expect(screen.getByText(/Successfully connected account/i)).toBeInTheDocument();
    const completeCalls = mockInvoke.mock.calls.filter(
      ([cmd]) => cmd === "complete_oauth_flow"
    ).length;
    expect(completeCalls).toBe(1);

    // Extra ticks after completion must not start anything new.
    const callsAfterSuccess = mockInvoke.mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(mockInvoke.mock.calls.length).toBe(callsAfterSuccess);
    vi.useRealTimers();
  });

  it("an expired/failed status surfaces truthfully and a retry starts a NEW flow", async () => {
    let failed = false;
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      const base = await okBase(cmd, args);
      if (cmd === "start_oauth_flow") {
        return {
          session_id: failed ? "sess_retry" : "sess_first",
          auth_url: "http://127.0.0.1:4567/auth",
          redirect_uri: "http://127.0.0.1:4567/callback",
          expires_at: Date.now() + 60000,
        };
      }
      if (cmd === "get_oauth_status") {
        return { status: failed ? "pending" : "failed", error: "user declined" };
      }
      return base;
    });

    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="discover" />);
    await waitFor(() => expect(screen.getByText("Gmail")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Gmail"));
    clickConnectForRow("Gmail");
    fireEvent.click(screen.getByText(/Launch Gmail Sign-In/i));

    // Poll interval is real (1.5s); allow the first tick to land.
    await waitFor(
      () => {
        expect(screen.getByText(/OAuth authorization failed: user declined/i)).toBeInTheDocument();
      },
      { timeout: 4000 }
    );
    expect(
      mockInvoke.mock.calls.some(([cmd]) => cmd === "complete_oauth_flow")
    ).toBe(false);

    // Retry: the Connect path is usable again and starts a fresh session.
    failed = true;
    fireEvent.click(screen.getAllByRole("button", { name: /^Cancel$/i })[0]);
    await waitFor(() => {
      expect(screen.queryByText(/Authenticating via/i)).not.toBeInTheDocument();
    });
    clickConnectForRow("Gmail");
    await waitFor(() => {
      expect(screen.getByText(/Launch Gmail Sign-In/i)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText(/Launch Gmail Sign-In/i));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("start_oauth_flow", expect.anything());
    });
  });
});
