import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ApiKeys,
  ApiKeysMetadata,
  GovernancePolicy,
  SandboxStatus,
  SystemHealthInfo,
  TemplatePack,
  ImportSummary,
  BrowserCapabilities,
  BrowserSettings,
  AuthorizedFolderRecord,
} from "../types/agent";
import {
  Key,
  Shield,
  Server,
  X,
  Lock,
  Trash2,
  Activity,
  Layers,
  Download,
  Upload,
  CheckCircle,
  FileCode,
  Globe,
  RefreshCw,
  FolderLock,
} from "lucide-react";
import { KyroLogo } from "./Logo";
import { InlineNotice } from "./InlineNotice";
import { ConfirmDialog } from "./ConfirmDialog";
import { TelegramIcon, NvidiaIcon } from "./BrandIcons";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  selectedModel: string;
  onSelectModel: (model: string) => void;
  initialTab?: string;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  selectedModel,
  onSelectModel,
  initialTab,
}) => {
  const [notice, setNotice] = useState<{ tone: "error" | "success" | "info"; message: string } | null>(
    null
  );
  const [secretPendingDeletion, setSecretPendingDeletion] = useState<string | null>(null);
  const [isDeletingSecret, setIsDeletingSecret] = useState(false);
  const [activeTab, setActiveTab] = useState<
    "model" | "privacy" | "governance" | "browser" | "telegram" | "sandbox" | "templates" | "diagnostics"
  >("model");
  const [authorizedFolders, setAuthorizedFolders] = useState<AuthorizedFolderRecord[]>([]);
  const [newFolderPath, setNewFolderPath] = useState("");
  const [newFolderLabel, setNewFolderLabel] = useState("");
  const [browserCapabilities, setBrowserCapabilities] = useState<BrowserCapabilities | null>(null);
  const [browserSettings, setBrowserSettings] = useState<BrowserSettings>({
    selected_browser: "auto",
    headless_default: true,
  });
  const [isRefreshingBrowser, setIsRefreshingBrowser] = useState(false);
  const [isSavingBrowser, setIsSavingBrowser] = useState(false);
  const [metadata, setMetadata] = useState<ApiKeysMetadata>({
    nvidia_configured: false,
    telegram_configured: false,
    telegram_chat_id: "",
  });
  const [newNvidiaKey, setNewNvidiaKey] = useState("");
  const [newTelegramToken, setNewTelegramToken] = useState("");
  const [telegramChatId, setTelegramChatId] = useState("");
  const [modelChoice, setModelChoice] = useState(selectedModel);
  const [policy, setPolicy] = useState<GovernancePolicy>({
    shell_mode: "SafeAutoApprove",
    computer_use_mode: "FullAutonomous",
    require_takeover_for_credentials: true,
    blocked_domains: ["chase.com", "bankofamerica.com", "paypal.com"],
    task_autonomy_mode: "Autonomous",
  });
  const [sandbox, setSandbox] = useState<SandboxStatus | null>(null);
  const [health, setHealth] = useState<SystemHealthInfo | null>(null);
  const [starterTemplates, setStarterTemplates] = useState<TemplatePack[]>([]);
  const [importJson, setImportJson] = useState("");
  const [templateStatus, setTemplateStatus] = useState<string | null>(null);
  const [isSaved, setIsSaved] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (isOpen) {
      if (initialTab && ["model", "privacy", "governance", "browser", "telegram", "sandbox", "templates", "diagnostics"].includes(initialTab)) {
        setActiveTab(initialTab as any);
      }
      loadSettings();
      loadHealthAndTemplates();
    }
  }, [isOpen, initialTab]);

  const loadAuthorizedFolders = async () => {
    try {
      const folders = await invoke<AuthorizedFolderRecord[]>("list_authorized_folders");
      setAuthorizedFolders(folders);
    } catch (err) {
      console.error("Failed to load authorized folders:", err);
    }
  };

  const handleAddFolder = async () => {
    if (!newFolderPath.trim()) return;
    try {
      await invoke("add_authorized_folder", {
        path: newFolderPath.trim(),
        displayName: newFolderLabel.trim() || null,
        label: newFolderLabel.trim() || null,
      });
      setNewFolderPath("");
      setNewFolderLabel("");
      await loadAuthorizedFolders();
      setNotice({ tone: "success", message: "Authorized folder added successfully." });
    } catch (err: any) {
      setNotice({ tone: "error", message: "Failed to authorize folder: " + err });
    }
  };

  const handleRemoveFolder = async (id: string, path: string) => {
    try {
      await invoke("remove_authorized_folder", { id, path });
      await loadAuthorizedFolders();
      setNotice({ tone: "info", message: `Folder authorization removed for ${path}.` });
    } catch (err: any) {
      setNotice({ tone: "error", message: "Failed to remove folder: " + err });
    }
  };

  const loadSettings = async () => {
    try {
      const meta = await invoke<ApiKeysMetadata>("get_api_keys");
      if (meta) {
        setMetadata(meta);
        setTelegramChatId(meta.telegram_chat_id || "");
      }
      const storedPolicy = await invoke<GovernancePolicy>("get_governance_policy");
      if (storedPolicy) {
        setPolicy(storedPolicy);
      }
      const sb = await invoke<SandboxStatus>("get_sandbox_status");
      setSandbox(sb);
      const savedModel = await invoke<string>("get_selected_model").catch(() => null);
      if (savedModel) {
        setModelChoice(savedModel);
      }
      await loadBrowserInfo();
      await loadAuthorizedFolders();
    } catch (err) {
      console.error("Failed to load settings:", err);
    }
  };

  const loadBrowserInfo = async () => {
    try {
      const [caps, bSettings] = await Promise.all([
        invoke<BrowserCapabilities>("get_browser_capabilities"),
        invoke<BrowserSettings>("get_browser_settings"),
      ]);
      setBrowserCapabilities(caps);
      if (bSettings) {
        setBrowserSettings(bSettings);
      }
    } catch (err) {
      console.error("Failed to load browser info:", err);
    }
  };

  const handleSaveBrowserSettings = async () => {
    setIsSavingBrowser(true);
    try {
      await invoke("save_browser_settings", { settings: browserSettings });
      setNotice({ tone: "success", message: "Browser settings saved successfully." });
    } catch (err: any) {
      setNotice({ tone: "error", message: "Failed to save browser settings: " + err });
    } finally {
      setIsSavingBrowser(false);
    }
  };

  const handleRefreshBrowserDiscovery = async () => {
    setIsRefreshingBrowser(true);
    try {
      const caps = await invoke<BrowserCapabilities>("get_browser_capabilities");
      setBrowserCapabilities(caps);
      setNotice({ tone: "info", message: "Discovered browser capabilities refreshed." });
    } catch (err: any) {
      setNotice({ tone: "error", message: "Failed to refresh browsers: " + err });
    } finally {
      setIsRefreshingBrowser(false);
    }
  };

  const loadHealthAndTemplates = async () => {
    try {
      const [hInfo, starters] = await Promise.all([
        invoke<SystemHealthInfo>("get_system_health"),
        invoke<TemplatePack[]>("get_starter_templates"),
      ]);
      setHealth(hInfo);
      setStarterTemplates(starters);
    } catch (err) {
      console.error("Failed to load health/templates:", err);
    }
  };

  const handleSave = async () => {
    setLoading(true);
    try {
      const keysToSave: ApiKeys = {};
      if (newNvidiaKey.trim()) {
        keysToSave.nvidia = newNvidiaKey.trim();
      }
      if (newTelegramToken.trim()) {
        keysToSave.telegram_bot_token = newTelegramToken.trim();
      }
      keysToSave.telegram_chat_id = telegramChatId.trim();

      await invoke("save_api_keys", { keys: keysToSave });
      await invoke("save_governance_policy", { policy });
      await invoke("save_selected_model", { model: modelChoice }).catch(console.error);
      onSelectModel(modelChoice);

      setIsSaved(true);
      setNewNvidiaKey("");
      setNewTelegramToken("");
      await loadSettings();
      setTimeout(() => setIsSaved(false), 2000);
    } catch (err) {
      setNotice({ tone: "error", message: "Failed to save settings: " + err });
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteSecret = async (key: string) => {
    setIsDeletingSecret(true);
    try {
      await invoke("delete_secret_key", { key });
      await loadSettings();
      setNotice({ tone: "success", message: `The ${key} key was removed.` });
    } catch (e) {
      // The key is only reported as removed once the backend confirmed it.
      setNotice({ tone: "error", message: "Failed to delete key: " + e });
    } finally {
      setIsDeletingSecret(false);
      setSecretPendingDeletion(null);
    }
  };

  const handleExportTemplates = async () => {
    try {
      const jsonStr = await invoke<string>("export_workspace_templates", {
        name: "Kyro Workspace Backup",
        description: "Exported agents, skills, and routines",
      });
      const blob = new Blob([jsonStr], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `kyro_template_pack_${Date.now()}.json`;
      a.click();
      URL.revokeObjectURL(url);
      setTemplateStatus("Export completed. Sensitive secrets were stripped automatically.");
    } catch (e: any) {
      setTemplateStatus(`Export failed: ${e}`);
    }
  };

  const handleImportTemplates = async (customJson?: string) => {
    const jsonToUse = customJson || importJson;
    if (!jsonToUse.trim()) return;
    try {
      const res = await invoke<ImportSummary>("import_workspace_templates", {
        jsonStr: jsonToUse.trim(),
      });
      setTemplateStatus(
        `Imported: ${res.bots_imported} bot(s), ${res.skills_imported} skill(s), ${res.routines_imported} routine(s).`
      );
      setImportJson("");
    } catch (e: any) {
      setTemplateStatus(`Import error: ${e}`);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 select-text">
      <div className="w-full max-w-3xl bg-[#111111] border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {notice && (
          <div className="px-6 pt-4">
            <InlineNotice
              tone={notice.tone}
              message={notice.message}
              onDismiss={() => setNotice(null)}
            />
          </div>
        )}
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-neutral-800 bg-[#141414]">
          <div className="flex items-center gap-2.5">
            <KyroLogo className="w-5 h-5" />
            <h2 className="text-base font-semibold text-neutral-100">
              Kyro System Settings & Security Matrix
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Navigation Tabs */}
        <div className="flex items-center px-6 border-b border-neutral-800 bg-[#141414] gap-1 text-xs font-medium overflow-x-auto">
          <button
            onClick={() => setActiveTab("model")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "model"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Key className="w-3.5 h-3.5" /> Model & Credentials
          </button>
          <button
            onClick={() => setActiveTab("privacy")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "privacy"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <FolderLock className="w-3.5 h-3.5" /> Privacy & Folders
          </button>
          <button
            onClick={() => setActiveTab("governance")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "governance"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Lock className="w-3.5 h-3.5" /> Governance Matrix
          </button>
          <button
            onClick={() => setActiveTab("browser")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "browser"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Globe className="w-3.5 h-3.5" /> Browser Automation
          </button>
          <button
            onClick={() => setActiveTab("telegram")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "telegram"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <TelegramIcon className="w-3.5 h-3.5 text-[#26A5E4]" /> Telegram Bridge
          </button>
          <button
            onClick={() => setActiveTab("sandbox")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "sandbox"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Server className="w-3.5 h-3.5" /> Container Sandbox
          </button>
          <button
            onClick={() => setActiveTab("templates")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "templates"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Layers className="w-3.5 h-3.5" /> Templates
          </button>
          <button
            onClick={() => setActiveTab("diagnostics")}
            className={`py-3 px-3 border-b-2 flex items-center gap-1.5 transition whitespace-nowrap ${
              activeTab === "diagnostics"
                ? "border-neutral-200 text-neutral-100 font-medium"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Activity className="w-3.5 h-3.5" /> Diagnostics
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* TAB 1: MODEL & VAULT */}
          {activeTab === "model" && (
            <div className="space-y-4">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Shield className="w-4 h-4 text-neutral-300" /> Secure Authenticated Vault
                </div>
                API keys are stored in the OS credential store / authenticated encrypted vault with AES-256-GCM. Raw credentials are never returned to UI state or persisted in plaintext.
              </div>

              {/* Model Choice */}
              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Autonomous Model Selection
                </label>
                <select
                  value={modelChoice}
                  onChange={(e) => setModelChoice(e.target.value)}
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                >
                  <option value="nvidia/nemotron-3-ultra-550b-a55b">
                    NVIDIA Nemotron-3-Ultra-550B (Default Primary Agent)
                  </option>
                  <option value="meta/llama-3.2-11b-vision-instruct">
                    Meta Llama-3.2-11B-Vision-Instruct (Multimodal Visual Specialist)
                  </option>
                  <option value="meta/llama-3.3-70b-instruct">
                    Meta Llama-3.3-70B-Instruct (Fast Code & Reasoning)
                  </option>
                </select>
              </div>

              {/* NVIDIA NIM Key */}
              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5 flex items-center gap-1.5">
                  <NvidiaIcon className="w-3.5 h-3.5 text-[#76B900]" />
                  <span>NVIDIA NIM API Key</span>
                </label>
                <div className="flex gap-2">
                  <input
                    type="password"
                    placeholder={
                      metadata?.nvidia_configured
                        ? "•••••••••••••••••••••••• (Configured in Vault)"
                        : "nvapi-..."
                    }
                    value={newNvidiaKey}
                    onChange={(e) => setNewNvidiaKey(e.target.value)}
                    className="flex-1 px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                  />
                  {metadata.nvidia_configured && (
                    <button
                      onClick={() => setSecretPendingDeletion("nvidia")}
                      className="px-3 py-2 bg-red-950/40 hover:bg-red-900/60 text-red-300 border border-red-800/40 rounded-xl text-xs flex items-center gap-1 transition"
                      title="Purge key from secure vault"
                    >
                      <Trash2 className="w-3.5 h-3.5" /> Purge
                    </button>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* TAB: PRIVACY & AUTHORIZED FOLDERS */}
          {activeTab === "privacy" && (
            <div className="space-y-5">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed space-y-1.5">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Shield className="w-4 h-4 text-amber-400" /> Local-First Architecture & Data Isolation
                </div>
                <p>
                  Document parsing, full-text extraction, folder indexing, SQLite metadata storage, and credentials stay securely on this device. Local files are never read or indexed outside your explicitly authorized folders.
                </p>
                <p className="text-neutral-400">
                  When you request natural-language document analysis or summarization, only the relevant extracted text passages are sent via encrypted HTTPS to your configured model endpoint (such as NVIDIA NIM) for completion. No raw files or databases are uploaded to external telemetry.
                </p>
              </div>

              {/* Authorized Folders */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-xs font-semibold text-neutral-100">Authorized Folders</h3>
                    <p className="text-[11px] text-neutral-400">
                      Kyro can only search, analyze, and organize files inside these explicitly authorized directories.
                    </p>
                  </div>
                </div>

                <div className="p-3.5 bg-[#141414] border border-neutral-800 rounded-xl space-y-3">
                  <div className="text-xs font-medium text-neutral-200">Authorize a New Folder</div>
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
                    <input
                      type="text"
                      placeholder="Folder path (e.g. /home/user/Documents)"
                      value={newFolderPath}
                      onChange={(e) => setNewFolderPath(e.target.value)}
                      className="md:col-span-2 px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 placeholder:text-neutral-600 focus:outline-none focus:border-neutral-600 font-mono"
                    />
                    <input
                      type="text"
                      placeholder="Label (optional, e.g. College Work)"
                      value={newFolderLabel}
                      onChange={(e) => setNewFolderLabel(e.target.value)}
                      className="px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 placeholder:text-neutral-600 focus:outline-none focus:border-neutral-600"
                    />
                  </div>
                  <div className="flex items-center justify-between pt-1">
                    <div className="text-[11px] text-neutral-400">
                      Kyro will be granted access to read and safely index documents inside this folder.
                    </div>
                    <button
                      onClick={handleAddFolder}
                      disabled={!newFolderPath.trim()}
                      className="px-3.5 py-1.5 bg-neutral-200 hover:bg-white text-neutral-950 font-medium rounded-lg text-xs transition disabled:opacity-40"
                    >
                      Authorize Folder
                    </button>
                  </div>
                </div>

                <div className="space-y-2">
                  {authorizedFolders.length === 0 ? (
                    <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl text-center text-xs text-neutral-500">
                      No folders authorized yet. Add a folder above to enable natural-language document search and assistance.
                    </div>
                  ) : (
                    authorizedFolders.map((folder) => (
                      <div
                        key={folder.id}
                        className="p-3 bg-[#141414] border border-neutral-800 rounded-xl flex items-center justify-between text-xs"
                      >
                        <div className="space-y-0.5">
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-neutral-200">
                              {folder.display_name || folder.path.split("/").filter(Boolean).pop() || "Folder"}
                            </span>
                            <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-neutral-800 text-neutral-200 border border-neutral-700">
                              Authorized
                            </span>
                          </div>
                          <div className="font-mono text-[11px] text-neutral-400">{folder.path}</div>
                        </div>
                        <button
                          onClick={() => handleRemoveFolder(folder.id, folder.path)}
                          className="p-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                          title="Revoke Folder Authorization"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    ))
                  )}
                </div>
              </div>

              {/* Local Retention Policy */}
              <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-2">
                <div className="text-xs font-semibold text-neutral-200">Local Retention Policy</div>
                <p className="text-xs text-neutral-400 leading-relaxed">
                  Routine runs, task execution logs, and file operation transaction logs are stored inside your local SQLite database
                  in your user app data directory. Kyro does not transmit audit logs to any central telemetry server.
                </p>
              </div>
            </div>
          )}

          {/* TAB 2: GOVERNANCE MATRIX */}
          {activeTab === "governance" && (
            <div className="space-y-4">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Lock className="w-4 h-4 text-neutral-300" /> Authoritative Policy Matrix
                </div>
                Controls host terminal permissions, computer use safety, and sensitive domain boundaries. Evaluated strictly inside Rust backend.
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Shell Execution Mode
                </label>
                <select
                  value={policy.shell_mode}
                  onChange={(e) => setPolicy({ ...policy, shell_mode: e.target.value })}
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
                >
                  <option value="SafeAutoApprove">SafeAutoApprove (Auto-executes read-only safe commands)</option>
                  <option value="SandboxOnly">SandboxOnly (Mandatory isolated container execution; fails closed)</option>
                  <option value="AskAlways">AskAlways (Prompt human approval for every shell call)</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Computer-Use Automation Mode
                </label>
                <select
                  value={policy.computer_use_mode}
                  onChange={(e) => setPolicy({ ...policy, computer_use_mode: e.target.value })}
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
                >
                  <option value="FullAutonomous">FullAutonomous (Free mouse & keyboard actions)</option>
                  <option value="ConfirmDestructive">ConfirmDestructive (Pause on close windows / delete shortcuts)</option>
                  <option value="AskAlways">AskAlways (Prompt human confirmation on all desktop inputs)</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Task Autonomy Mode
                </label>
                <select
                  value={policy.task_autonomy_mode || "Autonomous"}
                  onChange={(e) => setPolicy({ ...policy, task_autonomy_mode: e.target.value })}
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
                >
                  <option value="Autonomous">Autonomous (Run routine, in-scope actions without repeated prompts)</option>
                  <option value="Supervised">Supervised (Prompt approval for individual operations)</option>
                </select>
              </div>

              <div className="flex items-center justify-between p-3.5 bg-[#141414] border border-neutral-800 rounded-xl">
                <div>
                  <div className="text-xs font-semibold text-neutral-100">
                    Human Takeover for Sensitive Checkpoints
                  </div>
                  <div className="text-[11px] text-neutral-400">
                    Automatically halts agent on 2FA, password fields, or banking domains
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={policy.require_takeover_for_credentials}
                  onChange={(e) =>
                    setPolicy({ ...policy, require_takeover_for_credentials: e.target.checked })
                  }
                  className="w-4 h-4 rounded border-neutral-700 bg-neutral-900 accent-neutral-200"
                />
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Blocked High-Risk Domains (Comma-separated)
                </label>
                <input
                  type="text"
                  value={policy.blocked_domains.join(", ")}
                  onChange={(e) =>
                    setPolicy({
                      ...policy,
                      blocked_domains: e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
                    })
                  }
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                />
              </div>
            </div>
          )}

          {/* TAB: BROWSER AUTOMATION */}
          {activeTab === "browser" && (
            <div className="space-y-4">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed flex items-start justify-between gap-4">
                <div>
                  <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                    <Globe className="w-4 h-4 text-sky-400" /> Browser Automation & Capability Matrix
                  </div>
                  Kyro drives real web browsers via native protocols (WebDriver BiDi for Firefox, Chrome DevTools Protocol for Chromium family) using an isolated automation profile. Your everyday cookies, passwords, and personal history are never accessed.
                </div>
                <button
                  type="button"
                  onClick={handleRefreshBrowserDiscovery}
                  disabled={isRefreshingBrowser}
                  className="px-2.5 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 rounded-lg text-xs font-medium flex items-center gap-1.5 transition shrink-0"
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${isRefreshingBrowser ? "animate-spin" : ""}`} />
                  Refresh
                </button>
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Default Automation Browser
                </label>
                <div className="space-y-2">
                  <div
                    onClick={() => setBrowserSettings({ ...browserSettings, selected_browser: "auto" })}
                    className={`p-3 rounded-xl border text-xs cursor-pointer transition flex items-center justify-between ${
                      browserSettings.selected_browser === "auto"
                        ? "bg-neutral-800/80 border-neutral-600 text-neutral-100"
                        : "bg-[#141414] border-neutral-800 text-neutral-300 hover:border-neutral-700"
                    }`}
                  >
                    <div className="flex items-center gap-2.5">
                      <input
                        type="radio"
                        name="browser_choice"
                        checked={browserSettings.selected_browser === "auto"}
                        onChange={() => setBrowserSettings({ ...browserSettings, selected_browser: "auto" })}
                        className="text-neutral-200 focus:ring-0"
                      />
                      <div>
                        <div className="font-medium text-neutral-100">Automatic (System Default)</div>
                        <div className="text-[11px] text-neutral-400">
                          {browserCapabilities?.default_browser_id
                            ? `Detected OS default: ${browserCapabilities.browsers.find((b) => b.id === browserCapabilities.default_browser_id)?.display_name || browserCapabilities.default_browser_id}`
                            : "Uses operating system default browser if automatable, or first available"}
                        </div>
                      </div>
                    </div>
                    <span className="text-[10px] px-2 py-0.5 rounded bg-sky-950 text-sky-400 border border-sky-800">
                      Recommended
                    </span>
                  </div>

                  {browserCapabilities?.browsers
                    ?.filter((b) => b.available && b.automatable)
                    ?.map((b) => (
                      <div
                        key={b.id}
                        onClick={() => setBrowserSettings({ ...browserSettings, selected_browser: b.id })}
                        className={`p-3 rounded-xl border text-xs cursor-pointer transition flex items-center justify-between ${
                          browserSettings.selected_browser === b.id
                            ? "bg-neutral-800/80 border-neutral-600 text-neutral-100"
                            : "bg-[#141414] border-neutral-800 text-neutral-300 hover:border-neutral-700"
                        }`}
                      >
                        <div className="flex items-center gap-2.5">
                          <input
                            type="radio"
                            name="browser_choice"
                            checked={browserSettings.selected_browser === b.id}
                            onChange={() => setBrowserSettings({ ...browserSettings, selected_browser: b.id })}
                            className="text-neutral-200 focus:ring-0"
                          />
                          <div>
                            <div className="font-medium text-neutral-100 flex items-center gap-2">
                              {b.display_name}
                              {b.version && (
                                <span className="text-[10px] text-neutral-400 font-mono">v{b.version}</span>
                              )}
                            </div>
                            <div className="text-[11px] text-neutral-400 font-mono truncate max-w-md">
                              {b.path} ({b.adapter === "webdriver-bidi" ? "WebDriver BiDi" : "CDP"})
                            </div>
                          </div>
                        </div>
                        <span className="text-[10px] px-2 py-0.5 rounded bg-neutral-800 text-neutral-200 border border-neutral-700">
                          Ready
                        </span>
                      </div>
                    ))}

                  {browserCapabilities?.browsers
                    ?.filter((b) => !b.available || !b.automatable)
                    ?.map((b) => (
                      <div
                        key={b.id}
                        className="p-3 rounded-xl border border-neutral-800/60 bg-[#121212] text-xs text-neutral-500 flex items-center justify-between opacity-60"
                      >
                        <div>
                          <div className="font-medium text-neutral-400">{b.display_name}</div>
                          <div className="text-[11px] text-neutral-500">
                            {b.unavailable_reason || "Not installed or unavailable on this system"}
                          </div>
                        </div>
                        <span className="text-[10px] px-2 py-0.5 rounded bg-neutral-900 text-neutral-500 border border-neutral-800">
                          Unavailable
                        </span>
                      </div>
                    ))}
                </div>
              </div>

              <div className="flex items-center justify-between p-3.5 bg-[#141414] border border-neutral-800 rounded-xl">
                <div>
                  <div className="text-xs font-semibold text-neutral-100">
                    Headless Automation Mode (Unattended)
                  </div>
                  <div className="text-[11px] text-neutral-400">
                    Run browser operations silently in background. When unchecked or explicitly requested by a task, a visible browser window opens.
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={browserSettings.headless_default}
                  onChange={(e) =>
                    setBrowserSettings({ ...browserSettings, headless_default: e.target.checked })
                  }
                  className="w-4 h-4 rounded border-neutral-700 bg-neutral-900 accent-neutral-200"
                />
              </div>

              <div className="pt-2 flex justify-end">
                <button
                  type="button"
                  onClick={handleSaveBrowserSettings}
                  disabled={isSavingBrowser}
                  className="px-4 py-2 bg-neutral-100 hover:bg-white text-neutral-900 rounded-xl text-xs font-semibold transition disabled:opacity-50"
                >
                  {isSavingBrowser ? "Saving..." : "Save Browser Preferences"}
                </button>
              </div>
            </div>
          )}

          {/* TAB 3: TELEGRAM BRIDGE */}
          {activeTab === "telegram" && (
            <div className="space-y-4">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <TelegramIcon className="w-4 h-4 text-[#26A5E4]" /> Telegram Remote Notification Bridge
                </div>
                Enables long-polling Telegram notifications and human approval responses via Telegram bot commands. Only messages from the authorized Chat ID are executed.
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5 flex items-center gap-1.5">
                  <TelegramIcon className="w-3.5 h-3.5 text-[#26A5E4]" />
                  <span>Telegram Bot Token</span>
                </label>
                <div className="flex gap-2">
                  <input
                    type="password"
                    placeholder={
                      metadata.telegram_configured
                        ? "•••••••••••••••••••••••• (Configured in Vault)"
                        : "123456789:ABCdef..."
                    }
                    value={newTelegramToken}
                    onChange={(e) => setNewTelegramToken(e.target.value)}
                    className="flex-1 px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                  />
                  {metadata.telegram_configured && (
                    <button
                      onClick={() => setSecretPendingDeletion("telegram_bot_token")}
                      className="px-3 py-2 bg-red-950/40 hover:bg-red-900/60 text-red-300 border border-red-800/40 rounded-xl text-xs flex items-center gap-1 transition"
                    >
                      <Trash2 className="w-3.5 h-3.5" /> Purge
                    </button>
                  )}
                </div>
              </div>

              <div>
                <label className="block text-xs font-semibold text-neutral-200 mb-1.5">
                  Authorized Telegram User / Chat ID
                </label>
                <input
                  type="text"
                  placeholder="e.g. 987654321"
                  value={telegramChatId}
                  onChange={(e) => setTelegramChatId(e.target.value)}
                  className="w-full px-3.5 py-2.5 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                />
              </div>
            </div>
          )}

          {/* TAB 4: SANDBOX */}
          {activeTab === "sandbox" && (
            <div className="space-y-4">
              <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl text-xs space-y-2">
                <div className="font-semibold text-neutral-100">Local Container Engine Status</div>
                <div className="flex items-center gap-2">
                  <span
                    className={`w-2 h-2 rounded-full ${
                      sandbox?.available ? "bg-neutral-300" : "bg-red-400"
                    }`}
                  ></span>
                  <span className="font-mono text-neutral-300">
                    Engine: {sandbox?.engine?.toUpperCase() || "NONE"}
                  </span>
                </div>
                <div className="text-neutral-400">{sandbox?.message}</div>
              </div>
            </div>
          )}

          {/* TAB 5: TEMPLATES & ECOSYSTEM */}
          {activeTab === "templates" && (
            <div className="space-y-5">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Layers className="w-4 h-4 text-neutral-300" /> Extensible Template Packs & Export
                </div>
                Export or import custom bot profiles, Teach-a-Task skills, recurring routines, and MCP configs. Secrets and personal memories are always strictly sanitized.
              </div>

              {templateStatus && (
                <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-200 flex items-center gap-2">
                  <CheckCircle className="w-4 h-4 text-neutral-300 flex-shrink-0" />
                  <span>{templateStatus}</span>
                </div>
              )}

              {/* Export / Import Actions */}
              <div className="grid grid-cols-2 gap-3">
                <button
                  onClick={handleExportTemplates}
                  className="p-4 rounded-xl bg-[#141414] hover:bg-[#181818] border border-neutral-800 text-left transition flex items-start gap-3"
                >
                  <Download className="w-5 h-5 text-neutral-300 mt-0.5" />
                  <div>
                    <div className="text-xs font-semibold text-neutral-100">
                      Export Workspace Pack
                    </div>
                    <div className="text-[11px] text-neutral-400 mt-0.5">
                      Save bots, skills, and routines as portable JSON
                    </div>
                  </div>
                </button>

                <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 flex flex-col justify-between">
                  <div>
                    <div className="text-xs font-semibold text-neutral-100 flex items-center gap-2">
                      <Upload className="w-4 h-4 text-neutral-300" /> Import JSON Pack
                    </div>
                    <div className="text-[11px] text-neutral-400 mt-0.5">
                      Paste template pack JSON below to install
                    </div>
                  </div>
                </div>
              </div>

              {/* Paste JSON Import */}
              <div>
                <textarea
                  rows={3}
                  placeholder='Paste {"schema_version": 1, "bots": [...]} here...'
                  value={importJson}
                  onChange={(e) => setImportJson(e.target.value)}
                  className="w-full p-3 bg-[#000000] border border-neutral-800 rounded-xl text-xs text-neutral-200 font-mono placeholder-neutral-600 focus:outline-none focus:border-neutral-500"
                />
                <button
                  onClick={() => handleImportTemplates()}
                  disabled={!importJson.trim()}
                  className="mt-2 px-4 py-2 rounded-xl bg-neutral-100 hover:bg-white disabled:opacity-40 text-neutral-950 text-xs font-medium shadow-sm transition"
                >
                  Import Template Pack
                </button>
              </div>

              {/* Starter Template Library */}
              <div className="pt-2">
                <div className="text-xs font-semibold text-neutral-200 mb-2">
                  Built-in Starter Template Packs
                </div>
                <div className="space-y-2">
                  {starterTemplates.map((pack, idx) => (
                    <div
                      key={idx}
                      className="p-3 bg-[#141414] border border-neutral-800 rounded-xl flex items-center justify-between"
                    >
                      <div>
                        <div className="text-xs font-semibold text-neutral-100 flex items-center gap-2">
                          <FileCode className="w-4 h-4 text-neutral-400" />
                          <span>{pack.name}</span>
                        </div>
                        <div className="text-[11px] text-neutral-400 mt-0.5">{pack.description}</div>
                      </div>
                      <button
                        onClick={() => handleImportTemplates(JSON.stringify(pack))}
                        className="px-3 py-1.5 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-medium transition shadow-sm"
                      >
                        Install Pack
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          {/* TAB 6: DIAGNOSTICS & SYSTEM HEALTH */}
          {activeTab === "diagnostics" && (
            <div className="space-y-4">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Activity className="w-4 h-4 text-neutral-300" /> Real-Time Local Runtime Diagnostics
                </div>
                Probing local dependencies, database concurrency, display isolation, and CLI tools on this desktop machine.
              </div>

              {health ? (
                <div className="space-y-3">
                  {/* Core Services Grid */}
                  <div className="grid grid-cols-2 gap-3">
                    <div className="p-3 bg-[#141414] border border-neutral-800 rounded-xl space-y-1">
                      <div className="text-[10px] text-neutral-400 font-mono uppercase">
                        SQLite Database (WAL Mode)
                      </div>
                      <div className="flex items-center gap-2 text-xs font-semibold text-neutral-100">
                        <span
                          className={`w-2 h-2 rounded-full ${
                            health.db_connected ? "bg-neutral-300" : "bg-red-400"
                          }`}
                        />
                        <span>{health.db_connected ? "Connected & WAL Active" : "Disconnected"}</span>
                      </div>
                    </div>

                    <div className="p-3 bg-[#141414] border border-neutral-800 rounded-xl space-y-1">
                      <div className="text-[10px] text-neutral-400 font-mono uppercase">
                        Host Display Server
                      </div>
                      <div className="text-xs font-semibold text-neutral-100 font-mono">
                        {health.display_server}
                      </div>
                    </div>

                    <div className="p-3 bg-[#141414] border border-neutral-800 rounded-xl space-y-1">
                      <div className="text-[10px] text-neutral-400 font-mono uppercase">
                        Docker / Podman Sandbox
                      </div>
                      <div className="flex items-center gap-2 text-xs font-semibold text-neutral-100">
                        <span
                          className={`w-2 h-2 rounded-full ${
                            health.docker_available ? "bg-neutral-300" : "bg-amber-400"
                          }`}
                        />
                        <span>{health.docker_available ? "Available" : "Not Found / Inactive"}</span>
                      </div>
                    </div>

                    <div className="p-3 bg-[#141414] border border-neutral-800 rounded-xl space-y-1">
                      <div className="text-[10px] text-neutral-400 font-mono uppercase">
                        Ghost Mode Xvfb Virtual Display
                      </div>
                      <div className="flex items-center gap-2 text-xs font-semibold text-neutral-100">
                        <span
                          className={`w-2 h-2 rounded-full ${
                            health.ghost_xvfb_available ? "bg-neutral-300" : "bg-amber-400"
                          }`}
                        />
                        <span>{health.ghost_xvfb_available ? "Xvfb Binary Ready (:99)" : "Missing Xvfb"}</span>
                      </div>
                    </div>
                  </div>

                  {/* CLI Tools Availability */}
                  <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-2">
                    <div className="text-xs font-semibold text-neutral-200">
                      Installed CLI System Tools
                    </div>
                    <div className="grid grid-cols-3 gap-2">
                      {Object.entries(health.tools || {}).map(([tool, installed]) => (
                        <div
                          key={tool}
                          className="px-2.5 py-1.5 rounded-lg bg-[#000000] border border-neutral-800 flex items-center justify-between text-xs font-mono"
                        >
                          <span className="text-neutral-300">{tool}</span>
                          <span
                            className={`text-[10px] px-1.5 py-0.2 rounded font-medium ${
                              installed
                                ? "bg-neutral-800 text-neutral-200 border border-neutral-700"
                                : "bg-neutral-900 text-neutral-500"
                            }`}
                          >
                            {installed ? "Ready" : "Missing"}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              ) : (
                <div className="p-6 text-center text-xs text-neutral-500">
                  Loading diagnostics...
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-3.5 border-t border-neutral-800 bg-[#141414] flex items-center justify-between">
          <span className="text-xs text-neutral-300 font-medium">
            {isSaved ? "Settings updated successfully." : ""}
          </span>
          <div className="flex items-center gap-3">
            <button
              onClick={onClose}
              className="px-4 py-2 text-xs font-medium text-neutral-400 hover:text-neutral-100 rounded-xl hover:bg-neutral-800 transition"
            >
              Close
            </button>
            <button
              onClick={handleSave}
              disabled={loading}
              className="px-5 py-2 text-xs font-medium bg-neutral-100 hover:bg-white text-neutral-950 rounded-xl shadow-sm transition"
            >
              {loading ? "Saving..." : "Save Settings"}
            </button>
          </div>
        </div>
      </div>

      <ConfirmDialog
        isOpen={secretPendingDeletion !== null}
        title="Remove this stored key?"
        message={
          secretPendingDeletion
            ? `The ${secretPendingDeletion} key will be purged from the secure vault. Kyro will not be able to use it until you enter it again.`
            : ""
        }
        confirmLabel="Purge key"
        destructive
        isBusy={isDeletingSecret}
        onCancel={() => setSecretPendingDeletion(null)}
        onConfirm={() => {
          if (secretPendingDeletion) handleDeleteSecret(secretPendingDeletion);
        }}
      />
    </div>
  );
};
