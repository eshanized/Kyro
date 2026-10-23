import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AgentProfileRecord,
  AgentProfileDocument,
  CreateAgentProfileRequest,
  ProfileOrigin,
  BotStatus,
} from "../types/agent";
import {
  Users,
  Plus,
  Trash2,
  X,
  Sparkles,
  Check,
  Activity,
  Bot,
  Zap,
  Wrench,
  Globe,
  Terminal,
  Code,
  Cpu,
  Shield,
  User,
  Pencil,
  RotateCcw,
  Ban,
  Power,
  FileCode2,
} from "lucide-react";
import { AgentAvatar } from "./AgentAvatar";
import { ConfirmDialog } from "./ConfirmDialog";
import { InlineNotice } from "./InlineNotice";

interface TeammateRosterModalProps {
  isOpen: boolean;
  onClose: () => void;
  /** Selected teammate indicates the active profile id (AgentProfileRecord.id). */
  selectedBotId: string;
  onSelectBot: (bot: AgentProfileRecord) => void;
}

const AVATARS = [
  { key: "bot", label: "Bot", Icon: Bot },
  { key: "zap", label: "Apex / Fast", Icon: Zap },
  { key: "wrench", label: "DevOps / Forge", Icon: Wrench },
  { key: "globe", label: "Web / Scout", Icon: Globe },
  { key: "terminal", label: "Terminal", Icon: Terminal },
  { key: "code", label: "Code", Icon: Code },
  { key: "cpu", label: "Core", Icon: Cpu },
  { key: "shield", label: "Security", Icon: Shield },
  { key: "user", label: "User", Icon: User },
] as const;

const AVATAR_ICONS: Record<string, React.ComponentType<{ className?: string }>> = Object.fromEntries(
  AVATARS.map(({ key, Icon }) => [key, Icon]),
);

const ORIGIN_LABELS: Record<ProfileOrigin, { label: string; tone: string }> = {
  builtin: {
    label: "Shipped default",
    tone: "bg-neutral-800 text-neutral-300 border border-neutral-700",
  },
  override: {
    label: "Overridden",
    tone: "bg-purple-950/50 text-purple-300 border border-purple-800/60",
  },
  custom: {
    label: "Custom",
    tone: "bg-sky-950/50 text-sky-300 border border-sky-800/60",
  },
};

