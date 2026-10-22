import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Calendar,
  Mail,
  FileText,
  Clock,
  Sparkles,
  Play,
  FolderOpen,
  ExternalLink,
  RefreshCw,
  Plug,
  ArrowRight,
  Shield,
} from "lucide-react";
import { TodayDashboardData, RoutineRunRecord } from "../types/agent";

interface TodayDashboardViewProps {
  onOpenTool: (toolId: string, initialTab?: string) => void;
  onAskPrompt: (prompt: string) => void;
  onOpenConnections: () => void;
  onOpenSettings: (tab?: string) => void;
}

export const TodayDashboardView: React.FC<TodayDashboardViewProps> = ({
  onOpenTool,
  onAskPrompt,
  onOpenConnections,
  onOpenSettings,
}) => {
  const [data, setData] = useState<TodayDashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [runningRoutineId, setRunningRoutineId] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);

  const loadDashboard = async (isManual = false) => {
    if (isManual) setRefreshing(true);
    try {
      const res = await invoke<TodayDashboardData>("get_today_dashboard_data");
      setData(res);
    } catch {
      // In dev or web mode where Tauri backend may be mocked
      setData({
        greeting: "Good morning",
        date_display: new Date().toLocaleDateString(undefined, {
          weekday: "long",
          month: "long",
          day: "numeric",
          year: "numeric",
        }),
        upcoming_events: [],
        email_summary: {
          unread_count: 0,
          recent_subjects: [],
          is_connected: false,
          account_email: null,
        },
        recent_documents: [],
        upcoming_routines: [],
        recent_routine_results: [],
        pending_tasks_count: 0,
        services: [
          {
            service_name: "Google Workspace (Gmail & Calendar)",
            is_connected: false,
            status_text: "Not connected",
            can_connect: true,
          },
          {
            service_name: "GitHub",
            is_connected: false,
            status_text: "Not connected",
            can_connect: true,
          },
        ],
      });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => {
    loadDashboard();
  }, []);

  const handleRunRoutineNow = async (routineId: string, routineName: string) => {
    setRunningRoutineId(routineId);
    setActionNotice(null);
    try {
      const res = await invoke<RoutineRunRecord>("execute_routine_now", { routineId });
      if (res.status === "succeeded") {
        setActionNotice(`Routine '${routineName}' executed successfully.`);
      } else {
        setActionNotice(
          `Routine '${routineName}' finished with status '${res.status}': ${
            res.error || res.summary || "No error details"
          }`
        );
      }
    } catch (e: any) {
      setActionNotice(`Failed to run routine: ${e?.toString() || "Unknown error"}`);
    } finally {
      await loadDashboard();
      setRunningRoutineId(null);
    }
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center p-8 text-neutral-400">
        <div className="flex items-center gap-2">
          <RefreshCw className="w-5 h-5 animate-spin text-neutral-400" />
          <span className="text-sm">Loading your day...</span>
        </div>
      </div>
    );
  }

  const greeting = data?.greeting || "Welcome back";
  const dateDisplay = data?.date_display || "Today";

  return (
    <div className="flex-1 overflow-y-auto p-6 max-w-6xl mx-auto w-full space-y-6">
      {/* Top Banner Greeting */}
      <div className="flex flex-col md:flex-row md:items-center justify-between pb-4 border-b border-neutral-800 gap-4">
        <div>
          <div className="flex items-center gap-2 text-xs font-medium text-neutral-400 uppercase tracking-wider mb-1">
            <Calendar className="w-3.5 h-3.5 text-sky-400" />
            <span>{dateDisplay}</span>
          </div>
          <h1 className="text-2xl font-bold text-neutral-100">{greeting}!</h1>
          <p className="text-xs text-neutral-400 mt-0.5">
            Here is your daily briefing from your local computer and connected accounts.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => loadDashboard(true)}
            disabled={refreshing}
            className="px-3 py-1.5 bg-[#171717] hover:bg-[#222222] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
            title="Refresh dashboard data"
          >
            <RefreshCw className={`w-3.5 h-3.5 text-neutral-400 ${refreshing ? "animate-spin" : ""}`} />
            Refresh
          </button>
          <button
            onClick={() => onOpenSettings("privacy")}
            className="px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs text-neutral-200 flex items-center gap-1.5 transition"
          >
            <Shield className="w-3.5 h-3.5 text-neutral-300" />
            Privacy & Settings
          </button>
        </div>
      </div>

      {actionNotice && (
        <div className="p-3 rounded-lg bg-sky-950/40 border border-sky-800/60 text-sky-200 text-xs flex items-center justify-between">
          <span>{actionNotice}</span>
          <button onClick={() => setActionNotice(null)} className="text-sky-400 hover:text-sky-200 text-xs">
            Dismiss
          </button>
        </div>
      )}

      {/* Pending Authorizations Section */}
      {data?.pending_approvals && data.pending_approvals.length > 0 && (
        <div className="p-4 rounded-xl bg-amber-950/30 border border-amber-800/60 space-y-3 shadow-sm">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Shield className="w-4 h-4 text-amber-400" />
              <h3 className="text-xs font-bold text-amber-200 uppercase tracking-wider">
                Pending Authorizations ({data.pending_approvals.length})
              </h3>
            </div>
            <button
              onClick={() => onOpenSettings("governance")}
              className="text-xs text-amber-300 hover:text-amber-100 underline"
            >
              Manage Governance
            </button>
          </div>
          <div className="space-y-2">
            {data.pending_approvals.map((req) => (
              <div
                key={req.id}
                className="p-3 rounded-lg bg-[#141414] border border-amber-800/40 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
              >
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold text-neutral-200 font-mono">
                      {req.tool_name}
                    </span>
                    <span className="text-[10px] px-2 py-0.5 rounded font-medium bg-amber-900/50 text-amber-300 border border-amber-800">
                      {req.risk_level}
                    </span>
                  </div>
                  <p className="text-xs text-neutral-300 mt-1">{req.summary}</p>
                  <p className="text-[11px] text-neutral-400">{req.reason}</p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <button
                    onClick={async () => {
                      try {
                        await invoke("resolve_approval", {
                          requestId: req.id,
                          approved: true,
                          note: "Approved from Today dashboard",
                        });
                        setActionNotice(`Approved action for '${req.tool_name}'.`);
                        await loadDashboard();
                      } catch (e: any) {
                        setActionNotice(`Failed to approve: ${e?.toString() || "Unknown error"}`);
                      }
                    }}
                    className="px-3 py-1.5 bg-neutral-100 hover:bg-white text-neutral-950 font-semibold rounded-lg text-xs transition shadow-sm"
                  >
                    Approve
                  </button>
                  <button
                    onClick={async () => {
                      try {
                        await invoke("resolve_approval", {
                          requestId: req.id,
                          approved: false,
                          note: "Rejected from Today dashboard",
                        });
                        setActionNotice(`Rejected action for '${req.tool_name}'.`);
                        await loadDashboard();
                      } catch (e: any) {
                        setActionNotice(`Failed to reject: ${e?.toString() || "Unknown error"}`);
                      }
                    }}
                    className="px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 text-rose-300 border border-neutral-800 rounded-lg text-xs transition"
                  >
                    Reject
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Suggested Quick Everyday Actions */}
      <div className="space-y-2">
        <h2 className="text-xs font-semibold text-neutral-300 uppercase tracking-wider">
          What would you like to accomplish today?
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-2.5">
          <button
            onClick={() => onAskPrompt("Find the invoice I downloaded last month")}
            className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
          >
            <div className="flex items-center gap-2 mb-1.5">
              <FileText className="w-4 h-4 text-sky-400" />
              <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                Find Downloaded Invoices
              </span>
            </div>
            <p className="text-[11px] text-neutral-400 leading-snug">
              "Find the invoice I downloaded last month and extract the total."
            </p>
          </button>

          <button
            onClick={() => onAskPrompt("Organize my college documents into appropriate folders")}
            className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
          >
            <div className="flex items-center gap-2 mb-1.5">
              <FolderOpen className="w-4 h-4 text-amber-400" />
              <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                Organize Downloaded Files
              </span>
            </div>
            <p className="text-[11px] text-neutral-400 leading-snug">
              "Organize my downloads into category folders with safe preview."
            </p>
          </button>

          <button
            onClick={() => onAskPrompt("Explain this PDF in simple language and create revision notes")}
            className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
          >
            <div className="flex items-center gap-2 mb-1.5">
              <Sparkles className="w-4 h-4 text-purple-400" />
              <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                Study Notes & Summary
              </span>
            </div>
            <p className="text-[11px] text-neutral-400 leading-snug">
              "Explain this PDF in simple language and generate study flashcards."
            </p>
          </button>
        </div>
      </div>

      {/* Main 2-Column Dashboard Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
        {/* Card 1: Schedule & Google Calendar */}
        <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-neutral-800/80">
            <div className="flex items-center gap-2">
              <div className="p-1.5 rounded-lg bg-sky-950/40 text-sky-400 border border-sky-800/40">
                <Calendar className="w-4 h-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Upcoming Schedule</h3>
                <p className="text-[11px] text-neutral-400">Google Calendar agenda</p>
              </div>
            </div>
            {data?.upcoming_events && data.upcoming_events.length > 0 && (
              <span className="text-[10px] bg-sky-950/60 text-sky-300 border border-sky-800/60 px-2 py-0.5 rounded-full font-medium">
                {data.upcoming_events.length} upcoming
              </span>
            )}
          </div>

          <div className="space-y-2 min-h-[140px] flex flex-col justify-center">
            {data?.upcoming_events && data.upcoming_events.length > 0 ? (
              <div className="space-y-2">
                {data.upcoming_events.map((ev) => (
                  <div
                    key={ev.id}
                    className="p-2.5 rounded-lg bg-[#181818] border border-neutral-800 flex items-start justify-between"
                  >
                    <div>
                      <h4 className="text-xs font-semibold text-neutral-200">{ev.title}</h4>
                      <div className="flex items-center gap-2 text-[11px] text-neutral-400 mt-0.5">
                        <Clock className="w-3 h-3 text-neutral-400" />
                        <span>{ev.time_display}</span>
                        {ev.location && <span>• {ev.location}</span>}
                      </div>
                    </div>
                    {ev.html_link && (
                      <a
                        href={ev.html_link}
                        target="_blank"
                        rel="noreferrer"
                        className="text-neutral-400 hover:text-sky-300 transition p-1"
                        title="View event in Google Calendar"
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                      </a>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center py-6 px-4">
                <Calendar className="w-8 h-8 text-neutral-400 mx-auto mb-2" />
                <p className="text-xs font-medium text-neutral-300">No calendar events found</p>
                <p className="text-[11px] text-neutral-400 mt-1 max-w-xs mx-auto">
                  {data?.email_summary?.is_connected
                    ? "Your schedule is currently clear for the next 48 hours."
                    : "Connect your Google Calendar to view upcoming events and propose meeting times."}
                </p>
                {!data?.email_summary?.is_connected && (
                  <button
                    onClick={onOpenConnections}
                    className="mt-3 px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-medium text-neutral-200 inline-flex items-center gap-1.5 transition"
                  >
                    <Plug className="w-3.5 h-3.5 text-sky-400" />
                    Connect Google Calendar
                  </button>
                )}
              </div>
            )}
          </div>

          <div className="pt-2 border-t border-neutral-800/80 flex items-center justify-between text-[11px]">
            <span className="text-neutral-400">Schedule assistant</span>
            <button
              onClick={() => onAskPrompt("What is on my calendar tomorrow and help me organize it?")}
              className="text-sky-400 hover:text-sky-300 flex items-center gap-1"
            >
              Analyze Schedule <ArrowRight className="w-3 h-3" />
            </button>
          </div>
        </div>

        {/* Card 2: Email & Follow-ups */}
        <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-neutral-800/80">
            <div className="flex items-center gap-2">
              <div className="p-1.5 rounded-lg bg-sky-950/40 text-sky-400 border border-sky-800/40">
                <Mail className="w-4 h-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Email & Inbox</h3>
                <p className="text-[11px] text-neutral-400">Gmail summaries & action items</p>
              </div>
            </div>
            {data?.email_summary?.is_connected && (
              <span className="text-[10px] bg-neutral-800 text-neutral-200 border border-neutral-700 px-2 py-0.5 rounded-full font-medium">
                {data.email_summary.unread_count} unread
              </span>
            )}
          </div>

          <div className="space-y-2 min-h-[140px] flex flex-col justify-center">
            {data?.email_summary?.is_connected ? (
              <div className="space-y-2">
                <div className="p-3 rounded-lg bg-[#181818] border border-neutral-800">
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs font-semibold text-neutral-200">Connected to Gmail</span>
                    <span className="text-[10px] text-neutral-400 font-mono">
                      {data.email_summary.account_email || "Active"}
                    </span>
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    {data.email_summary.unread_count > 0
                      ? `You have ${data.email_summary.unread_count} unread messages in your inbox waiting for review.`
                      : "Your inbox is completely caught up! No unread messages."}
                  </p>
                </div>

                {data.email_summary.recent_subjects && data.email_summary.recent_subjects.length > 0 && (
                  <div className="space-y-1.5 pt-1">
                    <span className="text-[11px] font-semibold text-neutral-300 block">Recent Unread:</span>
                    {data.email_summary.recent_subjects.map((sub, sIdx) => (
                      <div
                        key={sIdx}
                        className="p-2 rounded-lg bg-[#181818] border border-neutral-800 text-xs text-neutral-200 flex items-center gap-1.5"
                      >
                        <Mail className="w-3.5 h-3.5 text-sky-400 shrink-0" />
                        <span className="truncate">{sub}</span>
                      </div>
                    ))}
                  </div>
                )}

                <button
                  onClick={() => onAskPrompt("Summarize my unread emails and draft replies")}
                  className="w-full py-2 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-medium text-neutral-200 flex items-center justify-center gap-1.5 transition"
                >
                  <Sparkles className="w-3.5 h-3.5" />
                  Summarize Unread & Draft Replies
                </button>
              </div>
            ) : (
              <div className="text-center py-6 px-4">
                <Mail className="w-8 h-8 text-neutral-400 mx-auto mb-2" />
                <p className="text-xs font-medium text-neutral-300">No email account connected</p>
                <p className="text-[11px] text-neutral-400 mt-1 max-w-xs mx-auto">
                  Connect your Google Workspace account to summarize important threads and safely prepare drafts.
                </p>
                <button
                  onClick={onOpenConnections}
                  className="mt-3 px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-medium text-neutral-200 inline-flex items-center gap-1.5 transition"
                >
                  <Plug className="w-3.5 h-3.5 text-neutral-300" />
                  Connect Gmail
                </button>
              </div>
            )}
          </div>

          <div className="pt-2 border-t border-neutral-800/80 flex items-center justify-between text-[11px]">
            <span className="text-neutral-400">Safe email drafting</span>
            <span className="text-neutral-400 italic">Explicit approval before sending</span>
          </div>
        </div>

        {/* Card 3: Recent Personal Documents */}
        <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-neutral-800/80">
            <div className="flex items-center gap-2">
              <div className="p-1.5 rounded-lg bg-amber-950/40 text-amber-400 border border-amber-800/40">
                <FileText className="w-4 h-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Recent Personal Documents</h3>
                <p className="text-[11px] text-neutral-400">Authorized local folders</p>
              </div>
            </div>
            <button
              onClick={() => onOpenTool("files")}
              className="text-xs text-amber-400 hover:text-amber-300 transition"
            >
              Browse All
            </button>
          </div>

          <div className="space-y-2 min-h-[140px] flex flex-col justify-center">
            {data?.recent_documents && data.recent_documents.length > 0 ? (
              <div className="space-y-2">
                {data.recent_documents.map((doc, idx) => (
                  <div
                    key={idx}
                    className="p-2.5 rounded-lg bg-[#181818] border border-neutral-800 flex items-center justify-between group hover:border-neutral-700 transition"
                  >
                    <div className="min-w-0 pr-2">
                      <h4 className="text-xs font-medium text-neutral-200 truncate">{doc.file_name}</h4>
                      <p className="text-[10px] text-neutral-400 truncate">{doc.path}</p>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      <span className="text-[10px] text-neutral-400">{doc.modified_ago}</span>
                      <button
                        onClick={() => onAskPrompt(`Summarize this document: ${doc.path}`)}
                        className="px-2 py-1 bg-neutral-800 hover:bg-neutral-700 text-[10px] font-medium text-neutral-200 rounded transition"
                      >
                        Summarize
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center py-6 px-4">
                <FolderOpen className="w-8 h-8 text-neutral-400 mx-auto mb-2" />
                <p className="text-xs font-medium text-neutral-300">No authorized folders selected</p>
                <p className="text-[11px] text-neutral-400 mt-1 max-w-xs mx-auto">
                  Select the folders (like Downloads or Documents) that Kyro is allowed to search and summarize.
                </p>
                <button
                  onClick={() => onOpenTool("files", "authorized")}
                  className="mt-3 px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-medium text-neutral-200 inline-flex items-center gap-1.5 transition"
                >
                  <FolderOpen className="w-3.5 h-3.5 text-amber-400" />
                  Authorize Folders
                </button>
              </div>
            )}
          </div>

          <div className="pt-2 border-t border-neutral-800/80 flex items-center justify-between text-[11px]">
            <span className="text-neutral-400">Local-first privacy</span>
            <button
              onClick={() => onOpenTool("files")}
              className="text-amber-400 hover:text-amber-300 flex items-center gap-1"
            >
              Organize & Duplicates <ArrowRight className="w-3 h-3" />
            </button>
          </div>
        </div>

        {/* Card 4: Automations & Routine Results */}
        <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-neutral-800/80">
            <div className="flex items-center gap-2">
              <div className="p-1.5 rounded-lg bg-purple-950/40 text-purple-400 border border-purple-800/40">
                <Clock className="w-4 h-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Personal Routines</h3>
                <p className="text-[11px] text-neutral-400">Scheduled tasks & run history</p>
              </div>
            </div>
            <button
              onClick={() => onOpenTool("routines")}
              className="text-xs text-purple-400 hover:text-purple-300 transition"
            >
              Manage
            </button>
          </div>

          <div className="space-y-2 min-h-[140px] flex flex-col justify-center">
            {data?.upcoming_routines && data.upcoming_routines.length > 0 ? (
              <div className="space-y-2">
                {data.upcoming_routines.slice(0, 3).map((rt) => (
                  <div
                    key={rt.id}
                    className="p-2.5 rounded-lg bg-[#181818] border border-neutral-800 flex items-center justify-between"
                  >
                    <div>
                      <h4 className="text-xs font-semibold text-neutral-200">{rt.name}</h4>
                      <div className="flex items-center gap-2 text-[10px] text-neutral-400 mt-0.5">
                        <span>Schedule: {rt.schedule}</span>
                        {rt.next_run_display && <span>• Next: {rt.next_run_display}</span>}
                      </div>
                    </div>
                    <button
                      onClick={() => handleRunRoutineNow(rt.id, rt.name)}
                      disabled={runningRoutineId === rt.id}
                      className="px-2.5 py-1 bg-purple-950/50 hover:bg-purple-900/50 border border-purple-800/60 text-purple-200 text-xs rounded font-medium flex items-center gap-1 transition"
                    >
                      <Play className="w-3 h-3 text-purple-300" />
                      {runningRoutineId === rt.id ? "Running..." : "Run"}
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center py-6 px-4">
                <Clock className="w-8 h-8 text-neutral-400 mx-auto mb-2" />
                <p className="text-xs font-medium text-neutral-300">No scheduled routines active</p>
                <p className="text-[11px] text-neutral-400 mt-1 max-w-xs mx-auto">
                  Create recurring automations like "Every Friday, summarize new receipts in Downloads".
                </p>
                <button
                  onClick={() => onOpenTool("routines")}
                  className="mt-3 px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-medium text-neutral-200 inline-flex items-center gap-1.5 transition"
                >
                  <Sparkles className="w-3.5 h-3.5 text-purple-400" />
                  Create a Routine
                </button>
              </div>
            )}
          </div>

          <div className="pt-2 border-t border-neutral-800/80 flex items-center justify-between text-[11px]">
            <span className="text-neutral-400">
              {data?.recent_routine_results?.length
                ? `${data.recent_routine_results.length} recent executions recorded`
                : "Safe execution & retry ledger"}
            </span>
            <button
              onClick={() => onOpenTool("routines", "history")}
              className="text-purple-400 hover:text-purple-300 flex items-center gap-1"
            >
              Run History <ArrowRight className="w-3 h-3" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
