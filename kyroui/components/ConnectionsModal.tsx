import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ConnectorCatalogItem,
  ConnectedAccountSummary,
  ConnectorToolPolicyRecord,
  McpServerRecord,
  AccountToolView,
  OAuthSessionStatusView,
} from "../types/agent";
import {
  Plug,
  Search,
  CheckCircle,
  AlertCircle,
  ExternalLink,
  Trash2,
  RefreshCw,
  Plus,
  Server,
  Lock,
  Terminal,
  Info,
  Sliders,
  X,
  Shield,
  Globe,
  Radio,
} from "lucide-react";
import { ServiceIcon } from "./BrandIcons";

interface ConnectionsModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialTab?: "discover" | "connected" | "custom_mcp";
}

function parseArgsSafe(raw: string): string[] {
  const trimmed = raw.trim();
  if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) return parsed.map(String);
    } catch {}
  }
  const matches = trimmed.match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g);
  if (!matches) return [];
  return matches.map((arg) => {
    if (
      (arg.startsWith('"') && arg.endsWith('"')) ||
      (arg.startsWith("'") && arg.endsWith("'"))
    ) {
      return arg.slice(1, -1);
    }
    return arg;
  });
}

export const ConnectionsModal: React.FC<ConnectionsModalProps> = ({
  isOpen,
  onClose,
  initialTab = "discover",
}) => {
  const [activeTab, setActiveTab] = useState<"discover" | "connected" | "custom_mcp">(initialTab);
  const [catalog, setCatalog] = useState<ConnectorCatalogItem[]>([]);
  const [connectedAccounts, setConnectedAccounts] = useState<ConnectedAccountSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<"all" | "native" | "mcp">("all");
  const [loading, setLoading] = useState(false);
  const [actionMessage, setActionMessage] = useState<{ text: string; type: "success" | "error" } | null>(null);

  // Connect dialog state
  const [connectingItem, setConnectingItem] = useState<ConnectorCatalogItem | null>(null);
  const [authMethod, setAuthMethod] = useState<"oauth" | "pat">("oauth");
  const [tokenInput, setTokenInput] = useState("");
  const [accountIdentifierInput, setAccountIdentifierInput] = useState("");
  const [displayNameInput, setDisplayNameInput] = useState("");
  const [remoteUrlInput, setRemoteUrlInput] = useState("");
  const [allowPrivateNetwork, setAllowPrivateNetwork] = useState(false);
  const [isSubmittingConnect, setIsSubmittingConnect] = useState(false);

  // GitHub OAuth credentials state
  const [oauthClientId, setOauthClientId] = useState("");
  const [oauthClientSecret, setOauthClientSecret] = useState("");

  // OAuth flow state
  const [oauthSession, setOauthSession] = useState<{
    sessionId: string;
    authUrl: string;
    redirectUri: string;
  } | null>(null);
  const [oauthPolling, setOauthPolling] = useState(false);

  // Manage Account / Policies dialog state
  const [managingAccount, setManagingAccount] = useState<ConnectedAccountSummary | null>(null);
  const [accountPolicies, setAccountPolicies] = useState<ConnectorToolPolicyRecord[]>([]);
  const [accountTools, setAccountTools] = useState<AccountToolView[]>([]);
  const [loadingPolicies, setLoadingPolicies] = useState(false);

  // Custom Local MCP State
  const [localMcpServers, setLocalMcpServers] = useState<McpServerRecord[]>([]);
  const [mcpServerStatuses, setMcpServerStatuses] = useState<Record<string, string>>({});
  const [isAddingLocalMcp, setIsAddingLocalMcp] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpCommand, setMcpCommand] = useState("npx");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpEnvKey, setMcpEnvKey] = useState("");
  const [mcpEnvValue, setMcpEnvValue] = useState("");
  const [mcpEnvIsSecret, setMcpEnvIsSecret] = useState(false);
  const [mcpEnvList, setMcpEnvList] = useState<{ key: string; val: string; is_secret: boolean }[]>([]);

  useEffect(() => {
    if (isOpen) {
      loadAllData();
    }
  }, [isOpen]);

  const loadAllData = async () => {
    setLoading(true);
    try {
      const [catList, accList, mcpList, statuses] = await Promise.all([
        invoke<ConnectorCatalogItem[]>("list_connector_catalog"),
        invoke<ConnectedAccountSummary[]>("list_connected_accounts"),
        invoke<McpServerRecord[]>("list_mcp_servers"),
        invoke<Record<string, string>>("get_mcp_server_statuses").catch(() => ({} as Record<string, string>)),
      ]);
      setCatalog(catList);
      setConnectedAccounts(accList);
      setLocalMcpServers(mcpList);
      setMcpServerStatuses(statuses || {});
    } catch (err: any) {
      console.error("Failed to load connections:", err);
      showMessage(String(err), "error");
    } finally {
      setLoading(false);
    }
  };

  const showMessage = (text: string, type: "success" | "error") => {
    setActionMessage({ text, type });
    setTimeout(() => setActionMessage(null), 5000);
  };

  const handleOpenConnect = (item: ConnectorCatalogItem) => {
    setConnectingItem(item);
    setAuthMethod(item.auth_type === "pat_or_oauth" ? "oauth" : "pat");
    setTokenInput("");
    setAccountIdentifierInput("");
    setDisplayNameInput(`${item.name} Account`);
    setRemoteUrlInput(item.id === "remote_mcp" ? "http://127.0.0.1:8000/mcp" : "");
    setAllowPrivateNetwork(false);
    setOauthSession(null);
    setOauthPolling(false);
  };

  const handleStartOAuth = async () => {
    if (!connectingItem) return;
    setIsSubmittingConnect(true);
    try {
      const init = await invoke<{
        auth_url: string;
        session_id: string;
        redirect_uri: string;
        expires_at: number;
      }>("start_oauth_flow", {
        connectorId: connectingItem.id,
        clientId: oauthClientId.trim() || null,
        clientSecret: oauthClientSecret.trim() || null,
      });

      setOauthSession({
        sessionId: init.session_id,
        authUrl: init.auth_url,
        redirectUri: init.redirect_uri,
      });
      setOauthPolling(true);

      try {
        const { open } = await import("@tauri-apps/plugin-shell");
        await open(init.auth_url);
      } catch {
        window.open(init.auth_url, "_blank");
      }

      pollOAuthCompletion(init.session_id);
    } catch (err: any) {
      showMessage(`Failed to initiate OAuth: ${err}`, "error");
      setIsSubmittingConnect(false);
    }
  };

  const pollOAuthCompletion = (sessionId: string) => {
    let attempts = 0;
    const maxAttempts = 80; // 2 minutes at 1.5s interval
    let isExchanging = false;

    const pollInterval = setInterval(async () => {
      attempts++;
      if (attempts > maxAttempts) {
        clearInterval(pollInterval);
        setOauthPolling(false);
        setIsSubmittingConnect(false);
        showMessage("OAuth authentication timed out. Please try again.", "error");
        return;
      }

      if (isExchanging) return;

      try {
        const statusView = await invoke<OAuthSessionStatusView>("get_oauth_status", {
          sessionId,
        });

        if (statusView.status === "completed") {
          isExchanging = true;
          clearInterval(pollInterval);
          try {
            const summary = await invoke<ConnectedAccountSummary>("complete_oauth_flow", {
              sessionId,
              code: null,
            });
            setOauthPolling(false);
            setOauthSession(null);
            setConnectingItem(null);
            setIsSubmittingConnect(false);
            showMessage(
              `Successfully connected GitHub account: @${summary.account_identifier}!`,
              "success"
            );
            await loadAllData();
            setActiveTab("connected");
          } catch (compErr: any) {
            setOauthPolling(false);
            setIsSubmittingConnect(false);
            setOauthSession(null);
            showMessage(`OAuth completion failed: ${compErr}`, "error");
          }
        } else if (
          statusView.status === "failed" ||
          statusView.status === "cancelled" ||
          statusView.status === "expired"
        ) {
          clearInterval(pollInterval);
          setOauthPolling(false);
          setIsSubmittingConnect(false);
          setOauthSession(null);
          const errDetail = statusView.error ? `: ${statusView.error}` : "";
          showMessage(`OAuth authorization ${statusView.status}${errDetail}`, "error");
        }
      } catch (err: any) {
        console.warn("OAuth status poll error:", err);
      }
    }, 1500);
  };

  const handleCancelOAuth = async () => {
    if (oauthSession) {
      try {
        await invoke("cancel_oauth_flow", { sessionId: oauthSession.sessionId });
      } catch (e) {
        console.warn("Cancel OAuth error:", e);
      }
    }
    setOauthPolling(false);
    setOauthSession(null);
    setIsSubmittingConnect(false);
  };

  const handleConnectTokenSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!connectingItem) return;

    if (connectingItem.id === "remote_mcp" && !remoteUrlInput.trim()) {
      showMessage("Please enter the Remote MCP Server URL", "error");
      return;
    }

    if (connectingItem.id !== "remote_mcp" && !tokenInput.trim()) {
      showMessage("Please provide the personal access token", "error");
      return;
    }

    setIsSubmittingConnect(true);
    try {
      const configPayload: Record<string, any> = {};
      if (connectingItem.id === "remote_mcp") {
        configPayload.server_url = remoteUrlInput.trim();
        configPayload.allow_private_network = allowPrivateNetwork;
      }

      await invoke("connect_account_token", {
        connectorId: connectingItem.id,
        displayName: displayNameInput.trim() || connectingItem.name,
        accountIdentifier: accountIdentifierInput.trim() || "default",
        tokenSecret: tokenInput.trim(),
        config: configPayload,
      });

      showMessage(`Successfully connected ${connectingItem.name}!`, "success");
      setConnectingItem(null);
      await loadAllData();
      setActiveTab("connected");
    } catch (err: any) {
      showMessage(`Connection failed: ${err}`, "error");
    } finally {
      setIsSubmittingConnect(false);
    }
  };

  const handleTestConnection = async (accountId: string) => {
    try {
      const res = await invoke<string>("test_connector_connection", { accountId });
      showMessage(`Connection verified: ${res}`, "success");
      await loadAllData();
    } catch (err: any) {
      showMessage(`Connection test failed: ${err}`, "error");
    }
  };

  const handleDisconnect = async (accountId: string, displayName: string) => {
    if (
      !confirm(
        `Are you sure you want to disconnect ${displayName}? All stored vault credentials and associated tool policies will be securely purged.`
      )
    ) {
      return;
    }
    try {
      await invoke("disconnect_account", { accountId });
      showMessage(`Disconnected ${displayName} and removed credentials from vault.`, "success");
      if (managingAccount?.id === accountId) {
        setManagingAccount(null);
      }
      await loadAllData();
    } catch (err: any) {
      showMessage(`Failed to disconnect: ${err}`, "error");
    }
  };

  const handleManagePolicies = async (acc: ConnectedAccountSummary) => {
    setManagingAccount(acc);
    setLoadingPolicies(true);
    try {
      const [policies, tools] = await Promise.all([
        invoke<ConnectorToolPolicyRecord[]>("list_account_tool_policies", {
          accountId: acc.id,
        }),
        invoke<AccountToolView[]>("get_account_tools", {
          accountId: acc.id,
        }),
      ]);
      setAccountPolicies(policies);
      setAccountTools(tools);
    } catch (err: any) {
      console.error(err);
      showMessage(`Failed to load account policies/tools: ${err}`, "error");
    } finally {
      setLoadingPolicies(false);
    }
  };

  const handleSetToolPolicy = async (
    accountId: string,
    toolName: string,
    policy: "allow" | "ask" | "deny",
    riskLevel: string
  ) => {
    try {
      await invoke("save_account_tool_policy", {
        accountId,
        toolName,
        policy,
        riskLevel,
      });
      const policies = await invoke<ConnectorToolPolicyRecord[]>("list_account_tool_policies", {
        accountId,
      });
      setAccountPolicies(policies);
      showMessage(`Updated policy for ${toolName} to ${policy.toUpperCase()}`, "success");
    } catch (err: any) {
      showMessage(`Failed to update policy: ${err}`, "error");
    }
  };

  const handleSaveLocalMcp = async () => {
    if (!mcpName.trim() || !mcpCommand.trim()) {
      showMessage("Please enter server name and command", "error");
      return;
    }
    try {
      const argsArray = parseArgsSafe(mcpArgs);
      const envObj: Record<string, { value: string; is_secret: boolean }> = {};
      mcpEnvList.forEach((e) => {
        if (e.key.trim()) {
          envObj[e.key.trim()] = { value: e.val, is_secret: e.is_secret };
        }
      });

      await invoke("create_mcp_server", {
        server: {
          id: `mcp-${Date.now()}`,
          name: mcpName.trim(),
          command: mcpCommand.trim(),
          args_json: JSON.stringify(argsArray),
          env_json: JSON.stringify(envObj),
          enabled: true,
        },
      });

      setIsAddingLocalMcp(false);
      setMcpName("");
      setMcpCommand("npx");
      setMcpArgs("");
      setMcpEnvList([]);
      showMessage(
        "Local MCP server saved. Sensitive environment secrets stored securely in OS vault.",
        "success"
      );
      await loadAllData();
    } catch (err: any) {
      showMessage(`Failed to save MCP server: ${err}`, "error");
    }
  };

  const handleDeleteLocalMcp = async (id: string) => {
    try {
      await invoke("delete_mcp_server", { id });
      showMessage("Removed local MCP server and deleted vault secrets.", "success");
      await loadAllData();
    } catch (err: any) {
      showMessage(`Failed to delete server: ${err}`, "error");
    }
  };

  const filteredCatalog = catalog.filter((item) => {
    const matchesSearch =
      item.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      item.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
      item.capabilities.some((c) => c.toLowerCase().includes(searchQuery.toLowerCase()));

    if (!matchesSearch) return false;
    if (selectedCategory === "native") return item.connector_type === "native";
    if (selectedCategory === "mcp") return item.connector_type !== "native";
    return true;
  });

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-md p-4">
      <div className="w-full max-w-4xl bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Top Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#262626] bg-[#141414]">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-xl bg-neutral-800 border border-neutral-700 text-neutral-200">
              <Plug className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-bold text-neutral-100 flex items-center gap-2">
                Connections & App Integrations
              </h2>
              <p className="text-xs text-neutral-400">
                Connect external apps, govern autonomous tools, and securely manage credentials with local-first OS vault storage.
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={loadAllData}
              disabled={loading}
              className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
              title="Refresh connections"
            >
              <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin text-neutral-200" : ""}`} />
            </button>
            <button
              onClick={onClose}
              className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Global Toast / Alert Message */}
        {actionMessage && (
          <div
            className={`px-6 py-2.5 text-xs flex items-center justify-between transition ${
              actionMessage.type === "success"
                ? "bg-neutral-900 border-b border-neutral-800 text-neutral-200"
                : "bg-red-950/60 border-b border-red-800 text-red-200"
            }`}
          >
            <div className="flex items-center gap-2">
              {actionMessage.type === "success" ? (
                <CheckCircle className="w-4 h-4 text-neutral-300" />
              ) : (
                <AlertCircle className="w-4 h-4 text-red-400" />
              )}
              <span>{actionMessage.text}</span>
            </div>
            <button onClick={() => setActionMessage(null)} className="text-xs opacity-70 hover:opacity-100">
              Dismiss
            </button>
          </div>
        )}

        {/* Navigation Tabs */}
        <div className="flex items-center gap-2 px-6 pt-3 border-b border-[#262626] bg-[#111111]">
          <button
            onClick={() => setActiveTab("discover")}
            className={`px-4 py-2.5 text-xs font-semibold flex items-center gap-2 border-b-2 transition ${
              activeTab === "discover"
                ? "border-neutral-100 text-neutral-100 bg-neutral-800/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Search className="w-3.5 h-3.5" /> Discover Catalog
          </button>
          <button
            onClick={() => setActiveTab("connected")}
            className={`px-4 py-2.5 text-xs font-semibold flex items-center gap-2 border-b-2 transition ${
              activeTab === "connected"
                ? "border-neutral-100 text-neutral-100 bg-neutral-800/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <CheckCircle className="w-3.5 h-3.5" /> Connected Accounts ({connectedAccounts.length})
          </button>
          <button
            onClick={() => setActiveTab("custom_mcp")}
            className={`px-4 py-2.5 text-xs font-semibold flex items-center gap-2 border-b-2 transition ${
              activeTab === "custom_mcp"
                ? "border-neutral-100 text-neutral-100 bg-neutral-800/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Terminal className="w-3.5 h-3.5" /> Local MCP Servers ({localMcpServers.length})
          </button>
        </div>

        {/* Body Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {/* TAB 1: DISCOVER CATALOG */}
          {activeTab === "discover" && (
            <div className="space-y-6">
              {/* Search and Filters */}
              <div className="flex flex-col sm:flex-row items-center justify-between gap-3">
                <div className="relative flex-1 w-full">
                  <Search className="w-4 h-4 text-neutral-500 absolute left-3.5 top-3" />
                  <input
                    type="text"
                    placeholder="Search integrations by app, keyword, or action (e.g. 'github', 'remote mcp')..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="w-full pl-10 pr-4 py-2 bg-[#0a0a0a] border border-[#262626] rounded-xl text-xs text-neutral-200 placeholder-neutral-500 focus:outline-none focus:border-neutral-400 transition"
                  />
                </div>
                <div className="flex items-center gap-1.5 w-full sm:w-auto">
                  <button
                    onClick={() => setSelectedCategory("all")}
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium transition ${
                      selectedCategory === "all"
                        ? "bg-[#ededed] text-[#0a0a0a]"
                        : "bg-[#171717] text-neutral-400 hover:text-neutral-200 border border-[#262626]"
                    }`}
                  >
                    All
                  </button>
                  <button
                    onClick={() => setSelectedCategory("native")}
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium transition ${
                      selectedCategory === "native"
                        ? "bg-[#ededed] text-[#0a0a0a]"
                        : "bg-[#171717] text-neutral-400 hover:text-neutral-200 border border-[#262626]"
                    }`}
                  >
                    Native API
                  </button>
                  <button
                    onClick={() => setSelectedCategory("mcp")}
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium transition ${
                      selectedCategory === "mcp"
                        ? "bg-[#ededed] text-[#0a0a0a]"
                        : "bg-[#171717] text-neutral-400 hover:text-neutral-200 border border-[#262626]"
                    }`}
                  >
                    MCP Protocols
                  </button>
                </div>
              </div>

              {/* Grid of Catalog Integrations */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {filteredCatalog.map((item) => (
                  <div
                    key={item.id}
                    className="p-5 bg-[#171717] border border-[#262626] hover:border-neutral-700 rounded-xl flex flex-col justify-between transition shadow-sm"
                  >
                    <div>
                      <div className="flex items-start justify-between gap-3 mb-2.5">
                        <div className="flex items-center gap-3">
                          <div className="w-10 h-10 rounded-xl bg-[#0a0a0a] border border-[#262626] flex items-center justify-center shadow-inner">
                            <ServiceIcon id={item.id} name={item.name} className="w-5 h-5" />
                          </div>
                          <div>
                            <div className="text-sm font-semibold text-neutral-100 flex items-center gap-2">
                              {item.name}
                              {item.connector_type === "native" ? (
                                <span className="text-[10px] px-1.5 py-0.5 bg-neutral-800 text-neutral-300 border border-neutral-700 rounded font-medium">
                                  Native API
                                </span>
                              ) : (
                                <span className="text-[10px] px-1.5 py-0.5 bg-neutral-800 text-neutral-300 border border-neutral-700 rounded font-medium">
                                  MCP Transport
                                </span>
                              )}
                            </div>
                            <div className="text-[11px] text-neutral-400">
                              Auth: <span className="font-mono text-neutral-300">{item.auth_type}</span>
                            </div>
                          </div>
                        </div>

                        {/* Truthful Status Badge */}
                        {item.status === "connected" ? (
                          <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-neutral-800 text-neutral-200 border border-neutral-600 flex items-center gap-1">
                            <CheckCircle className="w-3 h-3 text-neutral-300" /> Connected ({item.connected_accounts_count})
                          </span>
                        ) : item.status === "catalog_only" ? (
                          <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-neutral-800 text-neutral-400 border border-neutral-700">
                            Roadmap / Catalog
                          </span>
                        ) : item.is_available ? (
                          <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-neutral-800/80 text-neutral-300 border border-neutral-700">
                            Ready to Connect
                          </span>
                        ) : (
                          <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-neutral-800 text-neutral-400 border border-neutral-700">
                            Setup Required
                          </span>
                        )}
                      </div>

                      <p className="text-xs text-neutral-300 leading-relaxed mb-3">
                        {item.description}
                      </p>

                      {/* Capabilities pills */}
                      <div className="flex flex-wrap gap-1.5 mb-4">
                        {item.capabilities.map((cap) => (
                          <span
                            key={cap}
                            className="text-[10px] px-2 py-0.5 rounded-md bg-[#0a0a0a] border border-[#262626] text-neutral-400 font-mono"
                          >
                            {cap}
                          </span>
                        ))}
                      </div>
                    </div>

                    <div className="pt-3 border-t border-[#262626] flex items-center justify-between">
                      {item.doc_url ? (
                        <a
                          href={item.doc_url}
                          target="_blank"
                          rel="noreferrer"
                          className="text-[11px] text-neutral-400 hover:text-neutral-200 hover:underline flex items-center gap-1"
                        >
                          Docs <ExternalLink className="w-3 h-3" />
                        </a>
                      ) : (
                        <span />
                      )}

                      {item.status === "catalog_only" ? (
                        <span className="text-[11px] text-neutral-500 italic">
                          Custom OAuth app required
                        </span>
                      ) : item.is_available ? (
                        <button
                          onClick={() => handleOpenConnect(item)}
                          className="px-3 py-1.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold flex items-center gap-1.5 shadow transition"
                        >
                          <Plus className="w-3.5 h-3.5" /> Connect Account
                        </button>
                      ) : (
                        <button
                          disabled
                          className="px-3 py-1.5 bg-neutral-800 text-neutral-500 cursor-not-allowed rounded-lg text-xs font-semibold"
                        >
                          Requires Setup
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* TAB 2: CONNECTED ACCOUNTS */}
          {activeTab === "connected" && (
            <div className="space-y-6">
              {connectedAccounts.length === 0 ? (
                <div className="p-12 text-center bg-[#171717] border border-[#262626] rounded-2xl space-y-3">
                  <div className="w-12 h-12 mx-auto rounded-2xl bg-neutral-800 flex items-center justify-center text-neutral-400">
                    <Plug className="w-6 h-6" />
                  </div>
                  <h3 className="text-sm font-semibold text-neutral-200">No Connected Accounts Yet</h3>
                  <p className="text-xs text-neutral-400 max-w-sm mx-auto">
                    Browse the integration catalog to connect GitHub, remote MCP endpoints, or local MCP subprocesses.
                  </p>
                  <button
                    onClick={() => setActiveTab("discover")}
                    className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] text-xs font-semibold rounded-lg shadow transition"
                  >
                    Browse Catalog
                  </button>
                </div>
              ) : (
                <div className="space-y-4">
                  {connectedAccounts.map((acc) => (
                    <div
                      key={acc.id}
                      className="p-5 bg-[#171717] border border-[#262626] rounded-xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4 shadow-sm"
                    >
                      <div className="space-y-1.5">
                        <div className="flex items-center gap-2.5">
                          <div className="w-8 h-8 rounded-lg bg-[#0a0a0a] border border-[#262626] flex items-center justify-center shrink-0">
                            <ServiceIcon id={acc.connector_id} name={acc.display_name} className="w-4 h-4" />
                          </div>
                          <div className="flex items-center gap-2">
                            <div className="text-sm font-bold text-neutral-100">{acc.display_name}</div>
                            <span className="text-xs text-neutral-400 font-mono">(@{acc.account_identifier})</span>
                            <span
                              className={`text-[10px] font-semibold px-2 py-0.5 rounded-full border ${
                                acc.status === "connected"
                                  ? "bg-neutral-800 text-neutral-200 border-neutral-600"
                                  : acc.status === "error"
                                  ? "bg-red-600/20 text-red-300 border-red-500/30"
                                  : "bg-neutral-800 text-neutral-400 border-neutral-700"
                              }`}
                            >
                              {acc.status.toUpperCase()}
                            </span>
                          </div>
                        </div>

                        {acc.status_message && (
                          <div className="text-xs text-neutral-300 flex items-center gap-1.5">
                            <Info className="w-3.5 h-3.5 text-neutral-400 shrink-0" />
                            <span>{acc.status_message}</span>
                          </div>
                        )}

                        {acc.last_error && (
                          <div className="text-xs text-red-300 flex items-center gap-1.5 font-mono">
                            <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
                            <span>{acc.last_error}</span>
                          </div>
                        )}

                        {acc.granted_scopes && acc.granted_scopes.length > 0 && (
                          <div className="flex items-center gap-1.5 flex-wrap pt-0.5">
                            <span className="text-[10px] text-neutral-500">Scopes:</span>
                            {acc.granted_scopes.map((s) => (
                              <span
                                key={s}
                                className="text-[10px] font-mono px-1.5 py-0.5 bg-[#0a0a0a] border border-[#262626] rounded text-neutral-300"
                              >
                                {s}
                              </span>
                            ))}
                          </div>
                        )}

                        <div className="text-[11px] text-neutral-500 flex items-center gap-3 pt-1">
                          <span>
                            Connector: <strong>{acc.connector_id}</strong>
                          </span>
                          <span>•</span>
                          <span>Connected: {new Date(acc.created_at).toLocaleDateString()}</span>
                          {acc.last_connected_at && (
                            <>
                              <span>•</span>
                              <span>Last active: {new Date(acc.last_connected_at).toLocaleTimeString()}</span>
                            </>
                          )}
                        </div>
                      </div>

                      <div className="flex items-center gap-2 w-full md:w-auto justify-end">
                        <button
                          onClick={() => handleTestConnection(acc.id)}
                          className="px-3 py-1.5 bg-[#0a0a0a] hover:bg-neutral-800 border border-[#262626] text-neutral-300 rounded-lg text-xs flex items-center gap-1.5 transition"
                          title="Test live connection to API"
                        >
                          <RefreshCw className="w-3.5 h-3.5 text-neutral-400" /> Test
                        </button>
                        <button
                          onClick={() => handleManagePolicies(acc)}
                          className="px-3 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 rounded-lg text-xs flex items-center gap-1.5 transition"
                          title="Configure action approval rules and risk policies"
                        >
                          <Sliders className="w-3.5 h-3.5 text-neutral-400" /> Policies
                        </button>
                        <button
                          onClick={() => handleDisconnect(acc.id, acc.display_name)}
                          className="px-3 py-1.5 bg-red-600/20 hover:bg-red-600/30 text-red-400 border border-red-500/30 rounded-lg text-xs flex items-center gap-1.5 transition"
                          title="Safely remove account and purge vault credentials"
                        >
                          <Trash2 className="w-3.5 h-3.5" /> Disconnect
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 3: CUSTOM MCP */}
          {activeTab === "custom_mcp" && (
            <div className="space-y-6">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Server className="w-4 h-4 text-neutral-300" /> Local Model Context Protocol Subprocesses
                </div>
                Configure custom stdio MCP servers running locally on your machine. All arguments and environment variables are validated safely. Sensitive environment secrets are stored in Kyro's backend OS vault and never exposed through IPC or listings.
              </div>

              <div className="flex items-center justify-between">
                <span className="text-xs text-neutral-400 font-medium">
                  Configured Local Servers ({localMcpServers.length})
                </span>
                <button
                  onClick={() => setIsAddingLocalMcp(!isAddingLocalMcp)}
                  className="px-3 py-1.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold flex items-center gap-1.5 shadow transition"
                >
                  <Plus className="w-3.5 h-3.5" /> {isAddingLocalMcp ? "Cancel" : "Add Local MCP Server"}
                </button>
              </div>

              {/* Add Local MCP Form */}
              {isAddingLocalMcp && (
                <div className="p-4 bg-[#171717] border border-[#262626] rounded-xl space-y-3">
                  <div className="text-xs font-semibold text-neutral-200">New Local MCP Configuration</div>
                  <div>
                    <label className="block text-xs text-neutral-300 mb-1">Server Name</label>
                    <input
                      type="text"
                      placeholder="e.g. Local Filesystem Server"
                      value={mcpName}
                      onChange={(e) => setMcpName(e.target.value)}
                      className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-400"
                    />
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-xs text-neutral-300 mb-1">Executable Command</label>
                      <input
                        type="text"
                        placeholder="npx or python3"
                        value={mcpCommand}
                        onChange={(e) => setMcpCommand(e.target.value)}
                        className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-400 font-mono"
                      />
                    </div>
                    <div className="col-span-2">
                      <label className="block text-xs text-neutral-300 mb-1">Arguments</label>
                      <input
                        type="text"
                        placeholder='-y @modelcontextprotocol/server-filesystem "/path with spaces"'
                        value={mcpArgs}
                        onChange={(e) => setMcpArgs(e.target.value)}
                        className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-400 font-mono"
                      />
                    </div>
                  </div>

                  {/* Environment Variables Editor */}
                  <div className="pt-2 border-t border-[#262626]">
                    <label className="block text-xs text-neutral-300 mb-1 font-semibold flex items-center gap-1.5">
                      <Lock className="w-3.5 h-3.5 text-neutral-300" /> Environment Variables (Explicit Secret Classification)
                    </label>
                    <div className="flex gap-2 mb-2 items-center">
                      <input
                        type="text"
                        placeholder="KEY (e.g. DATABASE_URL, API_KEY)"
                        value={mcpEnvKey}
                        onChange={(e) => {
                          const val = e.target.value;
                          setMcpEnvKey(val);
                          const lower = val.toLowerCase();
                          if (
                            lower.includes("key") ||
                            lower.includes("token") ||
                            lower.includes("secret") ||
                            lower.includes("pass") ||
                            lower.includes("auth") ||
                            lower.includes("database") ||
                            lower.includes("db_url") ||
                            lower.includes("dsn") ||
                            lower.includes("url")
                          ) {
                            setMcpEnvIsSecret(true);
                          }
                        }}
                        className="flex-1 px-3 py-1.5 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 font-mono"
                      />
                      <input
                        type={mcpEnvIsSecret ? "password" : "text"}
                        placeholder={mcpEnvIsSecret ? "VALUE (stored in Vault)" : "VALUE (plain)"}
                        value={mcpEnvValue}
                        onChange={(e) => setMcpEnvValue(e.target.value)}
                        className="flex-1 px-3 py-1.5 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 font-mono"
                      />
                      <label className="flex items-center gap-1 text-xs text-neutral-300 cursor-pointer select-none">
                        <input
                          type="checkbox"
                          checked={mcpEnvIsSecret}
                          onChange={(e) => setMcpEnvIsSecret(e.target.checked)}
                          className="rounded border-neutral-700 bg-neutral-900 text-neutral-100 focus:ring-0"
                        />
                        <span className="text-[11px] text-neutral-400">Vault Secret</span>
                      </label>
                      <button
                        type="button"
                        onClick={() => {
                          if (mcpEnvKey.trim()) {
                            setMcpEnvList([
                              ...mcpEnvList,
                              { key: mcpEnvKey.trim(), val: mcpEnvValue, is_secret: mcpEnvIsSecret },
                            ]);
                            setMcpEnvKey("");
                            setMcpEnvValue("");
                            setMcpEnvIsSecret(false);
                          }
                        }}
                        className="px-3 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-xs text-neutral-200 rounded-lg whitespace-nowrap"
                      >
                        Add Env
                      </button>
                    </div>
                    {mcpEnvList.length > 0 && (
                      <div className="space-y-1">
                        {mcpEnvList.map((env, i) => (
                          <div
                            key={i}
                            className="flex items-center justify-between px-2.5 py-1 bg-[#0a0a0a] rounded text-xs text-neutral-300 font-mono"
                          >
                            <span>
                              {env.key} ={" "}
                              {env.is_secret ? (
                                <span className="text-neutral-300 font-semibold">[VAULT_SECRET]</span>
                              ) : (
                                <span className="text-neutral-300">{env.val}</span>
                              )}
                            </span>
                            <button
                              type="button"
                              onClick={() => setMcpEnvList(mcpEnvList.filter((_, idx) => idx !== i))}
                              className="text-red-400 hover:text-red-300"
                            >
                              <X className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  <button
                    onClick={handleSaveLocalMcp}
                    className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold shadow transition"
                  >
                    Save & Register Local Server
                  </button>
                </div>
              )}

              {/* Server List */}
              <div className="space-y-3">
                {localMcpServers.map((srv) => (
                  <div
                    key={srv.id}
                    className="p-4 bg-[#171717] border border-[#262626] rounded-xl flex items-center justify-between gap-4"
                  >
                    <div>
                      <div className="text-xs font-semibold text-neutral-100 flex items-center gap-2">
                        {srv.name}
                        {(() => {
                          const status = mcpServerStatuses[srv.id] || (srv.enabled ? "configured" : "disabled");
                          const isConnected = status === "connected";
                          const isFailed = status.startsWith("failed");
                          return (
                            <span
                              className={`text-[10px] px-1.5 py-0.5 rounded uppercase font-sans ${
                                isConnected
                                  ? "bg-neutral-800 text-neutral-200 border border-neutral-600"
                                  : isFailed
                                  ? "bg-red-950/60 text-red-400 border border-red-800/40"
                                  : "bg-neutral-800 text-neutral-400 border border-neutral-700/50"
                              }`}
                            >
                              {status}
                            </span>
                          );
                        })()}
                      </div>
                      <div className="text-[11px] text-neutral-400 font-mono mt-1">
                        {srv.command} {srv.args_json}
                      </div>
                    </div>
                    <button
                      onClick={() => handleDeleteLocalMcp(srv.id)}
                      className="p-2 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                      title="Delete server"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* CONNECT MODAL DIALOG */}
        {connectingItem && (
          <div className="fixed inset-0 z-60 flex items-center justify-center bg-black/80 backdrop-blur-md p-4">
            <div className="w-full max-w-md bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl p-6 space-y-4">
              <div className="flex items-center justify-between border-b border-[#262626] pb-3">
                <div className="flex items-center gap-2.5">
                  <div className="w-8 h-8 rounded-lg bg-[#0a0a0a] border border-[#262626] flex items-center justify-center shrink-0">
                    <ServiceIcon id={connectingItem.id} name={connectingItem.name} className="w-4 h-4" />
                  </div>
                  <div>
                    <h3 className="text-sm font-bold text-neutral-100">Connect {connectingItem.name}</h3>
                    <p className="text-[11px] text-neutral-400">Authenticating via {connectingItem.auth_type}</p>
                  </div>
                </div>
                <button
                  onClick={() => {
                    handleCancelOAuth();
                    setConnectingItem(null);
                  }}
                  className="text-neutral-400 hover:text-white"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              {/* GitHub Auth Mode Switcher */}
              {connectingItem.id === "github" && !oauthPolling && (
                <div className="flex rounded-lg bg-[#0a0a0a] p-1 border border-[#262626]">
                  <button
                    type="button"
                    onClick={() => setAuthMethod("oauth")}
                    className={`flex-1 py-1.5 text-xs font-semibold rounded-md transition ${
                      authMethod === "oauth"
                        ? "bg-[#ededed] text-[#0a0a0a] shadow"
                        : "text-neutral-400 hover:text-neutral-200"
                    }`}
                  >
                    OAuth 2.0 (Browser)
                  </button>
                  <button
                    type="button"
                    onClick={() => setAuthMethod("pat")}
                    className={`flex-1 py-1.5 text-xs font-semibold rounded-md transition ${
                      authMethod === "pat"
                        ? "bg-[#ededed] text-[#0a0a0a] shadow"
                        : "text-neutral-400 hover:text-neutral-200"
                    }`}
                  >
                    Personal Access Token
                  </button>
                </div>
              )}

              {/* OAuth Polling / In-Progress Screen */}
              {oauthPolling && oauthSession ? (
                <div className="p-5 bg-[#171717] border border-[#262626] rounded-xl text-center space-y-4">
                  <div className="w-12 h-12 mx-auto rounded-full bg-neutral-800 border border-neutral-700 flex items-center justify-center">
                    <Radio className="w-6 h-6 text-neutral-300 animate-pulse" />
                  </div>
                  <div>
                    <h4 className="text-sm font-bold text-neutral-100">Waiting for Browser Sign-In...</h4>
                    <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                      Complete authorization in your browser window. Kyro's local loopback server is securely awaiting the callback.
                    </p>
                  </div>
                  <div className="p-2 bg-[#0a0a0a] border border-[#262626] rounded text-[11px] font-mono text-neutral-400 break-all">
                    Callback: {oauthSession.redirectUri}
                  </div>
                  <div className="flex items-center justify-center gap-3 pt-2">
                    <a
                      href={oauthSession.authUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="px-3 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 rounded-lg text-xs font-medium flex items-center gap-1.5"
                    >
                      Reopen Browser <ExternalLink className="w-3.5 h-3.5" />
                    </a>
                    <button
                      type="button"
                      onClick={handleCancelOAuth}
                      className="px-3 py-1.5 bg-red-600/20 hover:bg-red-600/30 text-red-400 rounded-lg text-xs font-medium"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : connectingItem.id === "github" && authMethod === "oauth" ? (
                <div className="space-y-4">
                  <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                    <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                      <Shield className="w-4 h-4 text-neutral-300" /> Secure Browser PKCE Flow
                    </div>
                    Sign in with your GitHub account directly in your system browser. No credentials or secrets are manually entered into Kyro.
                  </div>

                  <div>
                    <label className="block text-xs text-neutral-300 mb-1 font-medium">
                      OAuth Client ID (Optional)
                    </label>
                    <input
                      type="text"
                      placeholder="e.g. Ov23li... (Leave blank for default / env KYRO_GITHUB_CLIENT_ID)"
                      value={oauthClientId}
                      onChange={(e) => setOauthClientId(e.target.value)}
                      className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 font-mono placeholder:text-neutral-600 focus:outline-none focus:border-neutral-400"
                    />
                    <p className="text-[11px] text-neutral-500 mt-1">
                      Configure your own GitHub OAuth application Client ID if you prefer dedicated rate limits.
                    </p>
                  </div>

                  <div>
                    <label className="block text-xs text-neutral-300 mb-1 font-medium">
                      OAuth Client Secret (Optional - Stored in Vault)
                    </label>
                    <input
                      type="password"
                      placeholder="Optional confidential client secret"
                      value={oauthClientSecret}
                      onChange={(e) => setOauthClientSecret(e.target.value)}
                      className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 font-mono placeholder:text-neutral-600 focus:outline-none focus:border-neutral-400"
                    />
                    <p className="text-[11px] text-neutral-500 mt-1">
                      If your GitHub OAuth App requires a client secret, it will be securely encrypted in the Vault, never in SQLite.
                    </p>
                  </div>

                  <div className="pt-2 flex justify-end gap-2">
                    <button
                      type="button"
                      onClick={() => setConnectingItem(null)}
                      className="px-3 py-1.5 text-xs text-neutral-400 hover:text-white"
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      onClick={handleStartOAuth}
                      disabled={isSubmittingConnect}
                      className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold shadow disabled:opacity-50 transition flex items-center gap-1.5"
                    >
                      {isSubmittingConnect ? (
                        <>
                          <RefreshCw className="w-3.5 h-3.5 animate-spin" /> Launching...
                        </>
                      ) : (
                        <>
                          <ExternalLink className="w-3.5 h-3.5" /> Launch GitHub Sign-In
                        </>
                      )}
                    </button>
                  </div>
                </div>
              ) : (
                /* PAT or Remote MCP Form */
                <form onSubmit={handleConnectTokenSubmit} className="space-y-3">
                  <div>
                    <label className="block text-xs text-neutral-300 mb-1">Account Display Name</label>
                    <input
                      type="text"
                      required
                      value={displayNameInput}
                      onChange={(e) => setDisplayNameInput(e.target.value)}
                      placeholder="e.g. Work GitHub or Personal"
                      className="w-full px-3 py-2 bg-[#0a0c10] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400"
                    />
                  </div>

                  <div>
                    <label className="block text-xs text-neutral-300 mb-1">Account Identifier</label>
                    <input
                      type="text"
                      value={accountIdentifierInput}
                      onChange={(e) => setAccountIdentifierInput(e.target.value)}
                      placeholder="e.g. your GitHub username"
                      className="w-full px-3 py-2 bg-[#0a0c10] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400"
                    />
                  </div>

                  {connectingItem.id === "remote_mcp" ? (
                    <>
                      <div>
                        <label className="block text-xs text-neutral-300 mb-1">Remote MCP Server URL</label>
                        <input
                          type="url"
                          required
                          value={remoteUrlInput}
                          onChange={(e) => setRemoteUrlInput(e.target.value)}
                          placeholder="http://127.0.0.1:8000/mcp or https://remote.company.com/mcp"
                          className="w-full px-3 py-2 bg-[#0a0c10] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400 font-mono"
                        />
                        <p className="text-[10px] text-neutral-400 mt-1 flex items-center gap-1">
                          <Globe className="w-3 h-3 text-neutral-300" /> Non-loopback endpoints require HTTPS.
                        </p>
                      </div>

                      <div className="flex items-center gap-2 pt-1">
                        <input
                          type="checkbox"
                          id="allow_private"
                          checked={allowPrivateNetwork}
                          onChange={(e) => setAllowPrivateNetwork(e.target.checked)}
                          className="rounded border-[#262626] bg-[#0a0a0a] text-neutral-100 focus:ring-0"
                        />
                        <label htmlFor="allow_private" className="text-xs text-neutral-300">
                          Allow private / local intranet network hosts
                        </label>
                      </div>
                    </>
                  ) : null}

                  <div>
                    <label className="block text-xs text-neutral-300 mb-1">
                      {connectingItem.id === "remote_mcp"
                        ? "Bearer Token (Optional)"
                        : "Personal Access Token / API Key"}
                    </label>
                    <input
                      type="password"
                      required={connectingItem.id !== "remote_mcp"}
                      value={tokenInput}
                      onChange={(e) => setTokenInput(e.target.value)}
                      placeholder={connectingItem.id === "github" ? "ghp_..." : "Secret bearer token"}
                      className="w-full px-3 py-2 bg-[#0a0c10] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400 font-mono"
                    />
                    <p className="text-[10px] text-neutral-500 mt-1 flex items-center gap-1">
                      <Lock className="w-3 h-3 text-neutral-300" /> Stored in encrypted OS Vault. Never exposed to model or UI.
                    </p>
                  </div>

                  <div className="pt-3 border-t border-[#262626] flex items-center justify-end gap-2">
                    <button
                      type="button"
                      onClick={() => setConnectingItem(null)}
                      className="px-3 py-1.5 text-xs text-neutral-400 hover:text-white"
                    >
                      Cancel
                    </button>
                    <button
                      type="submit"
                      disabled={isSubmittingConnect}
                      className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold shadow disabled:opacity-50 transition"
                    >
                      {isSubmittingConnect ? "Verifying..." : "Save & Connect"}
                    </button>
                  </div>
                </form>
              )}
            </div>
          </div>
        )}

        {/* MANAGE POLICIES DIALOG */}
        {managingAccount && (
          <div className="fixed inset-0 z-60 flex items-center justify-center bg-black/80 backdrop-blur-md p-4">
            <div className="w-full max-w-2xl bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl p-6 space-y-4 max-h-[85vh] flex flex-col">
              <div className="flex items-center justify-between border-b border-[#262626] pb-3">
                <div>
                  <h3 className="text-sm font-bold text-neutral-100">
                    Action Governance: {managingAccount.display_name}
                  </h3>
                  <p className="text-xs text-neutral-400">
                    Configure whether specific operations execute automatically, require human approval, or are denied. Destructive actions cannot be set to auto-approve.
                  </p>
                </div>
                <button onClick={() => setManagingAccount(null)} className="text-neutral-400 hover:text-white">
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="flex-1 overflow-y-auto space-y-3 pr-1">
                {loadingPolicies ? (
                  <div className="py-8 text-center text-xs text-neutral-400">
                    <RefreshCw className="w-4 h-4 animate-spin mx-auto mb-2 text-neutral-300" />
                    Loading action policies...
                  </div>
                ) : accountTools.length > 0 ? (
                  accountTools.map((tool) => {
                    const savedPolicy = accountPolicies.find(
                      (p) => p.tool_name === tool.original_name || p.tool_name === tool.name
                    );
                    const currentPolicy = savedPolicy ? savedPolicy.policy : tool.default_policy;
                    const isDestructive = tool.risk_level === "destructive";
                    const cannotAutoApprove = isDestructive || tool.can_auto_approve === false;

                    return (
                      <div
                        key={tool.name}
                        className="p-3 bg-[#171717] border border-[#262626] rounded-xl flex items-center justify-between gap-3 text-xs"
                      >
                        <div className="flex-1 pr-2">
                          <div className="font-semibold text-neutral-200 font-mono flex items-center gap-2">
                            {tool.original_name}
                            <span
                              className={`text-[9px] px-1.5 py-0.5 rounded font-sans uppercase ${
                                tool.risk_level === "safe"
                                  ? "bg-neutral-800 text-neutral-300 border border-neutral-700"
                                  : tool.risk_level === "destructive"
                                  ? "bg-red-950/60 text-red-400 border border-red-800/40"
                                  : "bg-neutral-800 text-neutral-300 border border-neutral-700"
                              }`}
                            >
                              {tool.risk_level}
                            </span>
                            <span className="text-[9px] px-1.5 py-0.5 rounded font-sans text-neutral-400 border border-neutral-700">
                              {tool.is_read_only ? "read-only" : "modifying"}
                            </span>
                            {tool.can_auto_approve === false && (
                              <span className="text-[9px] px-1.5 py-0.5 rounded font-sans text-amber-400 border border-amber-700/50">
                                approval-only
                              </span>
                            )}
                          </div>
                          <div className="text-[11px] text-neutral-400 mt-0.5">{tool.description}</div>
                        </div>

                        {/* Policy Choice Buttons */}
                        <div className="flex items-center gap-1 bg-[#0a0a0a] p-1 rounded-lg border border-[#262626]">
                          <button
                            disabled={cannotAutoApprove}
                            title={
                              isDestructive
                                ? "Destructive actions cannot be set to auto-approve ('allow'). Policy must be 'ask' or 'deny'."
                                : tool.can_auto_approve === false
                                ? "External tools performing modifications or command execution cannot be set to auto-approve. Policy must be 'ask' or 'deny'."
                                : undefined
                            }
                            onClick={() =>
                              handleSetToolPolicy(
                                managingAccount.id,
                                tool.original_name,
                                "allow",
                                tool.risk_level
                              )
                            }
                            className={`px-2 py-1 rounded text-[11px] font-semibold transition ${
                              cannotAutoApprove
                                ? "opacity-30 cursor-not-allowed text-neutral-500"
                                : currentPolicy === "allow"
                                ? "bg-[#ededed] text-[#0a0a0a] shadow"
                                : "text-neutral-400 hover:text-white"
                            }`}
                          >
                            Allow
                          </button>
                          <button
                            onClick={() =>
                              handleSetToolPolicy(
                                managingAccount.id,
                                tool.original_name,
                                "ask",
                                tool.risk_level
                              )
                            }
                            className={`px-2 py-1 rounded text-[11px] font-semibold transition ${
                              currentPolicy === "ask"
                                ? "bg-amber-600 text-white shadow"
                                : "text-neutral-400 hover:text-white"
                            }`}
                          >
                            Ask Always
                          </button>
                          <button
                            onClick={() =>
                              handleSetToolPolicy(
                                managingAccount.id,
                                tool.original_name,
                                "deny",
                                tool.risk_level
                              )
                            }
                            className={`px-2 py-1 rounded text-[11px] font-semibold transition ${
                              currentPolicy === "deny"
                                ? "bg-red-600 text-white shadow"
                                : "text-neutral-400 hover:text-white"
                            }`}
                          >
                            Deny
                          </button>
                        </div>
                      </div>
                    );
                  })
                ) : (
                  <p className="text-xs text-neutral-500 italic py-4 text-center">
                    No active tools registered for this account. Test or re-initialize the connection.
                  </p>
                )}
              </div>

              <div className="pt-3 border-t border-[#262626] flex justify-end">
                <button
                  onClick={() => setManagingAccount(null)}
                  className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] text-xs font-semibold rounded-lg shadow transition"
                >
                  Done
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
