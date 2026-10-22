import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Code,
  Server,
  GitCommit,
  Stethoscope,
  Eye,
  Users,
  Play,
  Square,
  RotateCcw,
  CheckCircle,
  AlertTriangle,
  FolderGit2,
  RefreshCw,
  Terminal,
  X,
  Copy,
  Check,
  Smartphone,
  Tablet,
  Monitor,
} from "lucide-react";
import {
  DiscoveryResult,
  DevServerState,
  CodeCheckpoint,
  RollbackResult,
  BuildDoctorDiagnostic,
  VisualBaseline,
  VisualRegressionResult,
  MultiAgentWorkspace,
  ReconciliationOutcome,
} from "../types/agent";

interface DevWorkbenchModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialTab?: "projects" | "checkpoints" | "doctor" | "audit" | "workspace";
}

export const DevWorkbenchModal: React.FC<DevWorkbenchModalProps> = ({
  isOpen,
  onClose,
  initialTab = "projects",
}) => {
  const [activeTab, setActiveTab] = useState<
    "projects" | "checkpoints" | "doctor" | "audit" | "workspace"
  >(initialTab);

  // --- Projects & Dev Servers State ---
  const [searchRoot, setSearchRoot] = useState<string>("/home/snigdha/Desktop/Kyro");
  const [projectName, setProjectName] = useState<string>("kyro");
  const [isDiscovering, setIsDiscovering] = useState<boolean>(false);
  const [discoveryResult, setDiscoveryResult] = useState<DiscoveryResult | null>(null);
  const [devServers, setDevServers] = useState<DevServerState[]>([]);
  const [selectedServerLogs, setSelectedServerLogs] = useState<{ id: string; lines: string[] } | null>(
    null
  );
  const [serverActionError, setServerActionError] = useState<string | null>(null);

  // --- Checkpoints State ---
  const [checkpointWorkspace, setCheckpointWorkspace] = useState<string>(
    "/home/snigdha/Desktop/Kyro"
  );
  const [checkpointDesc, setCheckpointDesc] = useState<string>("Pre-refactor checkpoint");
  const [checkpoints, setCheckpoints] = useState<CodeCheckpoint[]>([]);
  const [selectedCheckpoint, setSelectedCheckpoint] = useState<CodeCheckpoint | null>(null);
  const [isCreatingCheckpoint, setIsCreatingCheckpoint] = useState<boolean>(false);
  const [rollbackResult, setRollbackResult] = useState<RollbackResult | null>(null);
  const [checkpointError, setCheckpointError] = useState<string | null>(null);

  // --- Build Doctor State ---
  const [doctorWorkingDir, setDoctorWorkingDir] = useState<string>("/home/snigdha/Desktop/Kyro");
  const [doctorCommand, setDoctorCommand] = useState<string>("cargo test");
  const [doctorRawOutput, setDoctorRawOutput] = useState<string>(
    `error[E0425]: cannot find value \`non_existent_var\` in this scope\n  --> src/main.rs:42:15\n   |\n42 |     let _ = non_existent_var;\n   |             ^^^^^^^^^^^^^^^^ not found in this scope`
  );
  const [isDiagnosing, setIsDiagnosing] = useState<boolean>(false);
  const [diagnostic, setDiagnostic] = useState<BuildDoctorDiagnostic | null>(null);
  const [copiedRepair, setCopiedRepair] = useState<boolean>(false);

  // --- Visual Audit & Baselines State ---
  const [auditRoute, setAuditRoute] = useState<string>("/");
  const [auditViewport, setAuditViewport] = useState<"desktop" | "tablet" | "mobile">("desktop");
  const [auditProjectPath, setAuditProjectPath] = useState<string>("/home/snigdha/Desktop/Kyro");
  const [currentBaseline, setCurrentBaseline] = useState<VisualBaseline | null>(null);
  const [regressionResult, setRegressionResult] = useState<VisualRegressionResult | null>(null);
  const [baselineStatusNotice, setBaselineStatusNotice] = useState<string | null>(null);

  // --- Multi-Agent Workspace State ---
  const [agentObjective, setAgentObjective] = useState<string>(
    "Implement production visual regression suite and verify zero clippy warnings"
  );
  const [agentProjectPath, setAgentProjectPath] = useState<string>("/home/snigdha/Desktop/Kyro");
  const [multiAgentWorkspace, setMultiAgentWorkspace] = useState<MultiAgentWorkspace | null>(null);
  const [reconciliation, setReconciliation] = useState<ReconciliationOutcome | null>(null);
  const [isCreatingWorkspace, setIsCreatingWorkspace] = useState<boolean>(false);
  const [isReconciling, setIsReconciling] = useState<boolean>(false);

  useEffect(() => {
    if (isOpen) {
      loadDevServers();
    }
  }, [isOpen]);

  const loadDevServers = async () => {
    try {
      const servers = await invoke<DevServerState[]>("list_dev_servers");
      setDevServers(Array.isArray(servers) ? servers : []);
    } catch {
      setDevServers([]);
    }
  };

  const handleDiscoverProjects = async () => {
    setIsDiscovering(true);
    setServerActionError(null);
    try {
      const res = await invoke<DiscoveryResult>("discover_projects", {
        name: projectName,
        searchRoot: searchRoot.trim() ? searchRoot.trim() : null,
      });
      setDiscoveryResult(res);
    } catch (err: any) {
      setServerActionError(String(err));
    } finally {
      setIsDiscovering(false);
    }
  };

  const handleStartDevServer = async (candidatePath: string) => {
    setServerActionError(null);
    try {
      await invoke("start_dev_server", { projectPath: candidatePath });
      await loadDevServers();
    } catch (err: any) {
      setServerActionError(String(err));
    }
  };

  const handleStopDevServer = async (serverId: string) => {
    try {
      await invoke("stop_dev_server", { serverId });
      await loadDevServers();
      if (selectedServerLogs?.id === serverId) {
        setSelectedServerLogs(null);
      }
    } catch (err: any) {
      setServerActionError(String(err));
    }
  };

  const handleViewLogs = async (serverId: string) => {
    try {
      const lines = await invoke<string[]>("get_dev_server_logs", {
        serverId,
        maxLines: 200,
      });
      setSelectedServerLogs({ id: serverId, lines });
    } catch (err: any) {
      setServerActionError(String(err));
    }
  };

  // --- Checkpoints Handlers ---
  const handleCreateCheckpoint = async () => {
    setIsCreatingCheckpoint(true);
    setCheckpointError(null);
    try {
      const cp = await invoke<CodeCheckpoint>("create_code_checkpoint", {
        workspacePath: checkpointWorkspace,
        description: checkpointDesc,
      });
      setCheckpoints((prev) => [cp, ...prev]);
      setSelectedCheckpoint(cp);
    } catch (err: any) {
      setCheckpointError(String(err));
    } finally {
      setIsCreatingCheckpoint(false);
    }
  };

  const handleRollback = async (cp: CodeCheckpoint) => {
    setCheckpointError(null);
    try {
      const res = await invoke<RollbackResult>("rollback_code_checkpoint", {
        checkpoint: cp,
      });
      setRollbackResult(res);
      setCheckpoints((prev) =>
        prev.map((item) =>
          item.checkpoint_id === cp.checkpoint_id
            ? { ...item, status: "rolled_back" as const }
            : item
        )
      );
      if (selectedCheckpoint?.checkpoint_id === cp.checkpoint_id) {
        setSelectedCheckpoint((prev) => (prev ? { ...prev, status: "rolled_back" } : null));
      }
    } catch (err: any) {
      setCheckpointError(String(err));
    }
  };

  // --- Build Doctor Handlers ---
  const handleDiagnose = async () => {
    setIsDiagnosing(true);
    setCopiedRepair(false);
    try {
      const res = await invoke<BuildDoctorDiagnostic>("diagnose_build_failure", {
        workingDir: doctorWorkingDir,
        command: doctorCommand,
        output: doctorRawOutput,
      });
      setDiagnostic(res);
    } catch (err: any) {
      alert("Diagnostic failed: " + String(err));
    } finally {
      setIsDiagnosing(false);
    }
  };

  const handleCopyRepair = () => {
    if (diagnostic?.suggested_repair) {
      navigator.clipboard.writeText(diagnostic.suggested_repair);
      setCopiedRepair(true);
      setTimeout(() => setCopiedRepair(false), 2000);
    }
  };

  // --- Baseline Handlers ---
  const handleInspectBaseline = async () => {
    setBaselineStatusNotice(null);
    try {
      const res = await invoke<VisualBaseline | null>("get_audit_baseline", {
        projectPath: auditProjectPath,
        route: auditRoute,
        viewport: auditViewport,
      });
      setCurrentBaseline(res);
      if (!res) {
        setBaselineStatusNotice(`No baseline registered yet for ${auditRoute} [${auditViewport}]`);
      }
    } catch (err: any) {
      setBaselineStatusNotice("Error: " + String(err));
    }
  };

  const handleSaveDemoBaseline = async () => {
    setBaselineStatusNotice(null);
    try {
      // 1x1 base64 transparent PNG for deterministic baseline verification
      const samplePng =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
      const res = await invoke<VisualBaseline>("set_audit_baseline", {
        projectPath: auditProjectPath,
        route: auditRoute,
        viewport: auditViewport,
        base64Png: samplePng,
        artifactId: null,
      });
      setCurrentBaseline(res);
      setBaselineStatusNotice(`Saved baseline for ${auditRoute} [${auditViewport}]`);
    } catch (err: any) {
      setBaselineStatusNotice("Error: " + String(err));
    }
  };

  const handleCompareBaseline = async () => {
    if (!currentBaseline) return;
    try {
      const samplePng =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
      const res = await invoke<VisualRegressionResult>("compare_visual_baseline", {
        baselineB64: currentBaseline.base64_png,
        currentB64: samplePng,
        route: auditRoute,
        viewport: auditViewport,
      });
      setRegressionResult(res);
    } catch (err: any) {
      setBaselineStatusNotice("Comparison error: " + String(err));
    }
  };

  // --- Multi-Agent Handlers ---
  const handleCreateMultiAgentWorkspace = async () => {
    setIsCreatingWorkspace(true);
    try {
      const res = await invoke<MultiAgentWorkspace>("create_multi_agent_workspace", {
        parentRunId: "run-" + Date.now(),
        projectPath: agentProjectPath,
        objective: agentObjective,
      });
      setMultiAgentWorkspace(res);
      setReconciliation(null);
    } catch (err: any) {
      alert("Failed to initialize multi-agent workspace: " + String(err));
    } finally {
      setIsCreatingWorkspace(false);
    }
  };

  const handleReconcileWorkspace = async () => {
    if (!multiAgentWorkspace) return;
    setIsReconciling(true);
    try {
      const res = await invoke<ReconciliationOutcome>("reconcile_multi_agent_workspace", {
        projectRoot: multiAgentWorkspace.project_path,
        worktreePath: `${multiAgentWorkspace.project_path}/.kyro-worktrees/implementation_engineer`,
        workspaceId: multiAgentWorkspace.workspace_id,
      });
      setReconciliation(res);
    } catch (err: any) {
      alert("Reconciliation error: " + String(err));
    } finally {
      setIsReconciling(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 animate-in fade-in"
      role="dialog"
      aria-modal="true"
    >
      <div className="w-full max-w-5xl h-[88vh] bg-[#101010] border border-neutral-800 rounded-2xl shadow-2xl flex flex-col overflow-hidden">
        {/* Header */}
        <div className="px-6 py-4 border-b border-neutral-800 flex items-center justify-between bg-[#141414]">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-purple-950/40 border border-purple-800/60 flex items-center justify-center text-purple-300">
              <Code className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-neutral-100 flex items-center gap-2">
                <span>Kyro Developer Workbench</span>
                <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-purple-900/40 text-purple-300 border border-purple-800/60 font-medium">
                  DEV MODE
                </span>
              </h2>
              <p className="text-xs text-neutral-400">
                Project profiling, dev servers, reversible checkpoints, build diagnostics & visual regression
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-neutral-100 rounded-lg hover:bg-neutral-800 transition"
            aria-label="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tab Navigation */}
        <div className="flex border-b border-neutral-800 bg-[#0d0d0d] px-6">
          <button
            onClick={() => setActiveTab("projects")}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition flex items-center gap-2 ${
              activeTab === "projects"
                ? "border-purple-500 text-purple-300 bg-purple-950/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Server className="w-4 h-4" /> Projects & Dev Servers
          </button>
          <button
            onClick={() => setActiveTab("checkpoints")}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition flex items-center gap-2 ${
              activeTab === "checkpoints"
                ? "border-purple-500 text-purple-300 bg-purple-950/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <GitCommit className="w-4 h-4" /> Checkpoints & Diffs
          </button>
          <button
            onClick={() => setActiveTab("doctor")}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition flex items-center gap-2 ${
              activeTab === "doctor"
                ? "border-purple-500 text-purple-300 bg-purple-950/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Stethoscope className="w-4 h-4" /> Build Doctor
          </button>
          <button
            onClick={() => setActiveTab("audit")}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition flex items-center gap-2 ${
              activeTab === "audit"
                ? "border-purple-500 text-purple-300 bg-purple-950/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Eye className="w-4 h-4" /> UI Audit & Baselines
          </button>
          <button
            onClick={() => setActiveTab("workspace")}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition flex items-center gap-2 ${
              activeTab === "workspace"
                ? "border-purple-500 text-purple-300 bg-purple-950/20"
                : "border-transparent text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Users className="w-4 h-4" /> Multi-Agent Workspace
          </button>
        </div>

        {/* Content Area */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* TAB 1: Projects & Dev Servers */}
          {activeTab === "projects" && (
            <div className="space-y-6">
              {/* Search Bar */}
              <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Project Discovery & Profiling
                </div>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                  <div className="md:col-span-2">
                    <label className="text-[11px] text-neutral-400 block mb-1">Search Root Directory</label>
                    <input
                      type="text"
                      value={searchRoot}
                      onChange={(e) => setSearchRoot(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Project Name Query</label>
                    <input
                      type="text"
                      value={projectName}
                      onChange={(e) => setProjectName(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                </div>
                <div className="flex justify-end">
                  <button
                    onClick={handleDiscoverProjects}
                    disabled={isDiscovering}
                    className="px-4 py-2 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-2 transition disabled:opacity-50"
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${isDiscovering ? "animate-spin" : ""}`} />
                    {isDiscovering ? "Scanning Filesystem..." : "Discover Projects"}
                  </button>
                </div>
              </div>

              {serverActionError && (
                <div className="p-3 bg-red-950/40 border border-red-500/40 rounded-xl text-xs text-red-300 flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 text-red-400 shrink-0" />
                  <span>{serverActionError}</span>
                </div>
              )}

              {/* Discovery Candidates Table */}
              {discoveryResult && (
                <div className="space-y-3">
                  <div className="flex items-center justify-between text-xs text-neutral-400">
                    <span>
                      Found <strong>{discoveryResult.candidates.length}</strong> project candidate(s) in{" "}
                      <code>{discoveryResult.searched_root}</code>
                    </span>
                  </div>
                  <div className="space-y-2">
                    {discoveryResult.candidates.map((cand, idx) => (
                      <div
                        key={idx}
                        className="p-4 bg-[#141414] border border-neutral-800 rounded-xl flex items-center justify-between gap-4"
                      >
                        <div className="min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="font-semibold text-neutral-100 text-sm">{cand.name}</span>
                            <span className="px-2 py-0.5 rounded text-[10px] uppercase font-mono bg-neutral-800 text-neutral-300 border border-neutral-700">
                              {cand.kind}
                            </span>
                            {cand.package_manager && (
                              <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-purple-950/60 text-purple-300 border border-purple-800/60">
                                {cand.package_manager}
                              </span>
                            )}
                            {cand.is_git_repo && (
                              <span className="flex items-center gap-1 text-[10px] text-neutral-400 font-mono">
                                <FolderGit2 className="w-3 h-3 text-neutral-400" /> Git
                              </span>
                            )}
                          </div>
                          <div className="text-xs text-neutral-400 font-mono truncate mt-1">
                            {cand.path}
                          </div>
                          {cand.scripts.length > 0 && (
                            <div className="flex items-center gap-1.5 mt-2 flex-wrap">
                              <span className="text-[10px] text-neutral-400">Scripts:</span>
                              {cand.scripts.map((s) => (
                                <span
                                  key={s}
                                  className="px-1.5 py-0.5 bg-neutral-900 text-neutral-300 text-[10px] font-mono rounded border border-neutral-800"
                                >
                                  {s}
                                </span>
                              ))}
                            </div>
                          )}
                        </div>

                        <button
                          onClick={() => handleStartDevServer(cand.path)}
                          className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 font-medium text-xs flex items-center gap-1.5 transition shrink-0"
                        >
                          <Play className="w-3.5 h-3.5 fill-current" /> Start Dev Server
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Managed Dev Servers */}
              <div className="space-y-3 pt-2">
                <div className="flex items-center justify-between">
                  <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide flex items-center gap-2">
                    <Server className="w-4 h-4 text-purple-400" />
                    <span>Managed Development Servers ({(devServers || []).length})</span>
                  </div>
                  <button
                    onClick={loadDevServers}
                    className="text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1"
                  >
                    <RefreshCw className="w-3 h-3" /> Refresh
                  </button>
                </div>

                {(devServers || []).length === 0 ? (
                  <div className="p-6 text-center border border-dashed border-neutral-800 rounded-xl text-neutral-400 text-xs">
                    No active development servers. Discover a project above and click &ldquo;Start Dev Server&rdquo;.
                  </div>
                ) : (
                  <div className="space-y-3">
                    {(devServers || []).map((srv) => (
                      <div
                        key={srv.id}
                        className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-3"
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2.5">
                            <span
                              className={`w-2.5 h-2.5 rounded-full ${
                                srv.status === "ready"
                                  ? "bg-green-400 animate-pulse"
                                  : srv.status === "starting"
                                  ? "bg-amber-400 animate-spin"
                                  : "bg-red-400"
                              }`}
                            />
                            <span className="font-semibold text-neutral-200 text-sm">
                              Server #{srv.id}
                            </span>
                            <span className="text-xs font-mono text-neutral-400">PID: {srv.pid}</span>
                            {srv.port && (
                              <span className="px-2 py-0.5 rounded text-[11px] font-mono bg-purple-950/60 text-purple-300 border border-purple-800/60">
                                Port :{srv.port}
                              </span>
                            )}
                            {srv.ready_url && (
                              <a
                                href={srv.ready_url}
                                target="_blank"
                                rel="noreferrer"
                                className="text-xs text-purple-400 underline font-mono ml-1"
                              >
                                {srv.ready_url}
                              </a>
                            )}
                          </div>
                          <div className="flex items-center gap-2">
                            <button
                              onClick={() => handleViewLogs(srv.id)}
                              className="px-2.5 py-1 rounded bg-neutral-900 hover:bg-neutral-800 text-neutral-300 border border-neutral-700 text-xs flex items-center gap-1.5 transition"
                            >
                              <Terminal className="w-3 h-3" /> Logs
                            </button>
                            <button
                              onClick={() => handleStopDevServer(srv.id)}
                              className="px-2.5 py-1 rounded bg-red-950/60 hover:bg-red-900/60 text-red-200 border border-red-800/60 text-xs flex items-center gap-1.5 transition"
                            >
                              <Square className="w-3 h-3 fill-current" /> Stop
                            </button>
                          </div>
                        </div>

                        <div className="text-xs font-mono text-neutral-400 bg-neutral-950 p-2 rounded border border-neutral-900">
                          $ {srv.command}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Server Log Tail Viewer */}
              {selectedServerLogs && (
                <div className="p-4 bg-neutral-950 border border-neutral-800 rounded-xl space-y-2">
                  <div className="flex items-center justify-between text-xs border-b border-neutral-800 pb-2">
                    <span className="font-mono text-neutral-300">
                      Live Output: Server #{selectedServerLogs.id}
                    </span>
                    <button
                      onClick={() => setSelectedServerLogs(null)}
                      className="text-neutral-400 hover:text-neutral-200"
                    >
                      Close
                    </button>
                  </div>
                  <div className="h-48 overflow-y-auto font-mono text-[11px] text-neutral-300 space-y-0.5">
                    {selectedServerLogs.lines.length === 0 ? (
                      <span className="text-neutral-500 italic">No output yet.</span>
                    ) : (
                      selectedServerLogs.lines.map((line, i) => (
                        <div key={i} className="whitespace-pre-wrap">
                          {line}
                        </div>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* TAB 2: Checkpoints & Diffs */}
          {activeTab === "checkpoints" && (
            <div className="space-y-6">
              {/* Checkpoint Creator */}
              <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Create Reversible Code Checkpoint
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Workspace Directory</label>
                    <input
                      type="text"
                      value={checkpointWorkspace}
                      onChange={(e) => setCheckpointWorkspace(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Checkpoint Summary</label>
                    <input
                      type="text"
                      value={checkpointDesc}
                      onChange={(e) => setCheckpointDesc(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200"
                    />
                  </div>
                </div>
                <div className="flex justify-end">
                  <button
                    onClick={handleCreateCheckpoint}
                    disabled={isCreatingCheckpoint}
                    className="px-4 py-2 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-2 transition disabled:opacity-50"
                  >
                    <GitCommit className="w-3.5 h-3.5" />
                    {isCreatingCheckpoint ? "Recording Diff..." : "Save Checkpoint"}
                  </button>
                </div>
              </div>

              {checkpointError && (
                <div className="p-3 bg-red-950/40 border border-red-500/40 rounded-xl text-xs text-red-300 flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 text-red-400 shrink-0" />
                  <span>{checkpointError}</span>
                </div>
              )}

              {rollbackResult && (
                <div className="p-4 bg-green-950/40 border border-green-500/40 rounded-xl space-y-2 text-xs">
                  <div className="flex items-center gap-2 text-green-300 font-semibold">
                    <CheckCircle className="w-4 h-4 text-green-400" />
                    <span>Checkpoint Rollback {rollbackResult.status.toUpperCase()}</span>
                  </div>
                  <div className="text-neutral-300">
                    Reverted <strong>{rollbackResult.files_reverted.length}</strong> file(s) safely.
                  </div>
                  {rollbackResult.conflicts_detected.length > 0 && (
                    <div className="text-amber-300">
                      Conflicts preserved: {rollbackResult.conflicts_detected.join(", ")}
                    </div>
                  )}
                </div>
              )}

              {/* Checkpoints List */}
              <div className="space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Active & Historical Checkpoints ({checkpoints.length})
                </div>

                {checkpoints.length === 0 ? (
                  <div className="p-6 text-center border border-dashed border-neutral-800 rounded-xl text-neutral-400 text-xs">
                    No checkpoints recorded in this session. Create a checkpoint above to preserve working files before making risky agent modifications.
                  </div>
                ) : (
                  <div className="grid grid-cols-1 gap-3">
                    {checkpoints.map((cp) => (
                      <div
                        key={cp.checkpoint_id}
                        className={`p-4 rounded-xl border transition ${
                          selectedCheckpoint?.checkpoint_id === cp.checkpoint_id
                            ? "bg-[#161616] border-purple-500/80 shadow-sm"
                            : "bg-[#141414] border-neutral-800 hover:border-neutral-700"
                        }`}
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-3">
                            <GitCommit className="w-4 h-4 text-purple-400" />
                            <div>
                              <div className="text-sm font-semibold text-neutral-200">
                                {cp.description}
                              </div>
                              <div className="text-xs text-neutral-400 font-mono">
                                ID: {cp.checkpoint_id} • {new Date(cp.created_at * 1000).toLocaleTimeString()}
                              </div>
                            </div>
                          </div>

                          <div className="flex items-center gap-2">
                            <span
                              className={`px-2 py-0.5 rounded text-[10px] font-mono uppercase font-bold ${
                                cp.status === "active"
                                  ? "bg-green-950/60 text-green-300 border border-green-800/60"
                                  : "bg-neutral-800 text-neutral-400 border border-neutral-700"
                              }`}
                            >
                              {cp.status}
                            </span>
                            {cp.status === "active" && (
                              <button
                                onClick={() => handleRollback(cp)}
                                className="px-3 py-1.5 rounded-lg bg-red-950/60 hover:bg-red-900/60 text-red-200 border border-red-800/60 text-xs font-medium flex items-center gap-1.5 transition"
                              >
                                <RotateCcw className="w-3.5 h-3.5" /> 1-Click Rollback
                              </button>
                            )}
                          </div>
                        </div>

                        {/* Modified Files & Diffs */}
                        {cp.modified_files.length > 0 && (
                          <div className="mt-3 pt-3 border-t border-neutral-800/80 space-y-2">
                            <div className="text-[11px] font-semibold text-neutral-300">
                              Modified Files ({cp.modified_files.length}):
                            </div>
                            {cp.modified_files.map((fileDiff, fIdx) => (
                              <div
                                key={fIdx}
                                className="p-2.5 bg-neutral-950 rounded-lg border border-neutral-800 text-xs space-y-2"
                              >
                                <div className="flex items-center justify-between">
                                  <span className="font-mono text-neutral-200 font-medium">
                                    {fileDiff.file_path}
                                  </span>
                                  <div className="flex items-center gap-2 text-[10px] font-mono">
                                    <span className="text-green-400">+{fileDiff.additions}</span>
                                    <span className="text-red-400">-{fileDiff.deletions}</span>
                                    <span
                                      className={`px-1.5 py-0.5 rounded uppercase ${
                                        fileDiff.risk_assessment === "high"
                                          ? "bg-red-950 text-red-300"
                                          : fileDiff.risk_assessment === "medium"
                                          ? "bg-amber-950 text-amber-300"
                                          : "bg-neutral-800 text-neutral-300"
                                      }`}
                                    >
                                      {fileDiff.risk_assessment} risk
                                    </span>
                                  </div>
                                </div>

                                {fileDiff.unified_diff && (
                                  <div className="max-h-36 overflow-y-auto p-2 bg-[#0c0c0c] rounded font-mono text-[10px] space-y-0.5 border border-neutral-900">
                                    {fileDiff.unified_diff.split("\n").map((line, lIdx) => (
                                      <div
                                        key={lIdx}
                                        className={
                                          line.startsWith("+")
                                            ? "text-green-400"
                                            : line.startsWith("-")
                                            ? "text-red-400"
                                            : "text-neutral-400"
                                        }
                                      >
                                        {line}
                                      </div>
                                    ))}
                                  </div>
                                )}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {/* TAB 3: Build Doctor */}
          {activeTab === "doctor" && (
            <div className="space-y-6">
              <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Automated Failure Diagnostic Engine
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Working Directory</label>
                    <input
                      type="text"
                      value={doctorWorkingDir}
                      onChange={(e) => setDoctorWorkingDir(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Failing Command</label>
                    <input
                      type="text"
                      value={doctorCommand}
                      onChange={(e) => setDoctorCommand(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                </div>

                <div>
                  <label className="text-[11px] text-neutral-400 block mb-1">
                    Compiler / Test Error Log (Rust, TypeScript, Vitest)
                  </label>
                  <textarea
                    rows={5}
                    value={doctorRawOutput}
                    onChange={(e) => setDoctorRawOutput(e.target.value)}
                    className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                  />
                </div>

                <div className="flex justify-end">
                  <button
                    onClick={handleDiagnose}
                    disabled={isDiagnosing}
                    className="px-4 py-2 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-2 transition disabled:opacity-50"
                  >
                    <Stethoscope className="w-3.5 h-3.5" />
                    {isDiagnosing ? "Diagnosing..." : "Run Build Doctor"}
                  </button>
                </div>
              </div>

              {diagnostic && (
                <div className="p-5 bg-[#141414] border border-neutral-800 rounded-xl space-y-4">
                  <div className="flex items-center justify-between border-b border-neutral-800 pb-3">
                    <div className="flex items-center gap-2">
                      <span className="px-2 py-0.5 rounded text-[10px] uppercase font-mono font-bold bg-red-950/60 text-red-300 border border-red-800/60">
                        {diagnostic.category.toUpperCase()}
                      </span>
                      {diagnostic.error_code && (
                        <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-neutral-800 text-neutral-300 border border-neutral-700">
                          {diagnostic.error_code}
                        </span>
                      )}
                    </div>
                    {diagnostic.file && (
                      <span className="text-xs font-mono text-neutral-400">
                        {diagnostic.file}:{diagnostic.line_number ?? 1}
                      </span>
                    )}
                  </div>

                  <div className="space-y-1">
                    <div className="text-xs font-semibold text-neutral-300">Root Cause:</div>
                    <div className="text-xs text-neutral-200 leading-relaxed bg-neutral-950 p-3 rounded-lg border border-neutral-800 font-mono">
                      {diagnostic.root_cause_error}
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-semibold text-neutral-300">Suggested Code Repair:</span>
                      <button
                        onClick={handleCopyRepair}
                        className="text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1"
                      >
                        {copiedRepair ? <Check className="w-3.5 h-3.5 text-green-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>{copiedRepair ? "Copied" : "Copy Repair"}</span>
                      </button>
                    </div>
                    <div className="p-3 bg-neutral-950 border border-neutral-800 rounded-lg text-xs font-mono text-purple-300 whitespace-pre-wrap">
                      {diagnostic.suggested_repair}
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* TAB 4: UI Audit & Baselines */}
          {activeTab === "audit" && (
            <div className="space-y-6">
              <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Responsive Viewport & Visual Regression Suite
                </div>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Target Route</label>
                    <input
                      type="text"
                      value={auditRoute}
                      onChange={(e) => setAuditRoute(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Project Root</label>
                    <input
                      type="text"
                      value={auditProjectPath}
                      onChange={(e) => setAuditProjectPath(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Audit Viewport</label>
                    <div className="grid grid-cols-3 gap-1">
                      <button
                        onClick={() => setAuditViewport("desktop")}
                        className={`p-2 rounded text-xs flex items-center justify-center gap-1 border transition ${
                          auditViewport === "desktop"
                            ? "bg-purple-950/40 text-purple-200 border-purple-700"
                            : "bg-neutral-900 text-neutral-400 border-neutral-700"
                        }`}
                      >
                        <Monitor className="w-3 h-3" /> 1280
                      </button>
                      <button
                        onClick={() => setAuditViewport("tablet")}
                        className={`p-2 rounded text-xs flex items-center justify-center gap-1 border transition ${
                          auditViewport === "tablet"
                            ? "bg-purple-950/40 text-purple-200 border-purple-700"
                            : "bg-neutral-900 text-neutral-400 border-neutral-700"
                        }`}
                      >
                        <Tablet className="w-3 h-3" /> 768
                      </button>
                      <button
                        onClick={() => setAuditViewport("mobile")}
                        className={`p-2 rounded text-xs flex items-center justify-center gap-1 border transition ${
                          auditViewport === "mobile"
                            ? "bg-purple-950/40 text-purple-200 border-purple-700"
                            : "bg-neutral-900 text-neutral-400 border-neutral-700"
                        }`}
                      >
                        <Smartphone className="w-3 h-3" /> 375
                      </button>
                    </div>
                  </div>
                </div>

                <div className="flex items-center justify-end gap-2 pt-1">
                  <button
                    onClick={handleInspectBaseline}
                    className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 text-xs transition"
                  >
                    Inspect Stored Baseline
                  </button>
                  <button
                    onClick={handleSaveDemoBaseline}
                    className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 text-xs transition"
                  >
                    Save As Baseline
                  </button>
                  <button
                    onClick={handleCompareBaseline}
                    disabled={!currentBaseline}
                    className="px-4 py-1.5 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-1.5 transition disabled:opacity-50"
                  >
                    <Eye className="w-3.5 h-3.5" /> Compare Against Baseline
                  </button>
                </div>
              </div>

              {baselineStatusNotice && (
                <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-300">
                  {baselineStatusNotice}
                </div>
              )}

              {currentBaseline && (
                <div className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-2">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-semibold text-neutral-200">
                      Baseline for Route {currentBaseline.route} ({currentBaseline.viewport})
                    </span>
                    <span className="text-neutral-400 font-mono">
                      Recorded: {new Date(currentBaseline.created_at * 1000).toLocaleString()}
                    </span>
                  </div>
                  <div className="text-xs text-neutral-400">
                    Baseline is persisted in local trusted registry (<code>~/.config/kyro/baselines/</code>).
                  </div>
                </div>
              )}

              {regressionResult && (
                <div className="p-5 bg-[#141414] border border-neutral-800 rounded-xl space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] uppercase font-mono font-bold ${
                          regressionResult.has_regression
                            ? "bg-red-950/60 text-red-300 border border-red-800/60"
                            : "bg-green-950/60 text-green-300 border border-green-800/60"
                        }`}
                      >
                        {regressionResult.has_regression ? "Visual Regression Detected" : "Visual Match Passed"}
                      </span>
                      <span className="text-xs font-mono text-neutral-300">
                        Diff: {regressionResult.diff_percent.toFixed(2)}%
                      </span>
                    </div>
                  </div>
                  <p className="text-xs text-neutral-300 leading-relaxed">
                    {regressionResult.description}
                  </p>
                </div>
              )}
            </div>
          )}

          {/* TAB 5: Multi-Agent Workspace */}
          {activeTab === "workspace" && (
            <div className="space-y-6">
              <div className="p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3">
                <div className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                  Specialist Multi-Agent Swarm (5 Roles, Git Worktree Isolation)
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Project Root</label>
                    <input
                      type="text"
                      value={agentProjectPath}
                      onChange={(e) => setAgentProjectPath(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200 font-mono"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-neutral-400 block mb-1">Swarm Objective</label>
                    <input
                      type="text"
                      value={agentObjective}
                      onChange={(e) => setAgentObjective(e.target.value)}
                      className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-lg text-xs text-neutral-200"
                    />
                  </div>
                </div>

                <div className="flex justify-end">
                  <button
                    onClick={handleCreateMultiAgentWorkspace}
                    disabled={isCreatingWorkspace}
                    className="px-4 py-2 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-2 transition disabled:opacity-50"
                  >
                    <Users className="w-3.5 h-3.5" />
                    {isCreatingWorkspace ? "Creating Isolated Worktrees..." : "Initialize Specialist Workspace"}
                  </button>
                </div>
              </div>

              {multiAgentWorkspace && (
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-neutral-200 uppercase tracking-wide">
                      Specialist Task Graph & Worktrees
                    </span>
                    <button
                      onClick={handleReconcileWorkspace}
                      disabled={isReconciling}
                      className="px-3 py-1.5 rounded-lg bg-purple-950/60 hover:bg-purple-900/60 text-purple-200 border border-purple-800/60 text-xs font-medium flex items-center gap-1.5 transition"
                    >
                      <FolderGit2 className="w-3.5 h-3.5" />
                      {isReconciling ? "Reconciling..." : "Reconcile Worktree Changes"}
                    </button>
                  </div>

                  <div className="grid grid-cols-1 gap-3">
                    {multiAgentWorkspace.tasks.map((task) => (
                      <div
                        key={task.task_id}
                        className="p-4 bg-[#141414] border border-neutral-800 rounded-xl space-y-2"
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2.5">
                            <span className="font-semibold text-sm text-neutral-200 uppercase font-mono">
                              {task.role.replace(/_/g, " ")}
                            </span>
                            <span className="px-2 py-0.5 rounded text-[10px] font-mono uppercase bg-neutral-800 text-neutral-300">
                              {task.status}
                            </span>
                          </div>
                          {task.isolated_worktree_path && (
                            <span className="text-[11px] font-mono text-neutral-400">
                              Worktree: {task.isolated_worktree_path.split("/").slice(-2).join("/")}
                            </span>
                          )}
                        </div>

                        <p className="text-xs text-neutral-300">{task.objective}</p>

                        {task.dependencies.length > 0 && (
                          <div className="text-[10px] text-neutral-400 flex items-center gap-1">
                            <span>Dependencies:</span>
                            {task.dependencies.map((d) => (
                              <span key={d} className="px-1.5 py-0.5 bg-neutral-900 rounded font-mono">
                                {d}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {reconciliation && (
                <div className="p-5 bg-[#141414] border border-neutral-800 rounded-xl space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] uppercase font-mono font-bold ${
                          reconciliation.can_safely_merge
                            ? "bg-green-950/60 text-green-300 border border-green-800/60"
                            : "bg-red-950/60 text-red-300 border border-red-800/60"
                        }`}
                      >
                        {reconciliation.can_safely_merge ? "Safe To Merge" : "Conflicts Detected"}
                      </span>
                      <span className="text-xs font-mono text-neutral-300">
                        {reconciliation.modified_files.length} file(s) modified
                      </span>
                    </div>
                  </div>

                  <p className="text-xs text-neutral-300 leading-relaxed">
                    {reconciliation.reconciliation_summary}
                  </p>

                  {reconciliation.unified_diff && (
                    <div className="p-3 bg-neutral-950 border border-neutral-800 rounded-lg max-h-48 overflow-y-auto font-mono text-[10px] space-y-0.5">
                      {reconciliation.unified_diff.split("\n").map((line, i) => (
                        <div
                          key={i}
                          className={
                            line.startsWith("+")
                              ? "text-green-400"
                              : line.startsWith("-")
                              ? "text-red-400"
                              : "text-neutral-400"
                          }
                        >
                          {line}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
