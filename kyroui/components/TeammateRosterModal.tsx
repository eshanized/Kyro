import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BotRecord, BotStatus } from "../types/agent";
import { Users, Plus, Trash2, X, Sparkles, Check, Activity, Bot, Zap, Wrench, Globe, Terminal, Code, Cpu, Shield, User } from "lucide-react";
import { AgentAvatar } from "./AgentAvatar";
import { ConfirmDialog } from "./ConfirmDialog";
import { InlineNotice } from "./InlineNotice";

interface TeammateRosterModalProps {
  isOpen: boolean;
  onClose: () => void;
  selectedBotId: string;
  onSelectBot: (bot: BotRecord) => void;
}

export const TeammateRosterModal: React.FC<TeammateRosterModalProps> = ({
  isOpen,
  onClose,
  selectedBotId,
  onSelectBot,
}) => {
  const [bots, setBots] = useState<BotRecord[]>([]);
  const [statuses, setStatuses] = useState<Record<string, BotStatus>>({});
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [botPendingDeletion, setBotPendingDeletion] = useState<BotRecord | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [name, setName] = useState("");
  const [avatar, setAvatar] = useState("bot");
  const [role, setRole] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");

  useEffect(() => {
    if (isOpen) {
      loadBotsAndStatuses();
      const interval = setInterval(loadStatuses, 2000);
      return () => clearInterval(interval);
    }
  }, [isOpen]);

  const loadBotsAndStatuses = async () => {
    try {
      const list = await invoke<BotRecord[]>("list_bots");
      setBots(list);
      await loadStatuses();
    } catch (e) {
      console.error(e);
    }
  };

  const loadStatuses = async () => {
    try {
      const roster = await invoke<BotStatus[]>("get_roster_statuses");
      const map: Record<string, BotStatus> = {};
      for (const s of roster) {
        map[s.id] = s;
      }
      setStatuses(map);
    } catch (e) {
      // ignore
    }
  };

  const handleCreate = async () => {
    if (!name.trim() || !role.trim()) return;
    try {
      const id = `bot-${Date.now()}`;
      await invoke("create_bot", {
        bot: {
          id,
          name,
          avatar: avatar || "bot",
          role,
          system_prompt: systemPrompt || `You are ${name}, a specialized assistant in Kyro.`,
          is_default: false,
        },
      });
      setIsCreating(false);
      setName("");
      setRole("");
      setSystemPrompt("");
      await loadBotsAndStatuses();
      setError(null);
    } catch (err: any) {
      setError("Failed to create bot: " + err);
    }
  };

  const handleDelete = async (id: string) => {
    setIsDeleting(true);
    try {
      await invoke("delete_bot", { id });
      // The row is only removed once the backend confirmed the deletion.
      setBots((prev) => prev.filter((b) => b.id !== id));
      setError(null);
    } catch (err: any) {
      setError("Failed to delete bot: " + err);
    } finally {
      setIsDeleting(false);
      setBotPendingDeletion(null);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4">
      <div className="w-full max-w-2xl bg-[#111111] border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-neutral-800 bg-[#141414]">
          <div className="flex items-center gap-2.5">
            <Users className="w-5 h-5 text-neutral-400" />
            <h2 className="text-base font-semibold text-neutral-100">Teammate Roster & Multi-Agent State</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
            <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
              <Sparkles className="w-4 h-4 text-neutral-400" /> Multi-Agent Orchestration
            </div>
            Specialized teammates operate with bounded context, parent/child execution hierarchies, and distinct memory scopes. Mentioning <code>@Forge</code> or calling <code>delegate_task</code> activates specialized execution.
          </div>

          <div className="flex items-center justify-between">
            <span className="text-xs text-neutral-400 font-medium">Configured AI Teammates</span>
            <button
              onClick={() => setIsCreating(!isCreating)}
              className="px-3 py-1.5 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-medium flex items-center gap-1.5 shadow-sm transition"
            >
              <Plus className="w-3.5 h-3.5" /> {isCreating ? "Cancel" : "Add Teammate"}
            </button>
          </div>

          {/* Create Teammate Form */}
          {isCreating && (
            <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-3">
              <div className="text-xs font-semibold text-neutral-100">New Teammate Profile</div>
              <div>
                <label className="block text-[11px] text-neutral-400 mb-1.5 font-medium">Teammate Icon</label>
                <div className="flex items-center gap-1.5 flex-wrap">
                  {[
                    { key: "bot", label: "Bot", Icon: Bot },
                    { key: "zap", label: "Apex / Fast", Icon: Zap },
                    { key: "wrench", label: "DevOps / Forge", Icon: Wrench },
                    { key: "globe", label: "Web / Scout", Icon: Globe },
                    { key: "terminal", label: "Terminal", Icon: Terminal },
                    { key: "code", label: "Code", Icon: Code },
                    { key: "cpu", label: "Core", Icon: Cpu },
                    { key: "shield", label: "Security", Icon: Shield },
                    { key: "user", label: "User", Icon: User },
                  ].map(({ key, label, Icon }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => setAvatar(key)}
                      title={label}
                      className={`p-2 rounded-lg border transition ${
                        avatar === key
                          ? "bg-neutral-800 text-neutral-100 border-neutral-600 shadow"
                          : "bg-black/60 text-neutral-400 border-neutral-800 hover:text-neutral-200"
                      }`}
                    >
                      <Icon className="w-4 h-4" />
                    </button>
                  ))}
                </div>
              </div>
              <input
                type="text"
                placeholder="Name (e.g. Sentinel)"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
              />
              <input
                type="text"
                placeholder="Role (e.g. Code Review & Static Analysis Specialist)"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
              />
              <textarea
                placeholder="Custom System Prompt instructions..."
                rows={3}
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 resize-none"
              />
              <button
                onClick={handleCreate}
                className="px-4 py-2 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-medium shadow-sm transition"
              >
                Save Teammate
              </button>
            </div>
          )}

          {/* Teammates List */}
          <div className="grid grid-cols-1 gap-3">
            {bots.map((b) => {
              const liveStatus = statuses[b.id];
              const isBusy = liveStatus?.state === "busy";

              return (
                <div
                  key={b.id}
                  onClick={() => onSelectBot(b)}
                  className={`p-4 rounded-xl border flex flex-col gap-2 cursor-pointer transition ${
                    selectedBotId === b.id
                      ? "bg-neutral-800 text-neutral-100 border-neutral-700 shadow-sm"
                      : "bg-[#141414] border-neutral-800 hover:border-neutral-700"
                  }`}
                >
                  <div className="flex items-center justify-between gap-4">
                    <div className="flex items-center gap-3.5 min-w-0">
                      <AgentAvatar
                        avatar={b.avatar}
                        name={b.name}
                        className="w-10 h-10 rounded-xl bg-black/60 border border-neutral-800 flex items-center justify-center text-neutral-200 shadow-sm"
                        iconClassName="w-5 h-5"
                      />
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-semibold text-neutral-100">@{b.name}</span>
                          {b.is_default && (
                            <span className="text-[10px] uppercase font-medium px-1.5 py-0.2 rounded bg-neutral-800 text-neutral-300 border border-neutral-700">
                              Core
                            </span>
                          )}
                          <span
                            className={`text-[10px] px-2 py-0.5 rounded-full flex items-center gap-1 font-medium ${
                              isBusy
                                ? "bg-amber-950/40 text-amber-300 border border-amber-800/60 animate-pulse"
                                : "bg-neutral-800 text-neutral-300 border border-neutral-700"
                            }`}
                          >
                            <span
                              className={`w-1.5 h-1.5 rounded-full ${
                                isBusy ? "bg-amber-400" : "bg-neutral-400"
                              }`}
                            ></span>
                            {isBusy ? "Active Execution" : "Idle / Standby"}
                          </span>
                        </div>
                        <div className="text-xs text-neutral-400 mt-0.5">{b.role}</div>
                      </div>
                    </div>

                    <div className="flex items-center gap-2">
                      {selectedBotId === b.id && (
                        <span className="text-xs text-neutral-200 font-medium flex items-center gap-1">
                          <Check className="w-3.5 h-3.5" /> Selected
                        </span>
                      )}
                      {!b.is_default && (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setBotPendingDeletion(b);
                          }}
                          aria-label={`Remove teammate ${b.name}`}
                          className="p-1.5 text-neutral-500 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>

                  {liveStatus?.current_task && (
                    <div className="mt-2 text-xs p-2 rounded-lg bg-black/40 border border-amber-500/30 text-amber-200/90 flex items-center gap-2">
                      <Activity className="w-3.5 h-3.5 text-amber-400 animate-spin shrink-0" />
                      <span className="truncate">Task: {liveStatus.current_task}</span>
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {error && (
            <div className="px-4 py-3 border-t border-neutral-800">
              <InlineNotice tone="error" message={error} onDismiss={() => setError(null)} />
            </div>
          )}
        </div>
      </div>

      <ConfirmDialog
        isOpen={botPendingDeletion !== null}
        title="Remove this teammate?"
        message={
          botPendingDeletion
            ? `"${botPendingDeletion.name}" will be removed from the roster and can no longer receive delegated tasks. This cannot be undone.`
            : ""
        }
        confirmLabel="Remove teammate"
        destructive
        isBusy={isDeleting}
        onCancel={() => setBotPendingDeletion(null)}
        onConfirm={() => {
          if (botPendingDeletion) handleDelete(botPendingDeletion.id);
        }}
      />
    </div>
  );
};
