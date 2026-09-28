import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Send, Camera, Square, Maximize2, X, AlertTriangle, Play, ShieldAlert, Check, Ban } from "lucide-react";
import { TakeoverStatus, ApprovalRequest } from "../types/agent";
import { KyroLogo } from "../components/Logo";
import { NvidiaIcon } from "../components/BrandIcons";

export const HudPanel: React.FC = () => {
  const [prompt, setPrompt] = useState("");
  const [isRunning, setIsRunning] = useState(false);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [pendingApprovals, setPendingApprovals] = useState<ApprovalRequest[]>([]);
  const [statusText, setStatusText] = useState("Ready");
  const [takeover, setTakeover] = useState<TakeoverStatus>({
    is_active: false,
    reason: "",
    timestamp: 0,
  });

  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const [status, approvals] = await Promise.all([
          invoke<TakeoverStatus>("get_takeover_status"),
          invoke<ApprovalRequest[]>("list_pending_approvals"),
        ]);
        setTakeover(status);
        setPendingApprovals(approvals.filter((a) => a.status === "pending"));
      } catch (e) {
        // ignore
      }
    }, 800);
    return () => clearInterval(interval);
  }, []);

  const handleResumeTakeover = async () => {
    try {
      await invoke("resume_takeover");
      setTakeover({ is_active: false, reason: "", timestamp: 0 });
    } catch (e) {
      console.error(e);
    }
  };

  const handleResolveApproval = async (id: string, decision: "approved" | "denied") => {
    try {
      await invoke("resolve_approval", { id, decision, resolvedBy: "hud_user" });
      setPendingApprovals((prev) => prev.filter((a) => a.id !== id));
    } catch (e) {
      console.error("Failed to resolve approval from HUD:", e);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!prompt.trim() || isRunning) return;

    const runId = "run_" + crypto.randomUUID();
    setActiveRunId(runId);
    setIsRunning(true);
    setStatusText("Running with Nemotron-3 Ultra...");

    try {
      await invoke("run_agent_turn", {
        runId,
        model: "nvidia/nemotron-3-ultra-550b-a55b",
        history: [{ role: "user", content: prompt }],
        systemPrompt: null,
      });
      setStatusText("Completed");
    } catch (err: any) {
      setStatusText("Error: " + err);
    } finally {
      setIsRunning(false);
      setActiveRunId(null);
      setPrompt("");
    }
  };

  const handleAbort = async () => {
    if (activeRunId) {
      await invoke("cancel_agent_run", { runId: activeRunId }).catch(console.error);
    }
    await invoke("abort_computer_use").catch(console.error);
    setStatusText("Aborted by user");
    setIsRunning(false);
    setActiveRunId(null);
  };

  const openMain = async () => {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const main = await WebviewWindow.getByLabel("main");
    if (main) {
      await main.show();
      await main.setFocus();
    }
  };

  const closeHud = async () => {
    const hud = getCurrentWebviewWindow();
    hud.hide();
  };

  const triggerSnipper = async () => {
    await invoke("toggle_overlay_window", { show: true });
  };

  return (
    <div className="w-full h-full p-3 select-none">
      <div className="w-full h-full bg-[#0a0a0a]/95 backdrop-blur-md border border-[#262626] rounded-2xl shadow-2xl flex flex-col overflow-hidden text-neutral-100">
        {/* HUD Top Bar */}
        <div
          data-tauri-drag-region
          className="flex items-center justify-between px-3.5 py-2.5 bg-[#121212] border-b border-[#262626] cursor-move"
        >
          <div className="flex items-center gap-2">
            <KyroLogo className="w-4 h-4" />
            <span className="text-xs font-semibold tracking-wider uppercase text-neutral-200">Kyro HUD</span>
            <span className="text-[10px] px-1.5 py-0.5 rounded bg-neutral-800 text-neutral-300 border border-neutral-700 font-mono flex items-center gap-1">
              <NvidiaIcon className="w-2.5 h-2.5 text-[#76B900]" />
              <span>Nemotron-3</span>
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              onClick={openMain}
              className="p-1 text-neutral-400 hover:text-neutral-100 rounded hover:bg-neutral-800 transition"
              title="Open Full Workspace"
            >
              <Maximize2 className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={closeHud}
              className="p-1 text-neutral-400 hover:text-neutral-100 rounded hover:bg-neutral-800 transition"
              title="Close HUD"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Human Takeover Active Banner */}
        {takeover.is_active && (
          <div className="px-3 py-2 bg-amber-500/20 border-b border-amber-500/40 flex items-center justify-between text-xs text-amber-200">
            <div className="flex items-center gap-1.5 truncate">
              <AlertTriangle className="w-3.5 h-3.5 text-amber-400 flex-shrink-0 animate-bounce" />
              <span className="truncate font-medium">{takeover.reason || "Human Takeover Required"}</span>
            </div>
            <button
              onClick={handleResumeTakeover}
              className="px-2.5 py-1 rounded-lg bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] font-semibold text-[11px] flex items-center gap-1 shadow transition flex-shrink-0"
            >
              <Play className="w-3 h-3 fill-current" /> Resume
            </button>
          </div>
        )}

        {/* Pending Approval Requests in HUD */}
        {pendingApprovals.length > 0 && (
          <div className="px-3 py-2 bg-red-950/70 border-b border-red-500/50 flex flex-col gap-1 text-xs">
            {pendingApprovals.map((req) => (
              <div key={req.id} className="flex items-center justify-between gap-1">
                <div className="flex items-center gap-1.5 min-w-0 pr-1">
                  <ShieldAlert className="w-3.5 h-3.5 text-red-400 flex-shrink-0 animate-pulse" />
                  <span className="truncate text-[11px] text-red-200">
                    <strong>{req.tool_name}</strong>: {req.summary}
                  </span>
                </div>
                <div className="flex items-center gap-1 flex-shrink-0">
                  <button
                    onClick={() => handleResolveApproval(req.id, "approved")}
                    className="px-2 py-0.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded text-[10px] font-semibold flex items-center gap-0.5 shadow transition"
                    title="Approve Action"
                  >
                    <Check className="w-3 h-3" /> Approve
                  </button>
                  <button
                    onClick={() => handleResolveApproval(req.id, "denied")}
                    className="px-2 py-0.5 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 rounded text-[10px] font-semibold flex items-center gap-0.5 shadow transition"
                    title="Deny Action"
                  >
                    <Ban className="w-3 h-3" /> Deny
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Status display */}
        <div className="px-3.5 py-2 bg-[#0f0f0f] border-b border-[#262626] text-[11px] font-mono text-neutral-400 flex items-center justify-between">
          <span className="truncate">{statusText}</span>
          {isRunning && (
            <button
              onClick={handleAbort}
              className="px-2 py-0.5 rounded bg-red-600 hover:bg-red-500 text-white font-sans text-[10px] font-semibold flex items-center gap-1 transition shadow"
            >
              <Square className="w-2.5 h-2.5 fill-current" /> STOP
            </button>
          )}
        </div>

        {/* Quick action bar */}
        <div className="px-3 py-2 flex items-center gap-2 border-b border-[#262626] bg-[#141414]">
          <button
            onClick={triggerSnipper}
            className="flex-1 py-1.5 px-2 bg-[#171717] hover:bg-neutral-800 border border-[#262626] rounded-lg text-xs font-medium text-neutral-200 flex items-center justify-center gap-1.5 transition"
          >
            <Camera className="w-3.5 h-3.5 text-neutral-300" /> Snip Region
          </button>
        </div>

        {/* Quick command input */}
        <form onSubmit={handleSubmit} className="p-2.5 flex items-center gap-2 bg-[#121212]">
          <input
            type="text"
            placeholder="Direct Computer Use action..."
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            disabled={isRunning}
            className="flex-1 px-3 py-1.5 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-400"
          />
          <button
            type="submit"
            disabled={isRunning || !prompt.trim()}
            className="p-1.5 rounded-lg bg-[#ededed] hover:bg-neutral-300 disabled:opacity-40 text-[#0a0a0a] transition shadow"
          >
            <Send className="w-3.5 h-3.5" />
          </button>
        </form>
      </div>
    </div>
  );
};