export const TeammateRosterModal: React.FC<TeammateRosterModalProps> = ({
  isOpen,
  onClose,
  selectedBotId,
  onSelectBot,
}) => {
  const [profiles, setProfiles] = useState<AgentProfileRecord[]>([]);
  const [statuses, setStatuses] = useState<Record<string, BotStatus>>({});
  const [isCreating, setIsCreating] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [editorProfile, setEditorProfile] = useState<AgentProfileRecord | null>(null);
  const [editorDocument, setEditorDocument] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [profilePendingDeletion, setProfilePendingDeletion] = useState<AgentProfileRecord | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Create-teammate form state
  const [formId, setFormId] = useState("");
  const [formName, setFormName] = useState("");
  const [formAvatar, setFormAvatar] = useState("bot");
  const [formRole, setFormRole] = useState("");
  const [formDescription, setFormDescription] = useState("");
  const [formInstructions, setFormInstructions] = useState("");

  useEffect(() => {
    if (isOpen) {
      loadProfiles().catch(console.error);
      const interval = setInterval(() => loadStatuses().catch(() => {}), 2000);
      return () => clearInterval(interval);
    }
  }, [isOpen]);

  const loadProfiles = async () => {
    const list = await invoke<AgentProfileRecord[]>("list_agent_profiles");
    // Invalid/flagged entries can never be selected but stay visible for repair.
    setProfiles(Array.isArray(list) ? list : []);
    await loadStatuses();
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
      // Transient status fetch failures are non-fatal.
    }
  };

  const handleCreate = async () => {
    const request: CreateAgentProfileRequest = {
      id: formId,
      name: formName,
      avatar: formAvatar,
      role: formRole,
      description: formDescription,
      instructions: formInstructions,
    };
    if (!request.name.trim() || !request.role.trim()) return;
    try {
      const created = await invoke<AgentProfileRecord>("create_agent_profile", {
        request,
      });
      setIsCreating(false);
      setFormId("");
      setFormName("");
      setFormAvatar("bot");
      setFormRole("");
      setFormDescription("");
      setFormInstructions("");
      await loadProfiles();
      onSelectBot(created);
      setError(null);
    } catch (err: any) {
      setError("Failed to create teammate: " + err);
    }
  };

  const handleToggleEnabled = async (p: AgentProfileRecord) => {
    try {
      await invoke("set_agent_profile_enabled", { id: p.id, enabled: !p.enabled });
      await loadProfiles();
      setError(null);
    } catch (err: any) {
      setError("Failed to toggle " + p.name + ": " + err);
    }
  };

  const handleDelete = async (id: string) => {
    setIsDeleting(true);
    try {
      await invoke("delete_agent_profile", { id });
      // Refresh from the registry: deleting an override falls back to the
      // bundled default, deleting a custom profile removes it entirely.
      await loadProfiles();
      setError(null);
    } catch (err: any) {
      setError("Failed to delete teammate: " + err);
    } finally {
      setIsDeleting(false);
      setProfilePendingDeletion(null);
      setExpandedId(null);
    }
  };

  const handleReset = async (p: AgentProfileRecord) => {
    try {
      const record = await invoke<AgentProfileRecord>("reset_agent_profile", { id: p.id });
      await loadProfiles();
      setExpandedId(null);
      onSelectBot(record);
      if (editorProfile?.id === p.id) closeEditor();
      setError(null);
    } catch (err: any) {
      setError("Failed to reset " + p.name + ": " + err);
    }
  };

  const openEditor = async (p: AgentProfileRecord) => {
    try {
      const doc: AgentProfileDocument = await invoke("get_agent_profile_document", {
        id: p.id,
      });
      setEditorProfile(p);
      setEditorDocument(doc.document);
      setError(null);
    } catch (err: any) {
      setError("Failed to open manifest for " + p.name + ": " + err);
    }
  };

  const closeEditor = () => {
    setEditorProfile(null);
    setEditorDocument("");
  };

  const handleSaveDocument = async () => {
    if (!editorProfile) return;
    setIsSaving(true);
    try {
      const record: AgentProfileRecord = await invoke("save_agent_profile", {
        id: editorProfile.id,
        document: editorDocument,
      });
      await loadProfiles();
      setExpandedId(null);
      onSelectBot(record);
      closeEditor();
      setError(null);
    } catch (err: any) {
      // The backend never activates an invalid save; surface the validation.
      setError("Save rejected — previous manifest still active: " + err);
    } finally {
      setIsSaving(false);
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
            <h2 className="text-base font-semibold text-neutral-100">Teammate Roster & Agent Profiles</h2>
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
              <Sparkles className="w-4 h-4 text-neutral-400" /> Profile-Driven Agents
            </div>
            Teammates are Markdown agent profiles resolved by the runtime — prompts in chat can
            never override them. Each profile carries its own name, role, tool policy, delegation
            limits and instruction body. Mentioning <code>@Forge</code> or calling <code>delegate_task</code> activates a
            specialist.
          </div>

          <div className="flex items-center justify-between">
            <span className="text-xs text-neutral-400 font-medium">Agent Profiles</span>
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
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-[11px] text-neutral-400 mb-1.5 font-medium">Profile Id</label>
                  <input
                    type="text"
                    placeholder="sentinel-reviewer"
                    value={formId}
                    onChange={(e) => setFormId(e.target.value)}
                    className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 font-mono"
                  />
                </div>
                <div>
                  <label className="block text-[11px] text-neutral-400 mb-1.5 font-medium">Name</label>
                  <input
                    type="text"
                    placeholder="Sentinel"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
                  />
                </div>
              </div>
              <div>
                <label className="block text-[11px] text-neutral-400 mb-1.5 font-medium">Teammate Icon</label>
                <div className="flex items-center gap-1.5 flex-wrap">
                  {AVATARS.map(({ key, label, Icon }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => setFormAvatar(key)}
                      title={label}
                      className={`p-2 rounded-lg border transition ${
                        formAvatar === key
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
                placeholder="Role (e.g. Code Review & Static Analysis Specialist)"
                value={formRole}
                onChange={(e) => setFormRole(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
              />
              <input
                type="text"
                placeholder="Short description (optional)"
                value={formDescription}
                onChange={(e) => setFormDescription(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500"
              />
              <textarea
                placeholder={"Instruction body (Markdown). The runtime composes the platform prompt with this profile — never from chat."}
                rows={4}
                value={formInstructions}
                onChange={(e) => setFormInstructions(e.target.value)}
                className="w-full px-3 py-2 bg-[#000000] border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-500 resize-none font-mono"
              />
              <button
                onClick={handleCreate}
                className="px-4 py-2 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-medium shadow-sm transition"
              >
                Create Teammate
              </button>
            </div>
          )}

          {/* Profiles List */}
          <div className="grid grid-cols-1 gap-3">
            {profiles.map((p) => {
              const liveStatus = statuses[p.id];
              const isBusy = liveStatus?.state === "busy" || liveStatus?.state === "delegating";
              const isActive = p.enabled && !p.validation_error;
              const origin = ORIGIN_LABELS[p.origin] ?? {
                label: typeof p.origin === "string" ? p.origin : "Custom",
                tone: "bg-neutral-800 text-neutral-300 border border-neutral-700",
              };
              const expanded = expandedId === p.id;
              const canDelete = p.origin === "custom" || p.origin === "override";

              return (
                <div
                  key={p.id}
                  className={`rounded-xl border transition ${
                    selectedBotId === p.id
                      ? "bg-neutral-800 text-neutral-100 border-neutral-700 shadow-sm"
                      : "bg-[#141414] border-neutral-800 hover:border-neutral-700"
                  }`}
                >
                  <div
                    onClick={() => isActive && onSelectBot(p)}
                    className={`p-4 flex flex-col gap-2 ${isActive ? "cursor-pointer" : "cursor-default"}`}
                  >
                    <div className="flex items-center justify-between gap-4">
                      <div className="flex items-center gap-3.5 min-w-0">
                        <AgentAvatar
                          avatar={p.avatar}
                          name={p.name}
                          className="w-10 h-10 rounded-xl bg-black/60 border border-neutral-800 flex items-center justify-center text-neutral-200 shadow-sm"
                          iconClassName="w-5 h-5"
                        />
                        <div className="min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-sm font-semibold text-neutral-100">@{p.name}</span>
                            <span
                              className={`text-[10px] uppercase font-medium px-1.5 py-0.5 rounded ${origin.tone}`}
                            >
                              {origin.label}
                            </span>
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
                          <div className="text-xs text-neutral-400 mt-0.5">{p.role}</div>
                          <div className="text-[10px] text-neutral-500 mt-0.5 flex items-center gap-2 flex-wrap">
                            <span className="font-mono">{p.id}</span>
                            {p.version_label && <span className="font-mono">{p.version_label}</span>}
                            {p.delegation_enabled && (
                              <span>delegation ×{p.delegation_max_children} depth {p.delegation_max_depth}</span>
                            )}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-2 shrink-0">
                        {selectedBotId === p.id && (
                          <span className="text-xs text-neutral-200 font-medium flex items-center gap-1">
                            <Check className="w-3.5 h-3.5" /> Selected
                          </span>
                        )}
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleToggleEnabled(p);
                          }}
                          title={p.enabled ? "Disable agent" : "Enable agent"}
                          className={`p-1.5 rounded-lg transition ${
                            p.enabled
                              ? "text-neutral-300 hover:text-neutral-100 hover:bg-neutral-800"
                              : "text-neutral-600 hover:text-neutral-400 hover:bg-neutral-800"
                          }`}
                        >
                          <Power className="w-4 h-4" />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setExpandedId(expanded ? null : p.id);
                          }}
                          title="Manage"
                          className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
                        >
                          <Pencil className="w-4 h-4" />
                        </button>
                      </div>
                    </div>

                    {p.validation_error ? (
                      <div className="mt-2 text-xs p-2 rounded-lg bg-red-950/40 border border-red-500/40 text-red-200/90 flex items-center gap-2">
                        <Ban className="w-3.5 h-3.5 text-red-400 shrink-0" />
                        <span>Invalid manifest: {p.validation_error}</span>
                      </div>
                    ) : !isActive ? (
                      <div className="mt-2 text-xs p-2 rounded-lg bg-black/40 border border-neutral-700 text-neutral-400 flex items-center gap-2">
                        <Ban className="w-3.5 h-3.5 shrink-0" />
                        <span>Disabled — this agent cannot be selected or receive delegations.</span>
                      </div>
                    ) : null}

                    {liveStatus?.active_tasks && liveStatus.active_tasks.length > 0 && (
                      <div className="mt-2 space-y-1">
                        {liveStatus.active_tasks.map((t) => (
                          <div
                            key={t.run_id}
                            className="text-xs p-2 rounded-lg bg-black/40 border border-amber-500/30 text-amber-200/90 flex items-center gap-2"
                          >
                            <Activity className="w-3.5 h-3.5 text-amber-400 animate-spin shrink-0" />
                            <span className="truncate">Task: {t.objective}</span>
                          </div>
                        ))}
                      </div>
                    )}

                    {expanded && (
                      <div
                        onClick={(e) => e.stopPropagation()}
                        className="mt-2 p-3 bg-black/40 border border-neutral-800 rounded-lg space-y-2"
                      >
                        <div className="text-[11px] text-neutral-400 grid grid-cols-2 gap-2 font-mono">
                          <span>model: {p.model_id ?? "inherit"}</span>
                          <span>tools: {p.tools_mode}</span>
                          <span>max steps: {p.max_steps}</span>
                          <span>schema: v{p.schema_version}</span>
                        </div>
                        <div className="flex items-center gap-2 flex-wrap">
                          <button
                            onClick={() => openEditor(p)}
                            className="px-2.5 py-1.5 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 text-[11px] font-medium flex items-center gap-1.5 transition"
                          >
                            <FileCode2 className="w-3 h-3" /> Edit manifest
                          </button>
                          {p.is_builtin && (
                            <button
                              onClick={() => handleReset(p)}
                              className="px-2.5 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-[11px] font-medium flex items-center gap-1.5 transition"
                            >
                              <RotateCcw className="w-3 h-3" /> Reset to default
                            </button>
                          )}
                          {canDelete && (
                            <button
                              onClick={() => setProfilePendingDeletion(p)}
                              className="px-2.5 py-1.5 rounded-lg bg-red-950/50 hover:bg-red-900/60 text-red-200 border border-red-800/60 text-[11px] font-medium flex items-center gap-1.5 transition"
                            >
                              <Trash2 className="w-3 h-3" /> {p.origin === "override" ? "Remove override" : "Delete profile"}
                            </button>
                          )}
                          {p.source_path && (
                            <span className="text-[10px] text-neutral-500 font-mono truncate" title={p.source_path}>
                              {p.source_path}
                            </span>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
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

      {/* Manifest editor */}
      {editorProfile && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
          <div className="w-full max-w-xl bg-[#111111] border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
            <div className="flex items-center justify-between px-5 py-4 border-b border-neutral-800 bg-[#141414]">
              <div>
                <h3 className="text-sm font-semibold text-neutral-100">Edit manifest — @{editorProfile.name}</h3>
                <p className="text-[10px] text-neutral-500 font-mono">
                  { editorProfile.source_path ?? "bundled default (save creates an override)" }
                </p>
              </div>
              <button
                onClick={closeEditor}
                className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5">
              <textarea
                value={editorDocument}
                onChange={(e) => setEditorDocument(e.target.value)}
                spellCheck={false}
                className="w-full h-[50vh] px-3 py-2.5 bg-[#000000] border border-neutral-800 rounded-lg text-[11px] text-neutral-200 focus:outline-none focus:border-neutral-500 resize-none font-mono leading-relaxed"
              />
              <p className="text-[10px] text-neutral-500 mt-2">
                Frontmatter (YAML) defines id, name, avatar, role, model, tool policy, delegation
                limits; the Markdown body is the profile instructions. Invalid saves are rejected
                and the previously active manifest stays live.
              </p>
            </div>
            <div className="flex items-center justify-between px-5 py-4 border-t border-neutral-800 bg-[#141414]">
              <span className="text-[10px] text-neutral-500 font-mono">
                id: {editorProfile.id}
              </span>
              <div className="flex items-center gap-2">
                <button
                  onClick={closeEditor}
                  className="px-3 py-2 rounded-lg bg-neutral-900 hover:bg-neutral-800 text-neutral-300 text-xs font-medium border border-neutral-700 transition"
                >
                  Discard
                </button>
                <button
                  onClick={handleSaveDocument}
                  disabled={isSaving}
                  className="px-4 py-2 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 text-xs font-medium shadow-sm transition disabled:opacity-50"
                >
                  {isSaving ? "Validating..." : "Save manifest"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog
        isOpen={profilePendingDeletion !== null}
        title={
          profilePendingDeletion?.origin === "override"
            ? "Remove this override?"
            : "Delete this teammate?"
        }
        message={
          profilePendingDeletion
            ? profilePendingDeletion.origin === "override"
              ? `Removing "${profilePendingDeletion.name}" restores the shipped default profile. Your bundled agent comes back unchanged.`
              : `"${profilePendingDeletion.name}" will be deleted from the agent registry and can no longer receive delegated tasks. This cannot be undone.`
            : ""
        }
        confirmLabel={profilePendingDeletion?.origin === "override" ? "Remove override" : "Delete profile"}
        destructive
        isBusy={isDeleting}
        onCancel={() => setProfilePendingDeletion(null)}
        onConfirm={() => {
          if (profilePendingDeletion) handleDelete(profilePendingDeletion.id);
        }}
      />
    </div>
  );
};

// Re-exported icon names used by callers mapping avatar keys.
export const AgentStatusIcons = AVATAR_ICONS as unknown as Record<string, unknown>;
