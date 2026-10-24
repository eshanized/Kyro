import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("Phase 1.1 Regression Tests: Provider-Driven Connector Authentication", () => {
  const mockCatalog = [
    {
      id: "google",
      name: "Google Workspace (Gmail & Calendar)",
      connector_type: "native" as const,
      description: "Access your Gmail emails, draft replies, check upcoming Google Calendar events.",
      icon: "google",
      auth_type: "oauth_browser",
      capabilities: ["gmail_list_messages", "calendar_list_events"],
      required_scopes: ["gmail.readonly", "calendar.readonly"],
      is_builtin: true,
      is_available: true,
      connected_accounts_count: 0,
      status: "available" as const,
    },
    {
      id: "github",
      name: "GitHub",
      connector_type: "native" as const,
      description: "Manage repositories, pull requests, issues",
      icon: "github",
      auth_type: "pat_or_oauth",
      capabilities: ["repositories", "pull_requests"],
      required_scopes: ["repo"],
      is_builtin: true,
      is_available: true,
      connected_accounts_count: 1,
      status: "available" as const,
    },
    {
      id: "remote_mcp",
      name: "Remote MCP Server",
      connector_type: "remote_mcp" as const,
      description: "Connect to SSE or HTTP remote model context protocol",
      icon: "remotemcp",
      auth_type: "bearer_token",
      capabilities: ["tools", "resources"],
      required_scopes: [],
      is_builtin: false,
      is_available: true,
      connected_accounts_count: 0,
      status: "available" as const,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string, args: any) => {
      if (cmd === "list_connector_catalog") return Promise.resolve(mockCatalog);
      if (cmd === "list_connected_accounts") return Promise.resolve([]);
      if (cmd === "list_mcp_servers") return Promise.resolve([]);
      if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
      if (cmd === "start_oauth_flow") {
        return Promise.resolve({
          session_id: "sess_google_123",
          auth_url: "https://accounts.google.com/o/oauth2/v2/auth?client_id=test",
          redirect_uri: "http://127.0.0.1:4567/callback",
          expires_at: Date.now() + 60000,
        });
      }
      if (cmd === "get_oauth_status") {
        return Promise.resolve({ status: "completed" });
      }
      if (cmd === "complete_oauth_flow") {
        return Promise.resolve({
          id: "conn_google_123",
          connector_id: "google",
          account_identifier: "user@workspace.com",
          display_name: "Google (Work)",
          status: "connected",
        });
      }
      if (cmd === "connect_account_token") {
        return Promise.resolve({
          id: "conn_pat_123",
          status: "connected",
        });
      }
      return Promise.resolve(null);
    });
  });

  it("renders human-readable authentication labels instead of raw 'oauth_browser'", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Google Workspace (Gmail & Calendar)")).toBeInTheDocument();
    });

    // Verify human-readable label is rendered
    expect(screen.getByText(/Browser OAuth 2.0/i)).toBeInTheDocument();
    // Raw internal enum value should not be displayed
    expect(screen.queryByText(/^oauth_browser$/)).not.toBeInTheDocument();
  });

  it("routes Google Workspace through browser OAuth lifecycle and calls start_oauth_flow with correct arguments", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Google Workspace (Gmail & Calendar)")).toBeInTheDocument();
    });

    // Find the Connect button for Google Workspace
    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[0]);

    // Modal should be open for Google Workspace
    await waitFor(() => {
      expect(screen.getByText(/Connect Google Workspace/i)).toBeInTheDocument();
    });

    // Should NOT ask for Personal Access Token or PAT inputs
    expect(screen.queryByPlaceholderText(/Secret access token or API key/i)).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/GitHub OAuth App Client Secret/i)).not.toBeInTheDocument();

    // Should display Google OAuth PKCE information
    expect(screen.getByText(/Google Workspace OAuth 2.0 PKCE/i)).toBeInTheDocument();
    const launchButton = screen.getByRole("button", { name: /Launch.*Sign-In/i });
    expect(launchButton).toBeInTheDocument();

    // Trigger OAuth launch
    fireEvent.click(launchButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_oauth_flow", {
        connectorId: "google",
        clientId: null,
        clientSecret: null,
      });
    });

    // Should enter browser waiting state with callback URI
    await waitFor(() => {
      expect(screen.getByText(/Waiting for Browser Sign-In/i)).toBeInTheDocument();
      expect(screen.getByText(/http:\/\/127\.0\.0\.1:4567\/callback/i)).toBeInTheDocument();
    });

    // Polling triggers completion and displays provider-neutral success message
    await waitFor(
      () => {
        expect(screen.getByText(/Successfully connected account: Google \(Work\)!/i)).toBeInTheDocument();
      },
      { timeout: 3500 }
    );
  });

  it("handles GitHub OAuth with Client ID and Secret and passes correct Tauri arguments", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("GitHub")).toBeInTheDocument();
    });

    // Click Connect for GitHub (second card)
    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[1]);

    await waitFor(() => {
      expect(screen.getByText(/Connect GitHub/i)).toBeInTheDocument();
    });

    // Has dual-mode switcher
    expect(screen.getByRole("button", { name: /OAuth 2\.0 \(Browser\)/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Personal Access Token/i })).toBeInTheDocument();

    // Fill in OAuth credentials
    const clientIdInput = screen.getByPlaceholderText(/GitHub OAuth App Client ID/i);
    const clientSecretInput = screen.getByPlaceholderText(/GitHub OAuth App Client Secret/i);

    fireEvent.change(clientIdInput, { target: { value: "gh_client_123" } });
    fireEvent.change(clientSecretInput, { target: { value: "gh_secret_456" } });

    const launchButton = screen.getByRole("button", { name: /Launch GitHub Sign-In/i });
    fireEvent.click(launchButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_oauth_flow", {
        connectorId: "github",
        clientId: "gh_client_123",
        clientSecret: "gh_secret_456",
      });
    });
  });

  it("handles GitHub PAT flow through credential form", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("GitHub")).toBeInTheDocument();
    });

    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[1]);

    await waitFor(() => {
      expect(screen.getByText(/Connect GitHub/i)).toBeInTheDocument();
    });

    // Switch to PAT tab
    const patTabButton = screen.getByRole("button", { name: /Personal Access Token/i });
    fireEvent.click(patTabButton);

    // Form inputs should be visible
    const tokenInput = screen.getByPlaceholderText("ghp_...");
    fireEvent.change(tokenInput, { target: { value: "ghp_secret_token_abc" } });

    const saveButton = screen.getByRole("button", { name: /Save & Connect/i });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("connect_account_token", expect.objectContaining({
        connectorId: "github",
        tokenSecret: "ghp_secret_token_abc",
      }));
    });
  });

  it("handles Remote MCP form with server URL, private network flag, and optional bearer token", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Remote MCP Server")).toBeInTheDocument();
    });

    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[2]);

    await waitFor(() => {
      expect(screen.getByText(/Connect Remote MCP Server/i)).toBeInTheDocument();
    });

    const urlInput = screen.getByPlaceholderText(/http:\/\/127\.0\.0\.1:8000\/mcp/i);
    const privateNetCheckbox = screen.getByLabelText(/Allow private \/ local intranet network hosts/i);
    const tokenInput = screen.getByPlaceholderText(/Secret bearer token/i);

    fireEvent.change(urlInput, { target: { value: "https://mcp.internal.corp/sse" } });
    fireEvent.click(privateNetCheckbox);
    fireEvent.change(tokenInput, { target: { value: "mcp_bearer_token_xyz" } });

    const saveButton = screen.getByRole("button", { name: /Save & Connect/i });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("connect_account_token", {
        connectorId: "remote_mcp",
        displayName: "Remote MCP Server Account",
        accountIdentifier: "default",
        tokenSecret: "mcp_bearer_token_xyz",
        config: {
          server_url: "https://mcp.internal.corp/sse",
          allow_private_network: true,
        },
      });
    });
  });
});
