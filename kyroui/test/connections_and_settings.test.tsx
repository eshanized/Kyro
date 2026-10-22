import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { SettingsModal } from "../components/SettingsModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("ConnectionsModal Integration Tests", () => {
  const mockCatalog = [
    {
      id: "github",
      name: "GitHub",
      connector_type: "native" as const,
      description: "Manage repositories, pull requests, issues",
      category: "native",
      auth_type: "pat_or_oauth",
      scopes: ["repo", "read:user"],
      required_scopes: ["repo"],
      capabilities: ["repositories", "pull_requests", "issues"],
      is_builtin: true,
      is_available: true,
      connected_accounts_count: 1,
      status: "available" as const,
      icon: "github",
    },
    {
      id: "remote_mcp",
      name: "Remote MCP Server",
      connector_type: "remote_mcp" as const,
      description: "Connect to SSE or HTTP remote model context protocol",
      category: "mcp",
      auth_type: "token",
      scopes: [],
      required_scopes: [],
      capabilities: ["tools", "resources"],
      is_builtin: false,
      is_available: true,
      connected_accounts_count: 0,
      status: "available" as const,
      icon: "remotemcp",
    },
  ];

  const mockConnectedAccounts = [
    {
      id: "acc_gh_1",
      connector_id: "github",
      connector_name: "GitHub",
      account_identifier: "octocat",
      display_name: "Octocat Main",
      connected_at: 1000,
      last_used: 1050,
      status: "active",
      tool_count: 5,
    },
  ];

  const mockMcpServers = [
    {
      id: "mcp_srv_1",
      name: "Filesystem MCP",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      env: { SEC_TOKEN: { value: "secret123", is_secret: true } },
      created_at: 1000,
    },
  ];

  const mockPolicies = [
    {
      account_id: "acc_gh_1",
      tool_name: "create_pull_request",
      policy: "ask",
      risk_level: "high",
    },
  ];

  const mockTools = [
    {
      name: "create_pull_request",
      original_name: "create_pull_request",
      description: "Creates a pull request on GitHub",
      risk_level: "safe",
      policy: "ask",
      is_read_only: false,
      can_auto_approve: true,
      default_policy: "ask",
    },
    {
      name: "list_issues",
      original_name: "list_issues",
      description: "Lists repository issues",
      risk_level: "safe",
      policy: "allow",
      is_read_only: true,
      can_auto_approve: true,
      default_policy: "allow",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string, args: any) => {
      if (cmd === "list_connector_catalog") return Promise.resolve(mockCatalog);
      if (cmd === "list_connected_accounts") return Promise.resolve(mockConnectedAccounts);
      if (cmd === "list_mcp_servers") return Promise.resolve(mockMcpServers);
      if (cmd === "get_mcp_server_statuses") return Promise.resolve({ mcp_srv_1: "running" });
      if (cmd === "list_account_tool_policies") return Promise.resolve(mockPolicies);
      if (cmd === "get_account_tools") return Promise.resolve(mockTools);
      if (cmd === "connect_account_token") return Promise.resolve(true);
      if (cmd === "start_oauth_flow") {
        return Promise.resolve({
          session_id: "oauth_sess_test",
          auth_url: "https://github.com/login/oauth/authorize?client_id=xyz",
          redirect_uri: "http://localhost:1420/oauth/callback",
          expires_at: 9999999999,
        });
      }
      if (cmd === "get_oauth_status") {
        return Promise.resolve({ status: "completed" });
      }
      if (cmd === "complete_oauth_flow") {
        return Promise.resolve({
          id: "acc_gh_new",
          account_identifier: "newuser",
          display_name: "GitHub Account",
        });
      }
      if (cmd === "disconnect_account") return Promise.resolve(true);
      if (cmd === "test_connector_connection") return Promise.resolve("OK - Authenticated as @octocat");
      if (cmd === "save_account_tool_policy") return Promise.resolve(true);
      if (cmd === "create_mcp_server") return Promise.resolve(true);
      if (cmd === "delete_mcp_server") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("renders catalog and switches between tabs", async () => {
    const { rerender } = render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("GitHub")).toBeInTheDocument();
    });

    expect(screen.getByText("Remote MCP Server")).toBeInTheDocument();

    // Filter by search query
    const searchInput = screen.getByPlaceholderText(/Search integrations/i);
    fireEvent.change(searchInput, { target: { value: "Remote" } });
    expect(screen.queryByText("Manage repositories, pull requests, issues")).not.toBeInTheDocument();
    expect(screen.getByText("Remote MCP Server")).toBeInTheDocument();

    // Clear search
    fireEvent.change(searchInput, { target: { value: "" } });

    // Switch to Connected Accounts tab
    const connectedTab = screen.getByRole("button", { name: /Connected Accounts/i });
    fireEvent.click(connectedTab);

    await waitFor(() => {
      expect(screen.getByText("Octocat Main")).toBeInTheDocument();
    });

    // Switch to Custom MCP Servers tab
    const mcpTab = screen.getByRole("button", { name: /Local MCP Servers/i });
    fireEvent.click(mcpTab);

    await waitFor(() => {
      expect(screen.getByText("Filesystem MCP")).toBeInTheDocument();
    });
  });

  it("handles Connect flow with personal access token", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("GitHub")).toBeInTheDocument();
    });

    // Click Connect Account on GitHub card
    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[0]);

    // Choose Personal Access Token method
    const patTab = await screen.findByRole("button", { name: /Personal Access Token/i });
    fireEvent.click(patTab);

    // Fill in token and identifier
    const tokenInput = screen.getByPlaceholderText("ghp_...");
    fireEvent.change(tokenInput, { target: { value: "ghp_validaccesstoken123456789" } });

    const idInput = screen.getByPlaceholderText(/e\.g\. your GitHub username/i);
    fireEvent.change(idInput, { target: { value: "octocat_work" } });

    // Submit form
    const saveBtn = screen.getByRole("button", { name: /Save & Connect/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "connect_account_token",
        expect.objectContaining({
          connectorId: "github",
          accountIdentifier: "octocat_work",
          tokenSecret: "ghp_validaccesstoken123456789",
        })
      );
    });
  });

  it("initiates OAuth flow and handles polling completion", async () => {
    vi.stubGlobal("open", vi.fn());

    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("GitHub")).toBeInTheDocument();
    });

    const connectButtons = screen.getAllByRole("button", { name: /Connect Account/i });
    fireEvent.click(connectButtons[0]);

    // Click Launch GitHub Sign-In
    const oauthBtn = await screen.findByRole("button", { name: /Launch GitHub Sign-In/i });
    fireEvent.click(oauthBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "start_oauth_flow",
        expect.objectContaining({
          connectorId: "github",
        })
      );
    });

    // Verify polling triggers complete_oauth_flow
    await waitFor(
      () => {
        expect(invoke).toHaveBeenCalledWith(
          "complete_oauth_flow",
          expect.objectContaining({
            sessionId: "oauth_sess_test",
          })
        );
      },
      { timeout: 3500 }
    );
  });

  it("tests connection and manages tool policies for connected account", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="connected" />);

    await waitFor(() => {
      expect(screen.getByText("Octocat Main")).toBeInTheDocument();
    });

    // Click Test Connection
    const testBtn = screen.getByRole("button", { name: /Test/i });
    fireEvent.click(testBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("test_connector_connection", { accountId: "acc_gh_1" });
    });

    // Click Policies
    const policiesBtn = screen.getByRole("button", { name: /Policies/i });
    fireEvent.click(policiesBtn);

    await waitFor(() => {
      expect(screen.getByText("create_pull_request")).toBeInTheDocument();
    });

    // Toggle policy button to Allow
    const allowBtns = screen.getAllByRole("button", { name: "Allow" });
    fireEvent.click(allowBtns[0]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_account_tool_policy",
        expect.objectContaining({
          accountId: "acc_gh_1",
          toolName: "create_pull_request",
          policy: "allow",
        })
      );
    });
  });

  it("handles disconnecting an account", async () => {
    vi.stubGlobal("confirm", () => true);

    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="connected" />);

    await waitFor(() => {
      expect(screen.getByText("Octocat Main")).toBeInTheDocument();
    });

    const disconnectBtn = screen.getByRole("button", { name: /Disconnect/i });
    fireEvent.click(disconnectBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("disconnect_account", { accountId: "acc_gh_1" });
    });
  });

  it("adds a new custom local MCP server", async () => {
    render(<ConnectionsModal isOpen={true} onClose={vi.fn()} initialTab="custom_mcp" />);

    await waitFor(() => {
      expect(screen.getByText("Filesystem MCP")).toBeInTheDocument();
    });

    const addServerBtn = screen.getByRole("button", { name: /Add Local MCP Server/i });
    fireEvent.click(addServerBtn);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Local Filesystem Server/i);
    fireEvent.change(nameInput, { target: { value: "Postgres Server" } });

    const cmdInput = screen.getByPlaceholderText(/npx or python3/i);
    fireEvent.change(cmdInput, { target: { value: "docker" } });

    const argsInput = screen.getByPlaceholderText(/-y @modelcontextprotocol\/server-filesystem/i);
    fireEvent.change(argsInput, { target: { value: "run -i postgres-mcp" } });

    const saveMcpBtn = screen.getByRole("button", { name: /Save & Register Local Server/i });
    fireEvent.click(saveMcpBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_mcp_server",
        expect.objectContaining({
          server: expect.objectContaining({
            name: "Postgres Server",
            command: "docker",
          }),
        })
      );
    });
  });
});

