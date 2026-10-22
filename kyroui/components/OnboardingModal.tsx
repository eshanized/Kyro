import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Sparkles,
  FolderOpen,
  Plug,
  Shield,
  CheckCircle2,
  ArrowRight,
  ArrowLeft,
  X,
  FileText,
  Camera,
  Key,
  Plus,
} from "lucide-react";
import { KyroLogo } from "./Logo";
import { SuggestedFolder } from "../types/agent";

interface OnboardingModalProps {
  isOpen: boolean;
  onClose: () => void;
  onTryExample: (prompt: string) => void;
  onOpenConnections: () => void;
}

export const OnboardingModal: React.FC<OnboardingModalProps> = ({
  isOpen,
  onClose,
  onTryExample,
  onOpenConnections,
}) => {
  const [step, setStep] = useState(1);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [suggestedFolders, setSuggestedFolders] = useState<SuggestedFolder[]>([]);
  const [selectedFolders, setSelectedFolders] = useState<{ path: string; name: string }[]>([]);
  const [customPath, setCustomPath] = useState("");
  const [customLabel, setCustomLabel] = useState("");
  const [isSavingKey, setIsSavingKey] = useState(false);
  const [isSavingFolders, setIsSavingFolders] = useState(false);
  const [folderNotice, setFolderNotice] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      const fetchSuggestions = async () => {
        try {
          const list = await invoke<SuggestedFolder[]>("get_suggested_folders");
          if (Array.isArray(list)) {
            setSuggestedFolders(list);
            const defaultSelected = list
              .filter((f) => f && f.exists && (f.id === "downloads" || f.id === "documents"))
              .map((f) => ({ path: f.path, name: f.name }));
            setSelectedFolders(defaultSelected);
          } else {
            setSuggestedFolders([]);
          }
        } catch {
          setSuggestedFolders([]);
        }
      };
      fetchSuggestions();
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleFinish = () => {
    try {
      localStorage.setItem("kyro_onboarding_completed", "true");
    } catch {
      // ignore
    }
    onClose();
  };

  const handleSaveApiKey = async () => {
    if (!apiKeyInput.trim()) {
      setStep(3);
      return;
    }
    setIsSavingKey(true);
    try {
      await invoke("save_api_keys", {
        keys: {
          nvidia: apiKeyInput.trim(),
        },
      });
    } catch {
      // ignore
    } finally {
      setIsSavingKey(false);
      setStep(3);
    }
  };

  const handleToggleFolder = (folder: { path: string; name: string }) => {
    setSelectedFolders((prev) =>
      prev.some((f) => f.path === folder.path)
        ? prev.filter((f) => f.path !== folder.path)
        : [...prev, folder]
    );
  };

  const handleAddCustomFolder = () => {
    const trimmed = customPath.trim();
    if (!trimmed) return;
    if (selectedFolders.some((f) => f.path === trimmed)) {
      setFolderNotice("Folder is already selected.");
      return;
    }
    const name = customLabel.trim() || trimmed.split("/").filter(Boolean).pop() || "Custom Folder";
    setSelectedFolders([...selectedFolders, { path: trimmed, name }]);
    setCustomPath("");
    setCustomLabel("");
    setFolderNotice(null);
  };

  const handleSaveAuthorizedFolders = async () => {
    if (selectedFolders.length === 0) {
      setStep(4);
      return;
    }
    setIsSavingFolders(true);
    setFolderNotice(null);
    try {
      for (const folder of selectedFolders) {
        await invoke("add_authorized_folder", {
          path: folder.path,
          displayName: folder.name,
        });
      }
      setStep(4);
    } catch (e: any) {
      setFolderNotice(`Failed to authorize folder: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsSavingFolders(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 animate-in fade-in">
      <div className="w-full max-w-2xl bg-[#121212] border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col text-neutral-100">
        {/* Progress Bar & Header */}
        <div className="px-6 pt-5 pb-3 border-b border-neutral-800/80 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <KyroLogo className="w-6 h-6 text-neutral-100" />
            <div>
              <span className="text-xs font-semibold text-neutral-300">Getting Started with Kyro</span>
              <span className="text-[10px] text-neutral-400 block">Step {step} of 6</span>
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            {[1, 2, 3, 4, 5, 6].map((i) => (
              <div
                key={i}
                className={`h-1.5 rounded-full transition-all ${
                  i === step
                    ? "w-6 bg-purple-500"
                    : i < step
                    ? "w-2 bg-neutral-200"
                    : "w-2 bg-neutral-800"
                }`}
              />
            ))}
            <button
              onClick={handleFinish}
              className="text-neutral-400 hover:text-neutral-200 p-1 ml-2 transition"
              title="Skip onboarding"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Content Body */}
        <div className="p-6 flex-1 min-h-[320px] flex flex-col justify-between">
          {/* STEP 1: WELCOME & OVERVIEW */}
          {step === 1 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-purple-950/50 border border-purple-800/60 text-purple-400 flex items-center justify-center">
                <Sparkles className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">Your Everyday Personal AI Assistant</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  Kyro helps you get things done on your computer without needing to understand programming,
                  agents, or technical configuration.
                </p>
              </div>

              <div className="grid grid-cols-2 gap-2.5 pt-2 text-xs">
                <div className="p-3 rounded-xl bg-neutral-900 border border-neutral-800 space-y-1">
                  <div className="font-semibold text-neutral-200 flex items-center gap-1.5">
                    <FolderOpen className="w-4 h-4 text-amber-400" /> Personal Files
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    Find invoices, clean duplicate files, and categorize downloads safely.
                  </p>
                </div>
                <div className="p-3 rounded-xl bg-neutral-900 border border-neutral-800 space-y-1">
                  <div className="font-semibold text-neutral-200 flex items-center gap-1.5">
                    <FileText className="w-4 h-4 text-purple-400" /> Document Study
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    Explain PDFs in simple English, extract key dates, and generate revision notes.
                  </p>
                </div>
                <div className="p-3 rounded-xl bg-neutral-900 border border-neutral-800 space-y-1">
                  <div className="font-semibold text-neutral-200 flex items-center gap-1.5">
                    <Plug className="w-4 h-4 text-neutral-300" /> Email & Schedule
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    Summarize unread messages, prepare draft replies, and check upcoming events.
                  </p>
                </div>
                <div className="p-3 rounded-xl bg-neutral-900 border border-neutral-800 space-y-1">
                  <div className="font-semibold text-neutral-200 flex items-center gap-1.5">
                    <Camera className="w-4 h-4 text-sky-400" /> Screen & Web
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    Diagnose visual error dialogs, compare products, and automate repetitive tasks.
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* STEP 2: CONFIGURE AI PROVIDER */}
          {step === 2 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-amber-950/40 border border-amber-800/50 text-amber-400 flex items-center justify-center">
                <Key className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">AI Model Setup</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  Kyro is powered by NVIDIA Nemotron-3 Ultra (or compatible OpenAI/Anthropic models). Enter your
                  API key if you have one, or skip to continue with local capabilities.
                </p>
              </div>

              <div className="space-y-2 pt-2">
                <label className="text-xs font-medium text-neutral-300">
                  NVIDIA API Key (or provider key):
                </label>
                <input
                  type="password"
                  value={apiKeyInput}
                  onChange={(e) => setApiKeyInput(e.target.value)}
                  placeholder="nvapi-..."
                  className="w-full px-3.5 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 placeholder-neutral-400 focus:outline-none focus:border-purple-600 font-mono"
                />
                <p className="text-[11px] text-neutral-400">
                  Keys are stored encrypted inside your local protected vault and never shared.
                </p>
              </div>
            </div>
          )}

          {/* STEP 3: CHOOSE AUTHORIZED FOLDERS */}
          {step === 3 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-amber-950/50 border border-amber-800/60 text-amber-400 flex items-center justify-center">
                <FolderOpen className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">Choose Authorized Folders</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  Kyro follows local-first security principles and will only search, summarize, or organize
                  files inside the specific directories you explicitly authorize.
                </p>
              </div>

              {folderNotice && (
                <div className="p-2.5 rounded-lg bg-rose-950/40 border border-rose-800/60 text-rose-200 text-xs">
                  {folderNotice}
                </div>
              )}

              <div className="space-y-3 pt-1">
                <span className="text-xs font-semibold text-neutral-300">Suggested Standard Locations:</span>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                  {(suggestedFolders || []).map((f) => {
                    const isSelected = selectedFolders.some((s) => s.path === f.path);
                    return (
                      <button
                        key={f.id}
                        type="button"
                        onClick={() => handleToggleFolder({ path: f.path, name: f.name })}
                        className={`p-3 rounded-xl border text-left text-xs font-medium transition flex flex-col justify-between gap-1.5 ${
                          isSelected
                            ? "bg-amber-950/40 border-amber-800/70 text-amber-200"
                            : "bg-neutral-900 border-neutral-800 text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        <div className="flex items-center justify-between w-full">
                          <span className="font-semibold text-neutral-200">{f.name}</span>
                          {isSelected && <CheckCircle2 className="w-4 h-4 text-amber-400" />}
                        </div>
                        <code className="text-[10px] text-neutral-400 font-mono truncate max-w-full">
                          {f.path}
                        </code>
                      </button>
                    );
                  })}
                </div>

                {/* Add Custom Directory */}
                <div className="pt-2 border-t border-neutral-800/80 space-y-2">
                  <span className="text-xs font-semibold text-neutral-300">Add Custom Directory:</span>
                  <div className="flex flex-col sm:flex-row gap-2">
                    <input
                      type="text"
                      value={customPath}
                      onChange={(e) => setCustomPath(e.target.value)}
                      placeholder="Absolute directory path (e.g. /home/user/Work)..."
                      className="flex-1 px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 placeholder-neutral-400 focus:outline-none"
                    />
                    <input
                      type="text"
                      value={customLabel}
                      onChange={(e) => setCustomLabel(e.target.value)}
                      placeholder="Label (e.g. College / Work)"
                      className="w-full sm:w-36 px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 placeholder-neutral-400 focus:outline-none"
                    />
                    <button
                      type="button"
                      onClick={handleAddCustomFolder}
                      className="px-3 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition"
                    >
                      <Plus className="w-3.5 h-3.5" /> Add
                    </button>
                  </div>
                </div>

                {/* Active Authorized Confirmation Box */}
                <div className="p-3 rounded-xl bg-neutral-900/80 border border-neutral-800 text-[11px] text-neutral-400 space-y-1">
                  <div className="font-semibold text-neutral-200 flex items-center gap-1.5">
                    <Shield className="w-3.5 h-3.5 text-amber-400" />
                    <span>Exact Directories to Authorize ({selectedFolders.length}):</span>
                  </div>
                  {selectedFolders.length > 0 ? (
                    <ul className="list-disc list-inside space-y-0.5 text-neutral-300 font-mono text-[10px]">
                      {selectedFolders.map((sf, idx) => (
                        <li key={idx} className="truncate">
                          {sf.name}: {sf.path}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="italic text-neutral-400">
                      No folders selected. Kyro will not access any personal files without authorization.
                    </p>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* STEP 4: CONNECT OPTIONAL SERVICES */}
          {step === 4 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-sky-950/50 border border-sky-800/60 text-sky-400 flex items-center justify-center">
                <Plug className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">Optional Connected Services</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  You can connect Google Workspace (Gmail & Calendar) or GitHub if you want email summaries and
                  calendar help. All local document & computer capabilities work 100% without them!
                </p>
              </div>

              <div className="space-y-2 pt-2">
                <div className="p-3.5 rounded-xl bg-neutral-900 border border-neutral-800 flex items-center justify-between">
                  <div>
                    <span className="text-xs font-semibold text-neutral-200 block">Google Workspace</span>
                    <span className="text-[11px] text-neutral-400">Gmail unread summaries & Calendar agenda</span>
                  </div>
                  <button
                    onClick={onOpenConnections}
                    className="px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-xs font-medium text-neutral-200 rounded-lg transition"
                  >
                    Connect
                  </button>
                </div>

                <div className="p-3.5 rounded-xl bg-neutral-900 border border-neutral-800 flex items-center justify-between">
                  <div>
                    <span className="text-xs font-semibold text-neutral-200 block">GitHub</span>
                    <span className="text-[11px] text-neutral-400">Repositories, issues & workflows</span>
                  </div>
                  <button
                    onClick={onOpenConnections}
                    className="px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-xs font-medium text-neutral-200 rounded-lg transition"
                  >
                    Connect
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* STEP 5: PRIVACY & PERMISSIONS */}
          {step === 5 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-neutral-900 border border-neutral-700 text-neutral-200 flex items-center justify-center">
                <Shield className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">Privacy & Safety Principles</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  Kyro is built with guardrails so you always stay in control of your computer.
                </p>
              </div>

              <div className="space-y-2.5 pt-2 text-xs">
                <div className="flex items-start gap-2.5 p-2.5 rounded-lg bg-neutral-900 border border-neutral-800">
                  <CheckCircle2 className="w-4 h-4 text-neutral-300 shrink-0 mt-0.5" />
                  <div>
                    <span className="font-semibold text-neutral-200 block">Local Data Stays Local</span>
                    <span className="text-neutral-400 text-[11px]">
                      Your documents, transactions, and search indexes are stored locally in SQLite on your disk.
                    </span>
                  </div>
                </div>

                <div className="flex items-start gap-2.5 p-2.5 rounded-lg bg-neutral-900 border border-neutral-800">
                  <CheckCircle2 className="w-4 h-4 text-neutral-300 shrink-0 mt-0.5" />
                  <div>
                    <span className="font-semibold text-neutral-200 block">Approval for Consequential Actions</span>
                    <span className="text-neutral-400 text-[11px]">
                      Sending emails, deleting files, or moving folders requires your explicit review and confirmation.
                    </span>
                  </div>
                </div>

                <div className="flex items-start gap-2.5 p-2.5 rounded-lg bg-neutral-900 border border-neutral-800">
                  <CheckCircle2 className="w-4 h-4 text-neutral-300 shrink-0 mt-0.5" />
                  <div>
                    <span className="font-semibold text-neutral-200 block">Reversible File Operations</span>
                    <span className="text-neutral-400 text-[11px]">
                      Organized folders keep an audit ledger and can be undone with one click.
                    </span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* STEP 6: SAFE EXAMPLE TRY-OUT */}
          {step === 6 && (
            <div className="space-y-4">
              <div className="p-3 w-12 h-12 rounded-2xl bg-purple-950/50 border border-purple-800/60 text-purple-400 flex items-center justify-center">
                <Sparkles className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-neutral-100">Ready to Get Started!</h2>
                <p className="text-xs text-neutral-400 mt-1 leading-relaxed">
                  Try one of these safe example tasks, or type any request you have in the Ask Kyro prompt.
                </p>
              </div>

              <div className="space-y-2 pt-2">
                <button
                  onClick={() => {
                    handleFinish();
                    onTryExample("Find the invoice I downloaded last month and extract the total amount");
                  }}
                  className="w-full p-3 rounded-xl bg-neutral-900 hover:bg-neutral-800 border border-neutral-800 text-left transition flex items-center justify-between group"
                >
                  <div>
                    <span className="text-xs font-semibold text-neutral-200 group-hover:text-white block">
                      Find an invoice or document
                    </span>
                    <span className="text-[11px] text-neutral-400">
                      "Find the invoice I downloaded last month and extract the total amount"
                    </span>
                  </div>
                  <ArrowRight className="w-4 h-4 text-neutral-400 group-hover:text-white transition" />
                </button>

                <button
                  onClick={() => {
                    handleFinish();
                    onTryExample("Organize my downloads into category folders and check for duplicate files");
                  }}
                  className="w-full p-3 rounded-xl bg-neutral-900 hover:bg-neutral-800 border border-neutral-800 text-left transition flex items-center justify-between group"
                >
                  <div>
                    <span className="text-xs font-semibold text-neutral-200 group-hover:text-white block">
                      Organize downloaded files
                    </span>
                    <span className="text-[11px] text-neutral-400">
                      "Organize my downloads into category folders and check for duplicate files"
                    </span>
                  </div>
                  <ArrowRight className="w-4 h-4 text-neutral-400 group-hover:text-white transition" />
                </button>

                <button
                  onClick={() => {
                    handleFinish();
                    onTryExample("Explain the error on my screen and help me fix it");
                  }}
                  className="w-full p-3 rounded-xl bg-neutral-900 hover:bg-neutral-800 border border-neutral-800 text-left transition flex items-center justify-between group"
                >
                  <div>
                    <span className="text-xs font-semibold text-neutral-200 group-hover:text-white block">
                      Explain my screen
                    </span>
                    <span className="text-[11px] text-neutral-400">
                      "Explain the error on my screen and help me fix it"
                    </span>
                  </div>
                  <ArrowRight className="w-4 h-4 text-neutral-400 group-hover:text-white transition" />
                </button>
              </div>
            </div>
          )}

          {/* Bottom Buttons */}
          <div className="pt-4 border-t border-neutral-800/80 flex items-center justify-between">
            {step > 1 ? (
              <button
                onClick={() => setStep(step - 1)}
                className="px-3.5 py-1.5 rounded-lg text-xs font-medium text-neutral-400 hover:text-neutral-200 flex items-center gap-1.5 transition"
              >
                <ArrowLeft className="w-3.5 h-3.5" /> Back
              </button>
            ) : (
              <button
                onClick={handleFinish}
                className="text-xs text-neutral-400 hover:text-neutral-200 transition"
              >
                Skip Tutorial
              </button>
            )}

            {step < 6 ? (
              <button
                onClick={() => {
                  if (step === 2) {
                    handleSaveApiKey();
                  } else if (step === 3) {
                    handleSaveAuthorizedFolders();
                  } else {
                    setStep(step + 1);
                  }
                }}
                disabled={isSavingKey || isSavingFolders}
                className="px-4 py-2 bg-purple-950/60 hover:bg-purple-900/60 border border-purple-700/60 rounded-lg text-xs font-semibold text-purple-200 flex items-center gap-1.5 transition shadow-sm disabled:opacity-50"
              >
                {isSavingFolders ? "Saving Permissions..." : "Continue"} <ArrowRight className="w-3.5 h-3.5" />
              </button>
            ) : (
              <button
                onClick={handleFinish}
                className="px-4 py-2 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition shadow-sm"
              >
                <CheckCircle2 className="w-3.5 h-3.5" /> Get Started
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
