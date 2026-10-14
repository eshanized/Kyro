import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RoutineRecord, SkillRecord, SkillStep } from "../types/agent";
import {
  Play,
  Trash2,
  Clock,
  Circle,
  X,
  Plus,
  Calendar,
  Workflow,
  Search,
  AlertTriangle,
  Info,
} from "lucide-react";
import { InlineNotice } from "./InlineNotice";

interface TeachTaskModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialTab?: "skills" | "record" | "routines";
  highlightTaskId?: string;
}

interface DeleteTarget {
  id: string;
  name: string;
  type: "skill" | "routine";
  linkedRoutinesCount: number;
}

export const TeachTaskModal: React.FC<TeachTaskModalProps> = ({
  isOpen,
  onClose,
  initialTab = "skills",
  highlightTaskId,
}) => {
  const [activeTab, setActiveTab] = useState<"skills" | "record" | "routines">(initialTab);
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [routines, setRoutines] = useState<RoutineRecord[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [recordName, setRecordName] = useState("");
  const [recordDesc, setRecordDesc] = useState("");
  const [replayingId, setReplayingId] = useState<string | null>(null);

  // Search & Filter state
  const [skillSearchQuery, setSkillSearchQuery] = useState("");
  const [routineSearchQuery, setRoutineSearchQuery] = useState("");

  // Details view state
  const [selectedSkill, setSelectedSkill] = useState<SkillRecord | null>(null);
  const [selectedRoutine, setSelectedRoutine] = useState<RoutineRecord | null>(null);

  // Deletion Confirmation Dialog state
  const [taskToDelete, setTaskToDelete] = useState<DeleteTarget | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  // Inline notice for operation failures (never a blocking browser dialog).
  const [notice, setNotice] = useState<{ tone: "error" | "success" | "info"; message: string } | null>(
    null
  );

  // New routine fields
  const [routineName, setRoutineName] = useState("");
  const [routineCron, setRoutineCron] = useState("*/15 * * * *");
  const [routineTarget, setRoutineTarget] = useState("");

  useEffect(() => {
    if (isOpen) {
      if (initialTab) {
        setActiveTab(initialTab);
      }
      loadData();
    }
  }, [isOpen, initialTab]);

  useEffect(() => {
    if (highlightTaskId && isOpen) {
      const matchedSkill = skills.find((s) => s.id === highlightTaskId);
      if (matchedSkill) {
        setSelectedSkill(matchedSkill);
        setActiveTab("skills");
        return;
      }
      const matchedRoutine = routines.find((r) => r.id === highlightTaskId);
      if (matchedRoutine) {
        setSelectedRoutine(matchedRoutine);
        setActiveTab("routines");
      }
    }
  }, [highlightTaskId, skills, routines, isOpen]);

  const loadData = async () => {
    try {
      const s = await invoke<SkillRecord[]>("list_skills");
      setSkills(s);
      const r = await invoke<RoutineRecord[]>("list_routines");
      setRoutines(r);
      if (s.length > 0 && !routineTarget) {
        setRoutineTarget(s[0].id);
      }
    } catch (e) {
      console.error("Failed to load task and routine data:", e);
    }
  };

  const handleStartRecord = async () => {
    if (!recordName.trim()) {
      setNotice({ tone: "error", message: "Please provide a name for this workflow." });
      return;
    }
    try {
      await invoke("start_teach_task", { name: recordName, description: recordDesc });
      setIsRecording(true);
      setNotice(null);
    } catch (err: any) {
      setNotice({ tone: "error", message: "Error starting recording: " + err });
    }
  };

  const handleStopRecord = async () => {
    try {
      await invoke("stop_teach_task");
      setIsRecording(false);
      setRecordName("");
      setRecordDesc("");
      setActiveTab("skills");
      setNotice(null);
      await loadData();
    } catch (err: any) {
      setNotice({ tone: "error", message: "Error stopping recording: " + err });
    }
  };

  const handleReplaySkill = async (id: string) => {
    setReplayingId(id);
    try {
      await invoke("replay_skill", { skillId: id });
      setNotice(null);
    } catch (err: any) {
      setNotice({ tone: "error", message: "Error replaying skill: " + err });
    } finally {
      setReplayingId(null);
    }
  };

  const handleCreateRoutine = async () => {
    if (!routineName.trim() || !routineTarget) return;
    try {
      await invoke("create_routine", {
        routine: {
          id: `routine-${Date.now()}`,
          name: routineName,
          cron_expression: routineCron,
          action_type: "skill",
          target_id: routineTarget,
          enabled: true,
          last_run: null,
        },
      });
      setRoutineName("");
      setNotice(null);
      await loadData();
    } catch (err: any) {
      setNotice({ tone: "error", message: "Error creating routine: " + err });
    }
  };

  // Trigger Confirmation Dialog for a Skill
  const promptDeleteSkill = (skill: SkillRecord) => {
    const linkedRoutines = routines.filter(
      (r) => r.action_type === "skill" && r.target_id === skill.id
    );
    setDeleteError(null);
    setTaskToDelete({
      id: skill.id,
      name: skill.name,
      type: "skill",
      linkedRoutinesCount: linkedRoutines.length,
    });
  };

  // Trigger Confirmation Dialog for a Routine
  const promptDeleteRoutine = (routine: RoutineRecord) => {
    setDeleteError(null);
    setTaskToDelete({
      id: routine.id,
      name: routine.name,
      type: "routine",
      linkedRoutinesCount: 0,
    });
  };

  const handleCancelDelete = () => {
    if (isDeleting) return;
    setTaskToDelete(null);
    setDeleteError(null);
  };

  const handleConfirmDelete = async () => {
    if (!taskToDelete || isDeleting) return;

    setIsDeleting(true);
    setDeleteError(null);

    try {
      if (taskToDelete.type === "skill") {
        await invoke("delete_skill", { id: taskToDelete.id });
      } else {
        await invoke("delete_routine", { id: taskToDelete.id });
      }

      // Close details view if it matches the deleted task
      if (selectedSkill?.id === taskToDelete.id) {
        setSelectedSkill(null);
      }
      if (selectedRoutine?.id === taskToDelete.id) {
        setSelectedRoutine(null);
      }

      // Close dialog only on confirmed backend success
      setTaskToDelete(null);
      setIsDeleting(false);

      // Refresh all lists consistently from SQLite
      await loadData();
    } catch (err: any) {
      setIsDeleting(false);
      setDeleteError(typeof err === "string" ? err : err?.message || String(err));
      // Do not remove task optimistically on failure
    }
  };

  // Filter skills and routines by active search query
  const filteredSkills = skills.filter((s) => {
    if (!skillSearchQuery.trim()) return true;
    const q = skillSearchQuery.toLowerCase();
    return (
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      s.id.toLowerCase().includes(q)
    );
  });

  const filteredRoutines = routines.filter((r) => {
    if (!routineSearchQuery.trim()) return true;
    const q = routineSearchQuery.toLowerCase();
    return (
      r.name.toLowerCase().includes(q) ||
      r.cron_expression.toLowerCase().includes(q) ||
      r.target_id.toLowerCase().includes(q) ||
      r.id.toLowerCase().includes(q)
    );
  });

  const parseSteps = (stepsJson: string): SkillStep[] => {
    try {
      return JSON.parse(stepsJson);
    } catch {
      return [];
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-md p-4">
      <div className="w-full max-w-3xl bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#262626] bg-[#141414]">
          <div className="flex items-center gap-2.5">
            <Workflow className="w-5 h-5 text-neutral-200" />
            <h2 className="text-base font-semibold text-neutral-100">
              Teach-a-Task & Routine Scheduler
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
            aria-label="Close modal"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex items-center px-6 border-b border-[#262626] bg-[#111111] gap-2 text-xs font-medium">
          <button
            onClick={() => {
              setActiveTab("skills");
              setSelectedSkill(null);
            }}
            className={`py-3 px-3.5 border-b-2 flex items-center gap-1.5 transition ${
              activeTab === "skills"
                ? "border-neutral-100 text-neutral-100 font-semibold"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Play className="w-3.5 h-3.5" /> Saved Skills ({skills.length})
          </button>
          <button
            onClick={() => setActiveTab("record")}
            className={`py-3 px-3.5 border-b-2 flex items-center gap-1.5 transition ${
              activeTab === "record"
                ? "border-neutral-100 text-neutral-100 font-semibold"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Circle
              className={`w-3.5 h-3.5 ${
                isRecording ? "text-red-500 fill-current animate-pulse" : ""
              }`}
            />{" "}
            Record Workflow
          </button>
          <button
            onClick={() => {
              setActiveTab("routines");
              setSelectedRoutine(null);
            }}
            className={`py-3 px-3.5 border-b-2 flex items-center gap-1.5 transition ${
              activeTab === "routines"
                ? "border-neutral-100 text-neutral-100 font-semibold"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Calendar className="w-3.5 h-3.5" /> Routine Schedules ({routines.length})
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {notice && (
            <InlineNotice
              tone={notice.tone}
              message={notice.message}
              onDismiss={() => setNotice(null)}
            />
          )}
          {/* TAB 1: SAVED SKILLS */}
          {activeTab === "skills" && (
            <div className="space-y-4">
              <div className="flex items-center justify-between gap-4">
                <span className="text-xs text-neutral-400">
                  Pre-recorded workflows synthesized into reusable macro skills.
                </span>
                <button
                  onClick={() => setActiveTab("record")}
                  className="px-3 py-1.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-medium flex items-center gap-1.5 shadow transition shrink-0"
                >
                  <Plus className="w-3.5 h-3.5" /> Teach New Skill
                </button>
              </div>

              {/* Search Bar */}
              {skills.length > 0 && (
                <div className="relative">
                  <Search className="w-4 h-4 text-neutral-500 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
                  <input
                    type="text"
                    placeholder="Search saved skills by name, description, or ID..."
                    value={skillSearchQuery}
                    onChange={(e) => setSkillSearchQuery(e.target.value)}
                    className="w-full pl-9 pr-4 py-2 bg-[#0a0a0a] border border-[#262626] rounded-xl text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 transition"
                  />
                  {skillSearchQuery && (
                    <button
                      onClick={() => setSkillSearchQuery("")}
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-neutral-500 hover:text-neutral-300 text-xs"
                      aria-label="Clear skill search"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              )}

              {/* Skill Details Drawer/Panel if a skill is selected */}
              {selectedSkill && (
                <div className="p-4 bg-[#141414] border border-neutral-700 rounded-xl space-y-4">
                  <div className="flex items-start justify-between gap-4 border-b border-[#262626] pb-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-sm font-bold text-neutral-100">{selectedSkill.name}</h3>
                        <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-black/60 text-neutral-400 border border-neutral-800">
                          {selectedSkill.id}
                        </span>
                      </div>
                      {selectedSkill.description && (
                        <p className="text-xs text-neutral-400 mt-1">{selectedSkill.description}</p>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleReplaySkill(selectedSkill.id)}
                        disabled={replayingId === selectedSkill.id}
                        className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 text-xs font-medium flex items-center gap-1.5 transition"
                      >
                        <Play className="w-3 h-3 fill-current" />
                        {replayingId === selectedSkill.id ? "Replaying..." : "Replay"}
                      </button>
                      <button
                        onClick={() => promptDeleteSkill(selectedSkill)}
                        className="px-3 py-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 border border-transparent hover:border-red-500/30 text-xs font-medium flex items-center gap-1.5 transition"
                        title="Delete task"
                        aria-label="Delete task"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                        <span>Delete task</span>
                      </button>
                      <button
                        onClick={() => setSelectedSkill(null)}
                        className="p-1.5 text-neutral-400 hover:text-neutral-200 rounded-lg hover:bg-neutral-800"
                        aria-label="Close details view"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                  </div>

                  <div>
                    <div className="text-[11px] font-semibold uppercase tracking-wider text-neutral-400 mb-2">
                      Recorded Steps ({parseSteps(selectedSkill.steps_json).length})
                    </div>
                    <div className="space-y-1.5 max-h-48 overflow-y-auto pr-1">
                      {parseSteps(selectedSkill.steps_json).map((step, idx) => (
                        <div
                          key={idx}
                          className="px-3 py-2 rounded-lg bg-[#0a0a0a] border border-[#262626] text-xs font-mono text-neutral-300 flex items-center justify-between"
                        >
                          <span className="flex items-center gap-2">
                            <span className="text-neutral-500">{idx + 1}.</span>
                            <span className="text-neutral-200 font-semibold">{step.action_type}</span>
                            {step.x !== undefined && step.x !== null && (
                              <span className="text-neutral-400">
                                ({step.x}, {step.y})
                              </span>
                            )}
                            {step.text && <span className="text-amber-300">"{step.text}"</span>}
                            {step.key && <span className="text-blue-300">[{step.key}]</span>}
                          </span>
                          <span className="text-[10px] text-neutral-500">{step.delay_ms}ms delay</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* Skills List */}
              {skills.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-[#262626] rounded-xl text-neutral-500 text-xs">
                  No skills recorded yet. Click "Teach New Skill" to demonstrate a desktop action sequence.
                </div>
              ) : filteredSkills.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-[#262626] rounded-xl text-neutral-500 text-xs">
                  No saved skills match &ldquo;{skillSearchQuery}&rdquo;
                </div>
              ) : (
                <div className="space-y-2.5">
                  {filteredSkills.map((s) => (
                    <div
                      key={s.id}
                      onClick={() => setSelectedSkill(s)}
                      className={`p-4 bg-[#171717] border rounded-xl flex items-center justify-between gap-4 transition cursor-pointer ${
                        selectedSkill?.id === s.id
                          ? "border-neutral-500 bg-[#1a1a1a]"
                          : "border-[#262626] hover:border-neutral-700"
                      }`}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="text-sm font-semibold text-neutral-100 flex items-center gap-2">
                          <span>{s.name}</span>
                          <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-black/60 text-neutral-400 border border-neutral-800">
                            {s.id}
                          </span>
                        </div>
                        {s.description && (
                          <div className="text-xs text-neutral-400 truncate mt-0.5">{s.description}</div>
                        )}
                      </div>
                      <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
                        <button
                          onClick={() => handleReplaySkill(s.id)}
                          disabled={replayingId === s.id}
                          className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 text-xs font-medium flex items-center gap-1.5 transition"
                        >
                          <Play className="w-3 h-3 fill-current" />
                          {replayingId === s.id ? "Replaying..." : "Replay"}
                        </button>
                        <button
                          onClick={() => promptDeleteSkill(s)}
                          className="p-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                          title="Delete task"
                          aria-label="Delete task"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 2: RECORD NEW WORKFLOW */}
          {activeTab === "record" && (
            <div className="space-y-5">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1">Demonstration-Based Learning</div>
                Click <strong>"Start Recording"</strong> and perform your workflow on the computer (click buttons, navigate pages, enter text). Kyro records every action, normalizes screen coordinates, and packages it into a repeatable skill.
              </div>

              {!isRecording ? (
                <div className="space-y-4">
                  <div>
                    <label className="block text-xs font-semibold text-neutral-300 mb-1.5">Skill Name</label>
                    <input
                      type="text"
                      placeholder="e.g. Export Daily Analytics Spreadsheet"
                      value={recordName}
                      onChange={(e) => setRecordName(e.target.value)}
                      className="w-full px-3.5 py-2.5 bg-[#0a0a0a] border border-[#262626] rounded-xl text-sm text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 transition"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-neutral-300 mb-1.5">Description (Optional)</label>
                    <input
                      type="text"
                      placeholder="Opens browser, navigates to dashboard, clicks Export CSV"
                      value={recordDesc}
                      onChange={(e) => setRecordDesc(e.target.value)}
                      className="w-full px-3.5 py-2.5 bg-[#0a0a0a] border border-[#262626] rounded-xl text-sm text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 transition"
                    />
                  </div>
                  <button
                    onClick={handleStartRecord}
                    className="w-full py-3 bg-red-600 hover:bg-red-500 text-white rounded-xl text-xs font-bold uppercase tracking-wider flex items-center justify-center gap-2 shadow transition"
                  >
                    <Circle className="w-4 h-4 fill-current animate-pulse" /> Start Recording Workflow
                  </button>
                </div>
              ) : (
                <div className="p-8 border border-red-500/50 bg-red-950/10 rounded-2xl text-center space-y-4">
                  <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-red-500/20 border border-red-500/40 text-red-400 text-xs font-semibold animate-pulse">
                    <Circle className="w-3 h-3 fill-current" /> RECORDING ACTIVE
                  </div>
                  <h3 className="text-base font-semibold text-neutral-100">{recordName}</h3>
                  <p className="text-xs text-neutral-400 max-w-sm mx-auto">
                    Perform the actions on your desktop. When finished, return here and click Stop.
                  </p>
                  <button
                    onClick={handleStopRecord}
                    className="px-6 py-2.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-xl text-xs font-bold uppercase tracking-wider transition shadow"
                  >
                    Save & Finish Skill
                  </button>
                </div>
              )}
            </div>
          )}

          {/* TAB 3: ROUTINES (CRON) */}
          {activeTab === "routines" && (
            <div className="space-y-5">
              <div className="p-4 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300 leading-relaxed">
                <div className="font-semibold text-neutral-100 mb-1 flex items-center gap-1.5">
                  <Clock className="w-4 h-4 text-neutral-300" /> Always-On Background Scheduler
                </div>
                Routines run in the background on regular schedules even when minimized, triggering skills or prompt checks automatically.
              </div>

              {/* Create Routine */}
              <div className="p-4 bg-[#171717] border border-[#262626] rounded-xl space-y-3">
                <div className="text-xs font-semibold text-neutral-200">Schedule New Routine</div>
                <div className="grid grid-cols-3 gap-3">
                  <input
                    type="text"
                    placeholder="Routine Name (e.g. Morning Scrape)"
                    value={routineName}
                    onChange={(e) => setRoutineName(e.target.value)}
                    className="px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400"
                  />
                  <select
                    value={routineCron}
                    onChange={(e) => setRoutineCron(e.target.value)}
                    className="px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400"
                  >
                    <option value="*/5 * * * *">Every 5 minutes</option>
                    <option value="*/15 * * * *">Every 15 minutes</option>
                    <option value="*/30 * * * *">Every 30 minutes</option>
                    <option value="0 * * * *">Hourly</option>
                    <option value="daily">Daily</option>
                  </select>
                  <select
                    value={routineTarget}
                    onChange={(e) => setRoutineTarget(e.target.value)}
                    className="px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 focus:outline-none focus:border-neutral-400"
                  >
                    {skills.map((s) => (
                      <option key={s.id} value={s.id}>
                        Skill: {s.name}
                      </option>
                    ))}
                  </select>
                </div>
                <button
                  onClick={handleCreateRoutine}
                  className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold flex items-center gap-1.5 transition shadow"
                >
                  <Plus className="w-3.5 h-3.5" /> Schedule Routine
                </button>
              </div>

              {/* Routine Search */}
              {routines.length > 0 && (
                <div className="relative">
                  <Search className="w-4 h-4 text-neutral-500 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
                  <input
                    type="text"
                    placeholder="Search routines by name, cron, target, or ID..."
                    value={routineSearchQuery}
                    onChange={(e) => setRoutineSearchQuery(e.target.value)}
                    className="w-full pl-9 pr-4 py-2 bg-[#0a0a0a] border border-[#262626] rounded-xl text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 transition"
                  />
                  {routineSearchQuery && (
                    <button
                      onClick={() => setRoutineSearchQuery("")}
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-neutral-500 hover:text-neutral-300 text-xs"
                      aria-label="Clear routine search"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              )}

              {/* Routine Details Drawer if selected */}
              {selectedRoutine && (
                <div className="p-4 bg-[#141414] border border-neutral-700 rounded-xl space-y-3">
                  <div className="flex items-start justify-between gap-4 border-b border-[#262626] pb-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-sm font-bold text-neutral-100">{selectedRoutine.name}</h3>
                        <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-black/60 text-neutral-400 border border-neutral-800">
                          {selectedRoutine.id}
                        </span>
                      </div>
                      <div className="text-xs text-neutral-400 font-mono mt-1">
                        Schedule: {selectedRoutine.cron_expression} • Action: {selectedRoutine.action_type} • Target:{" "}
                        {selectedRoutine.target_id}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => promptDeleteRoutine(selectedRoutine)}
                        className="px-3 py-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 border border-transparent hover:border-red-500/30 text-xs font-medium flex items-center gap-1.5 transition"
                        title="Delete task"
                        aria-label="Delete task"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                        <span>Delete task</span>
                      </button>
                      <button
                        onClick={() => setSelectedRoutine(null)}
                        className="p-1.5 text-neutral-400 hover:text-neutral-200 rounded-lg hover:bg-neutral-800"
                        aria-label="Close details view"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs text-neutral-400 font-mono">
                    <div className="p-2.5 rounded-lg bg-[#0a0a0a] border border-[#262626]">
                      <span className="text-neutral-500 block text-[10px] uppercase">Last Executed</span>
                      {selectedRoutine.last_run
                        ? new Date(selectedRoutine.last_run).toLocaleString()
                        : "Never"}
                    </div>
                    <div className="p-2.5 rounded-lg bg-[#0a0a0a] border border-[#262626]">
                      <span className="text-neutral-500 block text-[10px] uppercase">Next Scheduled</span>
                      {selectedRoutine.next_run
                        ? new Date(selectedRoutine.next_run).toLocaleString()
                        : "Computed on trigger"}
                    </div>
                  </div>
                </div>
              )}

              {/* Routine list */}
              {routines.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-[#262626] rounded-xl text-neutral-500 text-xs">
                  No routines scheduled yet. Use the form above to configure recurring executions.
                </div>
              ) : filteredRoutines.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-[#262626] rounded-xl text-neutral-500 text-xs">
                  No routines match &ldquo;{routineSearchQuery}&rdquo;
                </div>
              ) : (
                <div className="space-y-2">
                  {filteredRoutines.map((r) => (
                    <div
                      key={r.id}
                      onClick={() => setSelectedRoutine(r)}
                      className={`p-3.5 bg-[#171717] border rounded-xl flex items-center justify-between text-xs text-neutral-300 transition cursor-pointer ${
                        selectedRoutine?.id === r.id
                          ? "border-neutral-500 bg-[#1a1a1a]"
                          : "border-[#262626] hover:border-neutral-700"
                      }`}
                    >
                      <div className="flex items-center gap-2.5">
                        <span className="w-2 h-2 rounded-full bg-neutral-400"></span>
                        <div>
                          <div className="font-semibold text-neutral-100">{r.name}</div>
                          <div className="text-[11px] text-neutral-400 font-mono">
                            Schedule: {r.cron_expression} • Target: {r.target_id}
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
                        <button
                          onClick={() => promptDeleteRoutine(r)}
                          className="p-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                          title="Delete task"
                          aria-label="Delete task"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* EXPLICIT CONFIRMATION DIALOG */}
      {taskToDelete && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 backdrop-blur-md p-4">
          <div
            className="w-full max-w-md bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl p-6 space-y-4"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-dialog-title"
          >
            <div className="flex items-center justify-between border-b border-[#262626] pb-3">
              <h3 id="delete-dialog-title" className="text-base font-semibold text-neutral-100">
                Delete task?
              </h3>
              <button
                onClick={handleCancelDelete}
                disabled={isDeleting}
                className="p-1 text-neutral-400 hover:text-neutral-100 rounded-lg disabled:opacity-50"
                aria-label="Cancel deletion"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="text-xs text-neutral-300 leading-relaxed space-y-2">
              <p>
                Are you sure you want to delete &ldquo;{taskToDelete.name}&rdquo;? This action cannot be
                undone.
              </p>
              {taskToDelete.type === "skill" && taskToDelete.linkedRoutinesCount > 0 && (
                <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-xl text-neutral-400 flex items-start gap-2">
                  <Info className="w-4 h-4 text-neutral-300 shrink-0 mt-0.5" />
                  <span>
                    {taskToDelete.linkedRoutinesCount} associated recurring routine
                    {taskToDelete.linkedRoutinesCount > 1 ? "s" : ""} targeting this skill definition will
                    also be cleaned up.
                  </span>
                </div>
              )}
            </div>

            {deleteError && (
              <div className="p-3 bg-red-950/40 border border-red-500/40 rounded-xl text-xs text-red-300 flex items-center gap-2">
                <AlertTriangle className="w-4 h-4 text-red-400 shrink-0" />
                <span>{deleteError}</span>
              </div>
            )}

            <div className="flex items-center justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={handleCancelDelete}
                disabled={isDeleting}
                data-testid="cancel-delete-task-button"
                className="px-4 py-2 bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border border-neutral-700 rounded-xl text-xs font-medium transition disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConfirmDelete}
                disabled={isDeleting}
                data-testid="confirm-delete-task-button"
                className="px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-xl text-xs font-semibold flex items-center gap-2 transition shadow disabled:opacity-50"
              >
                {isDeleting ? "Deleting..." : "Delete task"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};