describe("SettingsModal Integration Tests", () => {
  const mockApiKeys = {
    nvidia_configured: true,
    telegram_configured: true,
    telegram_chat_id: "12345678",
  };

  const mockGovernancePolicy = {
    shell_mode: "SafeAutoApprove",
    computer_use_mode: "FullAutonomous",
    require_takeover_for_credentials: true,
    blocked_domains: ["chase.com", "bankofamerica.com", "paypal.com"],
  };

  const mockBrowserCaps = {
    installed_browsers: ["chrome", "firefox"],
    default_browser: "chrome",
    cdp_available: true,
  };

  const mockBrowserSettings = {
    headless: false,
    stealth_mode: true,
    user_data_dir: "/home/user/.config/kyro/browser",
  };

  const mockFolders = [
    {
      id: "folder_1",
      path: "/home/user/Documents",
      display_name: "Documents",
      allowed_at: 1000,
    },
  ];

  const mockHealth = {
    rust_version: "1.82.0",
    sqlite_version: "3.45.0",
    database_size_bytes: 1048576,
    active_connections: 1,
    os_name: "linux",
    tools: { git: true, bwrap: true, chrome: true },
  };

  const mockStarters = [
    {
      id: "tpl_pack_1",
      name: "Engineering Productivity Pack",
      description: "Agents and tools for software teams",
      version: "1.0",
      bots: [],
      skills: [],
      routines: [],
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();

    (invoke as any).mockImplementation((cmd: string, args: any) => {
      if (cmd === "get_api_keys") return Promise.resolve(mockApiKeys);
      if (cmd === "get_governance_policy") return Promise.resolve(mockGovernancePolicy);
      if (cmd === "get_sandbox_status") return Promise.resolve({ supported: true, active: true });
      if (cmd === "get_browser_capabilities") return Promise.resolve(mockBrowserCaps);
      if (cmd === "get_browser_settings") return Promise.resolve(mockBrowserSettings);
      if (cmd === "list_authorized_folders") return Promise.resolve(mockFolders);
      if (cmd === "get_system_health") return Promise.resolve(mockHealth);
      if (cmd === "get_starter_templates") return Promise.resolve(mockStarters);
      if (cmd === "save_api_keys") return Promise.resolve(true);
      if (cmd === "save_governance_policy") return Promise.resolve(true);
      if (cmd === "save_browser_settings") return Promise.resolve(true);
      if (cmd === "add_authorized_folder") return Promise.resolve(true);
      if (cmd === "remove_authorized_folder") return Promise.resolve(true);
      if (cmd === "delete_secret_key") return Promise.resolve(true);
      if (cmd === "apply_template_pack") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("renders SettingsModal and navigates through all configuration tabs", async () => {
    render(
      <SettingsModal
        isOpen={true}
        onClose={vi.fn()}
        selectedModel="nvidia/nemotron-3-ultra-550b-a55b"
        onSelectModel={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Kyro System Settings & Security Matrix")).toBeInTheDocument();
    });

    // 1. Model & Credentials Tab is default
    expect(screen.getByText(/NVIDIA NIM API Key/i)).toBeInTheDocument();

    // 2. Privacy & Folders Tab
    const privacyTab = screen.getByRole("button", { name: /Privacy & Folders/i });
    fireEvent.click(privacyTab);

    await waitFor(() => {
      expect(screen.getByText("/home/user/Documents")).toBeInTheDocument();
    });

    // 3. Governance Matrix Tab
    const govTab = screen.getByRole("button", { name: /Governance Matrix/i });
    fireEvent.click(govTab);

    await waitFor(() => {
      expect(screen.getByDisplayValue(/chase\.com/)).toBeInTheDocument();
    });

    // 4. Browser Automation Tab
    const browserTab = screen.getByRole("button", { name: /Browser Automation/i });
    fireEvent.click(browserTab);

    await waitFor(() => {
      expect(screen.getByText(/Browser Automation & Capability Matrix/i)).toBeInTheDocument();
    });

    // 5. Telegram Bridge Tab
    const telegramTab = screen.getByRole("button", { name: /Telegram Bridge/i });
    fireEvent.click(telegramTab);

    await waitFor(() => {
      expect(screen.getByText(/Telegram Remote Notification Bridge/i)).toBeInTheDocument();
    });

    // 6. Container Sandbox Tab
    const sandboxTab = screen.getByRole("button", { name: /Container Sandbox/i });
    fireEvent.click(sandboxTab);

    await waitFor(() => {
      expect(screen.getByText(/Local Container Engine Status/i)).toBeInTheDocument();
    });

    // 7. Starter Templates Tab
    const templatesTab = screen.getByRole("button", { name: /Templates/i });
    fireEvent.click(templatesTab);

    await waitFor(() => {
      expect(screen.getByText("Engineering Productivity Pack")).toBeInTheDocument();
    });

    // 8. Diagnostics Tab
    const diagTab = screen.getByRole("button", { name: /Diagnostics/i });
    fireEvent.click(diagTab);

    await waitFor(() => {
      expect(screen.getByText(/Real-Time Local Runtime Diagnostics/i)).toBeInTheDocument();
    });
  });

  it("updates and saves API keys and model configuration", async () => {
    const handleSelectModel = vi.fn();

    render(
      <SettingsModal
        isOpen={true}
        onClose={vi.fn()}
        selectedModel="nvidia/nemotron-3-ultra-550b-a55b"
        onSelectModel={handleSelectModel}
      />
    );

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Configured in Vault/i)).toBeInTheDocument();
    });

    const keyInput = screen.getByPlaceholderText(/Configured in Vault/i);
    fireEvent.change(keyInput, { target: { value: "nvapi-newsecretkey12345" } });

    const saveBtn = screen.getByRole("button", { name: /Save Settings/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_api_keys",
        expect.objectContaining({
          keys: expect.objectContaining({
            nvidia: "nvapi-newsecretkey12345",
          }),
        })
      );
    });
  });

  it("adds and removes blocked domains in governance matrix", async () => {
    render(
      <SettingsModal
        isOpen={true}
        onClose={vi.fn()}
        selectedModel="nvidia/nemotron-3-ultra-550b-a55b"
        onSelectModel={vi.fn()}
        initialTab="governance"
      />
    );

    await waitFor(() => {
      expect(screen.getByDisplayValue(/chase\.com/)).toBeInTheDocument();
    });

    const domainInput = screen.getByDisplayValue(/chase\.com/);
    fireEvent.change(domainInput, { target: { value: "chase.com, wellsfargo.com" } });

    // Save governance
    const saveBtn = screen.getByRole("button", { name: /Save Settings/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_governance_policy",
        expect.objectContaining({
          policy: expect.objectContaining({
            blocked_domains: expect.arrayContaining(["wellsfargo.com"]),
          }),
        })
      );
    });
  });

  it("removes authorized folder", async () => {
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
      expect(screen.getByText("/home/user/Documents")).toBeInTheDocument();
    });

    const revokeBtn = screen.getByTitle("Revoke Folder Authorization");
    fireEvent.click(revokeBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "remove_authorized_folder",
        expect.objectContaining({
          id: "folder_1",
          path: "/home/user/Documents",
        })
      );
    });
  });
});
