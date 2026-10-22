import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Clock,
  Play,
  Plus,
  Trash2,
  CheckCircle2,
  Sparkles,
  RefreshCw,
  Pause,
  History,
  Edit3,
  AlertTriangle,
} from "lucide-react";
import {
  RoutineRecord,
  RoutineRunRecord,
  NlRoutineSuggestion,
} from "../types/agent";

interface MyRoutinesViewProps {
  initialTab?: "routines" | "history";
  onAskPrompt?: (prompt: string) => void;
}

export const MyRoutinesView: React.FC<MyRoutinesViewProps> = ({
  initialTab = "routines",
  onAskPrompt,
}) => {
  const [activeTab, setActiveTab] = useState<"routines" | "history">(initialTab);
  const [routines, setRoutines] = useState<RoutineRecord[]>([]);
  const [historyRuns, setHistoryRuns] = useState<RoutineRunRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [runningRoutineId, setRunningRoutineId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // NL Creator State
  const [nlPrompt, setNlPrompt] = useState("");
  const [isSuggesting, setIsSuggesting] = useState(false);
  const [suggestion, setSuggestion] = useState<NlRoutineSuggestion | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    setLoading(true);
    try {
      const r = await invoke<RoutineRecord[]>("list_routines");
      setRoutines(r);
      const h = await invoke<RoutineRunRecord[]>("list_routine_runs", { limit: 30 });
      setHistoryRuns(h);
    } catch {
      // fallback
    } finally {
      setLoading(false);
    }
  };

  const handleSuggest = async () => {
    if (!nlPrompt.trim()) return;
    setIsSuggesting(true);
    setNotice(null);
    try {
      const res = await invoke<NlRoutineSuggestion>("natural_language_to_routine", {
        prompt: nlPrompt.trim(),
      });
      setSuggestion(res);
    } catch (e: any) {
      setNotice(`Suggestion error: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsSuggesting(false);
    }
  };

  const handleSaveRoutine = async () => {
    if (!suggestion) return;
    setIsSaving(true);
    setNotice(null);
    try {
      const newRoutine: RoutineRecord = {
        id: `routine_${Date.now()}_${Math.random().toString(36).substring(2, 6)}`,
        name: suggestion.name,
        cron_expression: suggestion.cron_expression,
        action_type: suggestion.action_type,
        target_id: suggestion.target_id,
        enabled: true,
        last_run: undefined,
        next_run: undefined,
      };

      await invoke("create_routine", { routine: newRoutine });
      setNotice(`Routine '${suggestion.name}' created and scheduled!`);
      setSuggestion(null);
      setNlPrompt("");
      await loadData();
    } catch (e: any) {
      setNotice(`Failed to save routine: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsSaving(false);
    }
  };

  // Routine Edit State
  const [editingRoutine, setEditingRoutine] = useState<RoutineRecord | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);

  const handleRunNow = async (r: RoutineRecord) => {
    setRunningRoutineId(r.id);
    setNotice(null);
    try {
      const res = await invoke<RoutineRunRecord>("execute_routine_now", { routineId: r.id });
      if (res.status === "succeeded") {
        setNotice(`Routine '${r.name}' executed successfully.`);
      } else {
        setNotice(`Routine '${r.name}' ended with status '${res.status}': ${res.error || res.summary || "No error details"}`);
      }
    } catch (e: any) {
      setNotice(`Execution error: ${e?.toString() || "Unknown error"}`);
    } finally {
      await loadData();
      setRunningRoutineId(null);
    }
  };

  const handleToggleEnabled = async (r: RoutineRecord) => {
    try {
      const updated = { ...r, enabled: !r.enabled };
      await invoke("update_routine", { routine: updated });
      await loadData();
    } catch (e: any) {
      setNotice(`Update error: ${e?.toString() || "Unknown error"}`);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingRoutine) return;
    setIsUpdating(true);
    setNotice(null);
    try {
      await invoke("update_routine", { routine: editingRoutine });
      setNotice(`Routine '${editingRoutine.name}' updated successfully.`);
      setEditingRoutine(null);
      await loadData();
    } catch (e: any) {
      setNotice(`Failed to update routine: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsUpdating(false);
    }
  };

  const handleDeleteRoutine = async (id: string, name: string) => {
    if (!window.confirm(`Are you sure you want to delete routine '${name}'?`)) return;
    try {
      await invoke("delete_routine", { id });
      setNotice(`Deleted routine '${name}'.`);
      await loadData();
    } catch (e: any) {
      setNotice(`Delete error: ${e?.toString() || "Unknown error"}`);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto p-6 max-w-6xl mx-auto w-full space-y-6">
      {/* Header */}
      <div className="pb-4 border-b border-neutral-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold text-neutral-100">My Routines</h1>
          <p className="text-xs text-neutral-400 mt-1">
            Automate recurring computer tasks without coding. Describe what you need in plain English.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setActiveTab("routines")}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition ${
              activeTab === "routines"
                ? "bg-purple-950/60 text-purple-200 border border-purple-800/60"
                : "bg-neutral-900 text-neutral-400 hover:text-neutral-200 border border-neutral-800"
            }`}
          >
            Saved Routines ({routines.length})
          </button>
          <button
            onClick={() => setActiveTab("history")}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition ${
              activeTab === "history"
                ? "bg-purple-950/60 text-purple-200 border border-purple-800/60"
                : "bg-neutral-900 text-neutral-400 hover:text-neutral-200 border border-neutral-800"
            }`}
          >
            Run History ({historyRuns.length})
          </button>
        </div>
      </div>

      {notice && (
        <div className="p-3 rounded-lg bg-neutral-900 border border-neutral-800 text-neutral-200 text-xs flex items-center justify-between">
          <span>{notice}</span>
          <button onClick={() => setNotice(null)} className="text-neutral-400 hover:text-neutral-200 text-xs">
            Dismiss
          </button>
        </div>
      )}

      {/* Natural Language Creator Box */}
      <div className="p-4 rounded-xl bg-[#131313] border border-purple-900/30 space-y-3 shadow-sm">
        <div className="flex items-center gap-2 text-xs font-semibold text-purple-300">
          <Sparkles className="w-4 h-4 text-purple-400" />
          <span>Create an Automated Routine in Everyday Language</span>
        </div>

        <div className="flex flex-col sm:flex-row items-center gap-2">
          <input
            type="text"
            value={nlPrompt}
            onChange={(e) => setNlPrompt(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSuggest()}
            placeholder="e.g. 'Every Friday at 9am, find new receipts in Downloads and show me a summary'..."
            className="flex-1 px-3.5 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none focus:border-purple-600"
          />
          <button
            onClick={handleSuggest}
            disabled={isSuggesting || !nlPrompt.trim()}
            className="w-full sm:w-auto px-4 py-2 bg-purple-950/60 hover:bg-purple-900/60 border border-purple-700/60 rounded-lg text-xs font-semibold text-purple-200 flex items-center justify-center gap-1.5 transition"
          >
            {isSuggesting ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Plus className="w-3.5 h-3.5" />}
            Build Routine
          </button>
        </div>

        {/* Suggestion Preview & Customization Card */}
        {suggestion && (
          <div className="p-4 rounded-xl bg-[#181818] border border-purple-800/60 space-y-3 mt-3 animate-in fade-in">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-neutral-100 uppercase tracking-wider">
                Proposed Routine Review
              </span>
              <span className="text-[10px] bg-purple-900/50 text-purple-300 border border-purple-800 px-2 py-0.5 rounded font-mono">
                {suggestion.cron_expression}
              </span>
            </div>

            {suggestion.needs_clarification && (
              <div className="p-2.5 rounded-lg bg-amber-950/40 border border-amber-800/60 text-amber-200 text-xs flex items-start gap-2">
                <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
                <div>
                  <span className="font-semibold block">Schedule Clarification:</span>
                  <p className="text-[11px] text-amber-300 mt-0.5">
                    {suggestion.clarification_prompt || "No recurring schedule was specified. You can customize the cron schedule below before saving."}
                  </p>
                </div>
              </div>
            )}

            <div className="space-y-2">
              <div>
                <label className="text-[11px] text-neutral-400 block mb-0.5">Routine Name:</label>
                <input
                  type="text"
                  value={suggestion.name}
                  onChange={(e) => setSuggestion({ ...suggestion, name: e.target.value })}
                  className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-xs text-neutral-100 focus:outline-none"
                />
              </div>

              <div>
                <label className="text-[11px] text-neutral-400 block mb-0.5">Recurring Schedule (Cron Expression):</label>
                <input
                  type="text"
                  value={suggestion.cron_expression}
                  onChange={(e) => setSuggestion({ ...suggestion, cron_expression: e.target.value })}
                  placeholder="e.g. 0 9 * * 1-5"
                  className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-xs text-neutral-100 font-mono focus:outline-none"
                />
              </div>

              <div>
                <label className="text-[11px] text-neutral-400 block mb-0.5">What Kyro will execute:</label>
                <p className="text-xs text-neutral-200 p-2 rounded bg-neutral-900 border border-neutral-800/80">
                  {suggestion.target_id}
                </p>
              </div>

              <div className="text-[11px] text-neutral-300">
                <span className="text-neutral-400">Explanation: </span>
                {suggestion.explanation}
              </div>

              {suggestion.required_resources.length > 0 && (
                <div className="flex items-center gap-2 pt-1 text-[11px] text-neutral-400">
                  <span>Required Access:</span>
                  <div className="flex flex-wrap gap-1">
                    {suggestion.required_resources.map((res, idx) => (
                      <span key={idx} className="px-2 py-0.5 rounded bg-neutral-900 text-neutral-300 border border-neutral-800">
                        {res}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>

            <div className="flex items-center justify-end gap-2 pt-2 border-t border-neutral-800">
              <button
                onClick={() => setSuggestion(null)}
                className="px-3 py-1.5 rounded-lg text-xs text-neutral-400 hover:text-neutral-200"
              >
                Cancel
              </button>
              <button
                onClick={handleSaveRoutine}
                disabled={isSaving}
                className="px-4 py-1.5 bg-neutral-100 hover:bg-white text-neutral-950 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition shadow-sm"
              >
                {isSaving ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
                Confirm & Schedule Routine
              </button>
            </div>
          </div>
        )}
      </div>

      {/* TAB 1: SAVED ROUTINES */}
      {activeTab === "routines" && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-xs text-neutral-400">
            <span>{routines.length} saved automations</span>
            <span className="italic">Execution runs safely within authorized permissions</span>
          </div>

          {loading ? (
            <div className="p-8 text-center rounded-xl bg-[#121212] border border-neutral-800 text-neutral-400">
              <RefreshCw className="w-5 h-5 animate-spin mx-auto mb-2 text-neutral-400" />
              <p className="text-xs">Loading routines...</p>
            </div>
          ) : routines.length > 0 ? (
            <div className="space-y-2.5">
              {routines.map((r) => (
                <div
                  key={r.id}
                  className="p-4 rounded-xl bg-[#141414] border border-neutral-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:border-neutral-700 transition"
                >
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-neutral-100">{r.name}</span>
                      <span
                        className={`text-[10px] px-2 py-0.5 rounded-full font-medium ${
                          r.enabled
                            ? "bg-purple-950/60 text-purple-300 border border-purple-800/60"
                            : "bg-neutral-800 text-neutral-400 border border-neutral-700"
                        }`}
                      >
                        {r.enabled ? "Active" : "Paused"}
                      </span>
                    </div>
                    <p className="text-xs text-neutral-400 line-clamp-1">{r.target_id}</p>
                    <div className="flex items-center gap-3 text-[11px] text-neutral-400 pt-0.5">
                      <span className="flex items-center gap-1">
                        <Clock className="w-3 h-3 text-neutral-400" />
                        Schedule: <code className="text-neutral-300 font-mono">{r.cron_expression}</code>
                      </span>
                      {r.last_run && (
                        <span>Last run: {new Date(r.last_run).toLocaleDateString()}</span>
                      )}
                    </div>
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      onClick={() => handleRunNow(r)}
                      disabled={runningRoutineId === r.id}
                      className="px-3 py-1.5 bg-purple-950/50 hover:bg-purple-900/50 border border-purple-800/60 rounded-lg text-xs font-semibold text-purple-200 flex items-center gap-1.5 transition"
                    >
                      <Play className="w-3.5 h-3.5 text-purple-300" />
                      {runningRoutineId === r.id ? "Running..." : "Run Now"}
                    </button>
                    <button
                      onClick={() => setEditingRoutine({ ...r })}
                      className="p-1.5 rounded-lg bg-neutral-900 hover:bg-neutral-800 text-neutral-300 border border-neutral-800 transition"
                      title="Edit routine"
                      aria-label="Edit routine"
                    >
                      <Edit3 className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleToggleEnabled(r)}
                      className="p-1.5 rounded-lg bg-neutral-900 hover:bg-neutral-800 text-neutral-300 border border-neutral-800 transition"
                      title={r.enabled ? "Pause routine" : "Resume routine"}
                    >
                      <Pause className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleDeleteRoutine(r.id, r.name)}
                      className="p-1.5 rounded-lg bg-neutral-900 hover:bg-neutral-800 text-neutral-400 hover:text-rose-400 border border-neutral-800 transition"
                      title="Delete routine"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="p-8 text-center rounded-xl bg-[#121212] border border-neutral-800 text-neutral-400 space-y-2">
              <Clock className="w-8 h-8 text-neutral-400 mx-auto" />
              <p className="text-sm font-medium text-neutral-300">No saved routines yet</p>
              <p className="text-xs text-neutral-400 max-w-sm mx-auto">
                Use the box above to type what you want to automate. For example: &ldquo;Every Friday, organize my downloads&rdquo;.
              </p>
              {onAskPrompt && (
                <button
                  onClick={() => onAskPrompt("Every Friday, organize my new documents and show me what changed")}
                  className="text-xs text-purple-400 hover:text-purple-300 underline pt-1 block mx-auto"
                >
                  Ask Kyro: &ldquo;Every Friday, organize my new documents&rdquo;
                </button>
              )}
            </div>
          )}

          {/* Edit Routine Modal */}
          {editingRoutine && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
              <div className="bg-[#141414] border border-neutral-800 rounded-2xl p-6 max-w-lg w-full space-y-4 shadow-2xl">
                <div className="flex items-center justify-between border-b border-neutral-800 pb-3">
                  <h3 className="text-sm font-bold text-neutral-100 flex items-center gap-2">
                    <Edit3 className="w-4 h-4 text-purple-400" />
                    Edit Routine
                  </h3>
                  <button
                    onClick={() => setEditingRoutine(null)}
                    className="text-neutral-400 hover:text-neutral-200 text-xs"
                  >
                    Cancel
                  </button>
                </div>

                <div className="space-y-3">
                  <div>
                    <label className="text-xs font-semibold text-neutral-300 block mb-1">Name:</label>
                    <input
                      type="text"
                      value={editingRoutine.name}
                      onChange={(e) => setEditingRoutine({ ...editingRoutine, name: e.target.value })}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none"
                    />
                  </div>

                  <div>
                    <label className="text-xs font-semibold text-neutral-300 block mb-1">
                      Cron Schedule Expression:
                    </label>
                    <input
                      type="text"
                      value={editingRoutine.cron_expression}
                      onChange={(e) =>
                        setEditingRoutine({ ...editingRoutine, cron_expression: e.target.value })
                      }
                      placeholder="e.g. 0 9 * * 1-5"
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 font-mono focus:outline-none"
                    />
                  </div>

                  <div>
                    <label className="text-xs font-semibold text-neutral-300 block mb-1">
                      Instruction / Target:
                    </label>
                    <textarea
                      value={editingRoutine.target_id}
                      onChange={(e) =>
                        setEditingRoutine({ ...editingRoutine, target_id: e.target.value })
                      }
                      rows={3}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-100 focus:outline-none"
                    />
                  </div>
                </div>

                <div className="flex items-center justify-end gap-2 pt-2 border-t border-neutral-800">
                  <button
                    onClick={() => setEditingRoutine(null)}
                    className="px-3 py-1.5 rounded-lg text-xs text-neutral-400 hover:text-neutral-200"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleSaveEdit}
                    disabled={isUpdating}
                    className="px-4 py-1.5 bg-neutral-100 hover:bg-white text-neutral-950 font-semibold rounded-lg text-xs transition"
                  >
                    {isUpdating ? "Saving..." : "Save Changes"}
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* TAB 2: EXECUTION HISTORY */}
      {activeTab === "history" && (
        <div className="space-y-3">
          <div className="flex items-center justify-between text-xs text-neutral-400">
            <span>Durable execution history</span>
            <button onClick={loadData} className="text-purple-400 hover:text-purple-300 flex items-center gap-1">
              <RefreshCw className="w-3 h-3" /> Refresh
            </button>
          </div>

          {historyRuns.length > 0 ? (
            <div className="space-y-2">
              {historyRuns.map((run) => (
                <div
                  key={run.id}
                  className="p-3 rounded-lg bg-[#141414] border border-neutral-800 flex items-start justify-between text-xs"
                >
                  <div className="space-y-1 pr-3">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-neutral-200">{run.routine_name}</span>
                      <span
                        className={`text-[10px] px-2 py-0.2 rounded font-medium uppercase font-mono ${
                          run.status === "succeeded"
                            ? "bg-neutral-800 text-neutral-200 border border-neutral-700"
                            : run.status === "failed"
                            ? "bg-rose-950/60 text-rose-300 border border-rose-800/60"
                            : "bg-neutral-800 text-neutral-300"
                        }`}
                      >
                        {run.status}
                      </span>
                    </div>
                    {run.summary && <p className="text-neutral-300 leading-snug">{run.summary}</p>}
                    {run.error && <p className="text-rose-400 text-[11px]">{run.error}</p>}
                    <span className="text-[10px] text-neutral-400">
                      Executed: {new Date(run.started_at).toLocaleString()}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="p-8 text-center rounded-xl bg-[#121212] border border-neutral-800 text-neutral-400">
              <History className="w-8 h-8 text-neutral-400 mx-auto mb-2" />
              <p className="text-xs font-medium text-neutral-300">No routine runs recorded yet</p>
              <p className="text-[11px] text-neutral-400 mt-1">
                When routines execute automatically or manually, their verifiable outcomes appear here.
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
