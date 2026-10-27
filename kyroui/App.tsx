import React, { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Conversation,
  Message,
  AgentStepEvent,
  AgentReasoningEvent,
  AgentProfileRecord,
  TakeoverStatus,
  ApprovalRequest,
  GroundedScreenResult,
} from "./types/agent";
import {
  TaskProgress,
  evaluateVerificationOutcome,
  isInspectionToolName,
  isVerificationToolName,
  TaskPlan,
  TaskStep,
} from "./components/TaskProgress";
import { TerminalView } from "./components/TerminalView";
import { TaskComposer } from "./components/TaskComposer";
import { AgentActivityFeed } from "./components/AgentActivityFeed";
import { SettingsModal } from "./components/SettingsModal";
import { TeachTaskModal } from "./components/TeachTaskModal";
import { TeammateRosterModal } from "./components/TeammateRosterModal";
import { McpModal } from "./components/McpModal";
import { ConnectionsModal } from "./components/ConnectionsModal";
import { TimelineDrawer } from "./components/TimelineDrawer";
import { ArtifactsModal } from "./components/ArtifactsModal";
import { FileOrganizerModal } from "./components/FileOrganizerModal";
import { DocumentAssistantModal } from "./components/DocumentAssistantModal";
import { ScreenExplainerModal } from "./components/ScreenExplainerModal";
import { DevWorkbenchModal } from "./components/DevWorkbenchModal";
import { TodayDashboardView } from "./components/TodayDashboardView";
import { MyToolsView } from "./components/MyToolsView";
import { MyRoutinesView } from "./components/MyRoutinesView";
import { OnboardingModal } from "./components/OnboardingModal";
import { CommandPalette } from "./components/CommandPalette";
import { InlineNotice } from "./components/InlineNotice";
import { classifyTaskIntent, suggestedResourcesForIntent } from "./lib/taskIntent";
import {
  classifyAttachment,
  validateStrictUtf8,
  detectBinaryMagicBytes,
  ACCEPTED_ATTACHMENT_TYPES,
} from "./lib/attachments";
import { KyroLogo } from "./components/Logo";
import { AgentAvatar } from "./components/AgentAvatar";
import { NvidiaIcon } from "./components/BrandIcons";
import {
  Plus,
  Settings,
  Terminal,
  GitCommit,
  Eye,
  Camera,
  Crop,
  ChevronRight,
  ExternalLink,
  Users,
  Workflow,
  Server,
  AlertTriangle,
  Play,
  Ghost,
  Target,
  History,
  ShieldAlert,
  CheckCircle,
  XCircle,
  Layers,
  Command,
  X,
  Plug,
  Trash2,
  FolderTree,
  FileText,
  Sparkles,
  Code2,
  Stethoscope,
  MessageSquare,
  Calendar,
  Wrench,
  Clock,
  HelpCircle,
  Cpu,
} from "lucide-react";

export const getModelDisplayInfo = (modelId?: string | null) => {
  const mid = typeof modelId === "string" && modelId.trim() ? modelId : "nvidia/nemotron-3-ultra-550b-a55b";
  if (mid.includes("nemotron-3-ultra")) return { label: "Nemotron-3 Ultra", badge: "550B", isNvidia: true };
  if (mid.includes("llama-3.3-70b")) return { label: "Llama 3.3 Instruct", badge: "70B", isNvidia: false };
  if (mid.includes("llama-3.2-90b")) return { label: "Llama 3.2 Vision", badge: "90B", isNvidia: false };
  if (mid.includes("claude-3-5-sonnet")) return { label: "Claude 3.5 Sonnet", badge: "Anthropic", isNvidia: false };
  if (mid.includes("gpt-4o-mini")) return { label: "GPT-4o Mini", badge: "OpenAI", isNvidia: false };
  if (mid.includes("gpt-4o")) return { label: "GPT-4o", badge: "OpenAI", isNvidia: false };
  if (mid.includes("gemini-1.5-pro")) return { label: "Gemini 1.5 Pro", badge: "Google", isNvidia: false };
  if (mid.includes("gemini-1.5-flash")) return { label: "Gemini 1.5 Flash", badge: "Google", isNvidia: false };
  if (mid.includes("deepseek-coder")) return { label: "DeepSeek Coder", badge: "33B", isNvidia: false };
  if (mid.includes("mistral-large")) return { label: "Mistral Large", badge: "Mistral", isNvidia: false };
  const parts = mid.split("/");
  const name = parts[1] || parts[0];
  return { label: name.slice(0, 20), badge: parts[0]?.slice(0, 8) || "LLM", isNvidia: mid.startsWith("nvidia/") };
};

export function redactSensitiveFields(val: any): any {
  if (val === null || val === undefined) return val;
  if (typeof val === "string") return val;
  if (Array.isArray(val)) return val.map(redactSensitiveFields);
  if (typeof val === "object") {
    const res: Record<string, any> = {};
    const SENSITIVE_KEY_REGEX = /pass(word)?|token|secret|api[_-]?key|auth|bearer|credential/i;
    for (const [k, v] of Object.entries(val)) {
      if (SENSITIVE_KEY_REGEX.test(k) && typeof v === "string") {
        res[k] = "[REDACTED]";
      } else {
        res[k] = redactSensitiveFields(v);
      }
    }
    return res;
  }
  return val;
}

export function parseApprovalArgs(argsJson: string): {
  parsed: Record<string, any> | null;
  targetDetail: string | null;
  formattedRedacted: string;
} {
  try {
    const obj = JSON.parse(argsJson);
    if (obj && typeof obj === "object") {
      let target: string | null = null;
      if (obj.command) target = `Command: ${obj.command}`;
      else if (obj.path) target = `File: ${obj.path}`;
      else if (obj.url) target = `URL: ${obj.url}`;
      else if (obj.action) target = `Action: ${obj.action}${obj.x !== undefined ? ` at (${obj.x}, ${obj.y})` : ""}`;
      else if (obj.keys) target = `Keys: ${Array.isArray(obj.keys) ? obj.keys.join("+") : obj.keys}`;

      const redacted = redactSensitiveFields(obj);
      return {
        parsed: obj,
        targetDetail: target,
        formattedRedacted: JSON.stringify(redacted, null, 2),
      };
    }
  } catch {
    // not valid JSON
  }
  return {
    parsed: null,
    targetDetail: null,
    formattedRedacted: argsJson,
  };
}

export const App: React.FC = () => {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeConvId, setActiveConvId] = useState<string | null>(null);
  const activeConvIdRef = useRef<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [prompt, setPrompt] = useState("");
  const [selectedModel, setSelectedModel] = useState("nvidia/nemotron-3-ultra-550b-a55b");

  // Session Deletion State
  const [sessionToDelete, setSessionToDelete] = useState<Conversation | null>(null);
  const [isDeletingSession, setIsDeletingSession] = useState(false);
  const [sessionDeleteError, setSessionDeleteError] = useState<string | null>(null);

  // Experience Mode
  const [experienceMode, setExperienceMode] = useState<"everyday" | "dev">(() => {
    return (localStorage.getItem("kyro_experience_mode") as "everyday" | "dev") || "everyday";
  });

  // Everyday Destinations: "ask" | "today" | "tools" | "routines"
  const [everydayTab, setEverydayTab] = useState<"ask" | "today" | "tools" | "routines">("ask");
  const [toolsInitialTab, setToolsInitialTab] = useState<"files" | "study" | "screen" | "web" | "connected">("files");
  const [routinesInitialTab, setRoutinesInitialTab] = useState<"routines" | "history">("routines");
  const [isOnboardingOpen, setIsOnboardingOpen] = useState(false);

  // Structured Task Plan before consequential operations.
  // Single source of truth: the shape lives in TaskProgress.tsx and the same
  // component renders it (Bug 18). A plan without a backend origin is a
  // *suggested workflow*, never an authoritative execution plan (Bug 5).
  const [activePlan, setActivePlan] = useState<TaskPlan | null>(null);
  /** Synchronously readable mirror of activePlan for outcome wording. */
  const activePlanRef = useRef<TaskPlan | null>(null);
  activePlanRef.current = activePlan;
  /** Persisted plans per conversation so conversation switches do not discard live plan state (Bugs 2 & 6). */
  const plansByConvRef = useRef<Map<string, TaskPlan>>(new Map());
  /** Monotonic selection generation token preventing out-of-order attachment races (Bug 19). */
  const attachmentSelectionGenRef = useRef<number>(0);

  const handleSelectModel = async (model: string) => {
    setSelectedModel(model);
    try {
      await invoke("save_selected_model", { model });
    } catch (err) {
      console.error("Failed to persist selected model to SQLite settings:", err);
    }
  };

  const handleSetExperienceMode = (mode: "everyday" | "dev") => {
    setExperienceMode(mode);
    try {
      localStorage.setItem("kyro_experience_mode", mode);
    } catch {
      // ignore
    }
  };

  // Everyday & Dev Modals
  const [isFileOrganizerOpen, setIsFileOrganizerOpen] = useState(false);
  const [isDocumentAssistantOpen, setIsDocumentAssistantOpen] = useState(false);
  const [isScreenExplainerOpen, setIsScreenExplainerOpen] = useState(false);
  const [isDevWorkbenchOpen, setIsDevWorkbenchOpen] = useState(false);
  const [devWorkbenchTab, setDevWorkbenchTab] = useState<
    "projects" | "checkpoints" | "doctor" | "audit" | "workspace"
  >("projects");

  // Modals & Drawers
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<string>("model");
  const [isTeachTaskOpen, setIsTeachTaskOpen] = useState(false);
  const [teachTaskInitialTab, setTeachTaskInitialTab] = useState<"skills" | "record" | "routines">("skills");
  const [teachTaskHighlightId, setTeachTaskHighlightId] = useState<string | undefined>(undefined);
  const [isTeammatesOpen, setIsTeammatesOpen] = useState(false);
  const [isMcpOpen, setIsMcpOpen] = useState(false);
  const [isConnectionsOpen, setIsConnectionsOpen] = useState(false);
  const [isTimelineOpen, setIsTimelineOpen] = useState(false);
  const [isArtifactsOpen, setIsArtifactsOpen] = useState(false);
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
  const [showTerminal, setShowTerminal] = useState(false);
  const [isGhostMode, setIsGhostMode] = useState(false);

  // Agent State & Governance
  const [isAgentRunning, setIsAgentRunning] = useState(false);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const activeRunIdRef = useRef<string | null>(null);
  useEffect(() => {
    activeRunIdRef.current = activeRunId;
  }, [activeRunId]);

  /**
   * The runs this window submitted and have not yet resolved, keyed by run id
   * (value: owning conversation). Background runs stay alive here when the
   * user switches conversation, so they keep their identity and remain
   * stoppable explicitly (Bug 1/2).
   */
  const inFlightRunsRef = useRef<Map<string, string>>(new Map());
  /** Per-conversation synchronous submission lock (Bug 4). */
  const submitLocksRef = useRef<Map<string, string>>(new Map());
  /** Mirror of the newest unresolved run per conversation + current busy/selection. */
  const syncAgentBusy = () => {
    const currentConv = activeConvIdRef.current;
    let newestRunForConv: string | null = null;
    let anyForConv = false;
    for (const [runId, convId] of inFlightRunsRef.current.entries()) {
      if (convId === currentConv) {
        anyForConv = true;
        newestRunForConv = runId; // entries() iterates insertion order; last wins
      }
    }
    setActiveRunId(newestRunForConv);
    setIsAgentRunning(anyForConv);
  };

  const [activeReasoning, setActiveReasoning] = useState<{
    runId: string;
    conversationId?: string | null;
    agentId?: string | null;
    parentRunId?: string | null;
    isChildRun?: boolean;
    step: number;
    text: string;
    isStreaming: boolean;
  } | null>(null);
  const [allProfiles, setAllProfiles] = useState<AgentProfileRecord[]>([]);
  const [pendingApprovals, setPendingApprovals] = useState<ApprovalRequest[]>([]);
  const [expandedApprovalIds, setExpandedApprovalIds] = useState<Record<string, boolean>>({});
  const [showRawArgs, setShowRawArgs] = useState<Record<string, boolean>>({});
  /**
   * Live step events, grouped by the run that produced them.
   *
   * Grouping by run keeps simultaneous runs apart, and it means the events of
   * a background run are preserved when the user switches conversation
   * instead of being lost.
   */
  const [liveStepsByRun, setLiveStepsByRun] = useState<Record<string, AgentStepEvent[]>>({});
  const [screenPreview, setScreenPreview] = useState<string | null>(null);
  /** Non-blocking feedback for operations that failed outside of a modal. */
  const [notice, setNotice] = useState<{ tone: "error" | "success" | "info"; message: string } | null>(
    null
  );
  const [groundedResult, setGroundedResult] = useState<GroundedScreenResult | null>(null);

  // Voice Dictation & Text-to-Speech
  const [isListening, setIsListening] = useState(false);
  const [speakingMessageId, setSpeakingMessageId] = useState<string | null>(null);
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null);
  const recognitionRef = useRef<any>(null);

  // File Attachment
  const [attachedFile, setAttachedFile] = useState<{
    name: string;
    content: string;
    base64?: string;
    mimeType?: string;
  } | null>(null);

  // Multi-Bot & Governance. The active teammate is an agent profile resolved
  // by the registry — client-supplied prompts are never trusted.
  const DEFAULT_PROFILE: AgentProfileRecord = {
    id: "apex-orchestrator",
    name: "Apex",
    avatar: "zap",
    role: "General Workspace & Strategic Orchestrator",
    description: "",
    model_id: null,
    origin: "builtin",
    enabled: true,
    runtime_disabled: false,
    schema_version: 1,
    version_label: "",
    aliases: [],
    validation_error: null,
    source_path: null,
    instructions: "",
    tools_mode: "inherit",
    tools_allow: [],
    delegation_enabled: true,
    delegation_max_depth: 3,
    delegation_max_children: 4,
    max_steps: 25,
    is_builtin: true,
  };
  const [activeBot, setActiveBot] = useState<AgentProfileRecord>(DEFAULT_PROFILE);

  const [takeover, setTakeover] = useState<TakeoverStatus>({
    is_active: false,
    reason: "",
    timestamp: 0,
  });

  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    activeConvIdRef.current = activeConvId;
  }, [activeConvId]);

  useEffect(() => {
    loadConversations();
    loadDefaultBot();

    invoke<string>("get_selected_model")
      .then((saved) => {
        if (typeof saved === "string" && saved.trim()) setSelectedModel(saved);
      })
      .catch((err) => console.warn("Failed to load persisted model from SQLite settings:", err));

    try {
      const onboardingDone = localStorage.getItem("kyro_onboarding_completed");
      if (!onboardingDone) {
        setIsOnboardingOpen(true);
      }
    } catch {
      // ignore
    }

    // Request notification permission if supported
    if ("Notification" in window && Notification.permission === "default") {
      Notification.requestPermission().catch(() => {});
    }

    // Initialize Web Speech API for voice dictation (speech transcription is handled by the platform/webview provider)
    const SpeechRecognition =
      (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (SpeechRecognition) {
      try {
        const recognition = new SpeechRecognition();
        recognition.continuous = false;
        recognition.interimResults = false;
        recognition.onresult = (event: any) => {
          const transcript = event.results[0][0].transcript;
          if (transcript) {
            setPrompt((prev) => (prev ? prev + " " + transcript : transcript));
          }
          setIsListening(false);
        };
        recognition.onerror = () => setIsListening(false);
        recognition.onend = () => setIsListening(false);
        recognitionRef.current = recognition;
      } catch (e) {
        console.warn("Speech recognition init error:", e);
      }
    }

    // Keyboard shortcut for command palette (Ctrl+K or Cmd+K)
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setIsCommandPaletteOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleGlobalKeyDown);

    // Listen to real-time agent turn step updates from Rust engine.
    //
    // Every event carries the identity of the run and the conversation it
    // belongs to, so that events from concurrent runs are routed to the right
    // timeline and events for another conversation never update this one.
    const unlistenPromise = listen<AgentStepEvent>("agent:step", (event) => {
      const payload = event.payload;
      if (!payload || !payload.run_id) {
        // An event without a run identity cannot be attributed safely.
        return;
      }

      setLiveStepsByRun((prev) => {
        const runSteps = prev[payload.run_id] ?? [];
        const withoutSameStep = runSteps.filter((s) => s.step !== payload.step);
        return {
          ...prev,
          [payload.run_id]: [...withoutSameStep, payload].sort((a, b) => a.step - b.step),
        };
      });

      // Update active plan evidence based on verifiable tool outcomes.
      // The plan of one run can never be mutated by the events of another
      // (run-scoped ownership), a recorded failure is never overwritten by a
      // later success, and verification is evaluated through the truthful
      // helper with the actual shell command the run executed (Bug 6).
      setActivePlan((prev) => {
        if (!prev || prev.runId !== payload.run_id) return prev;
        const calls = payload.tool_calls || [];
        const findCallCommand = (res: { tool_call_id: string; name: string }) => {
          const call =
            calls.find((tc: any) => tc.id === res.tool_call_id) ||
            calls.find((tc: any) => tc.name === res.name);
          const command = call?.arguments?.command;
          return typeof command === "string" ? command : undefined;
        };

        const newAccomplished = [...prev.accomplished];
        const newRemaining = [...prev.remaining];
        let inspectionRan = false;
        let inspectionFailed = false;
        let actionRan = false;
        let actionFailed = false;
        let actionUncertain = false;
        let verificationPassed = false;
        let verificationFailed = false;
        let verificationUncertain = false;
        let verificationEvidence: string | null = null;

        // Decisive failure markers in an action's stdout/stderr; coarse words
        // alone are not evidence, but a process exit code of 1 is.
        const ACTION_FAILURE_PATTERN =
          /exit code: 1|exit status: 1|compilation error|build failed|tests? failed|assertion error|error\[/i;

        for (const res of payload.tool_results || []) {
          const commandArg = findCallCommand(res);
          const isVerification = isVerificationToolName(res.name, commandArg);
          const isInspection = !isVerification && isInspectionToolName(res.name);

          const isToolUncertain = (() => {
            if (!res.output || typeof res.output !== "string") return false;
            try {
              const parsed = JSON.parse(res.output);
              if (parsed.status === "uncertain" || parsed.verification?.status === "uncertain") {
                return true;
              }
            } catch {
              if (res.output.includes('"status": "uncertain"') || res.output.includes('"status":"uncertain"')) {
                return true;
              }
            }
            return false;
          })();

          if (isVerification) {
            // Executing a verification tool IS an accomplished, factual
            // action — recorded even when the verification itself fails
            // (the failure lands in the step/remaining evidence).
            const accSummary = `Executed ${res.name}`;
            if (!newAccomplished.includes(accSummary)) {
              newAccomplished.push(accSummary);
            }
            const outcome = evaluateVerificationOutcome(res, commandArg);
            if (res.is_error || outcome.failed) {
              verificationFailed = true;
              verificationEvidence = outcome.reason;
            } else if (outcome.uncertain || isToolUncertain) {
              verificationUncertain = true;
              verificationEvidence = outcome.reason || "Verification outcome inconclusive (uncertain)";
            } else if (outcome.verified) {
              verificationPassed = true;
              verificationEvidence = outcome.reason;
            }
            continue;
          }

          if (isInspection) {
            if (res.is_error) {
              inspectionFailed = true;
            } else {
              inspectionRan = true;
            }
            continue;
          }

          if (res.is_error || ACTION_FAILURE_PATTERN.test(res.output || "")) {
            actionFailed = true;
            const errSummary = res.is_error
              ? `Error in ${res.name}: ${res.output.slice(0, 80)}`
              : `${res.name} reported failure: ${res.output.slice(0, 80)}`;
            if (!newRemaining.includes(errSummary)) {
              newRemaining.push(errSummary);
            }
            continue;
          }

          if (isToolUncertain) {
            actionUncertain = true;
            const uncSummary = `${res.name} completed with uncertain postcondition`;
            if (!newRemaining.includes(uncSummary)) {
              newRemaining.push(uncSummary);
            }
            continue;
          }

          actionRan = true;
          const accSummary = `Executed ${res.name}`;
          if (!newAccomplished.includes(accSummary)) {
            newAccomplished.push(accSummary);
          }
        }

        // Immutable evidence policy: a step that already reached completed or
        // failed stays there; only evidence advances it otherwise.
        const updatedSteps = prev.steps.map(
          (st: TaskStep): TaskStep => {
            if (st.id === "step_inspect") {
              if (st.status === "completed" || st.status === "failed") return st;
              if (inspectionFailed) {
                return {
                  ...st,
                  status: "failed" as const,
                  done: false,
                  evidence: "Inspection tool reported an error",
                };
              }
              if (inspectionRan) {
                return {
                  ...st,
                  status: "completed" as const,
                  done: true,
                  evidence: "Inspection tool ran successfully",
                };
              }
              return st;
            }
            if (st.id === "step_execute") {
              if (st.status === "failed" || st.status === "completed") return st;
              if (actionFailed) {
                return {
                  ...st,
                  status: "failed" as const,
                  done: false,
                  evidence: "Execution failure observed",
                };
              }
              if (actionUncertain) {
                return {
                  ...st,
                  status: "uncertain" as const,
                  done: false,
                  evidence: "Action completed but outcome is uncertain (inconclusive postcondition)",
                };
              }
              if (actionRan) {
                return {
                  ...st,
                  status: "completed" as const,
                  done: true,
                  evidence: "Actions executed",
                };
              }
              return st;
            }
            if (st.id === "step_verify") {
              if (st.status === "completed" || st.status === "failed") return st;
              if (verificationFailed) {
                return {
                  ...st,
                  status: "failed" as const,
                  done: false,
                  evidence: verificationEvidence || "Verification did not pass",
                };
              }
              if (verificationUncertain) {
                return {
                  ...st,
                  status: "uncertain" as const,
                  done: false,
                  evidence: verificationEvidence || "Verification check inconclusive (uncertain)",
                };
              }
              if (verificationPassed) {
                return {
                  ...st,
                  status: "completed" as const,
                  done: true,
                  evidence: verificationEvidence || "Verification check passed",
                };
              }
              return st;
            }
            return st;
          }
        );

        return {
          ...prev,
          steps: updatedSteps,
          accomplished: newAccomplished,
          remaining: newRemaining.filter((r) => !newAccomplished.includes(r)),
        };
      });

      const currentConversation = activeConvIdRef.current;
      const belongsToActiveConversation =
        !!payload.conversation_id && !!currentConversation &&
        payload.conversation_id === currentConversation;

      if (belongsToActiveConversation) {
        // Step events may heal the visible run selection for runs this window
        // did not start (e.g. after a reload). A newer engine-driven timeline
        // replaces an older engine-driven one (same conversation), but a
        // submission-owned in-flight run is never stolen from (Bug 2).
        const currentActive = activeRunIdRef.current;
        const currentIsInFlight =
          !!currentActive && inFlightRunsRef.current.has(currentActive);
        if (
          !currentIsInFlight &&
          !inFlightRunsRef.current.has(payload.run_id)
        ) {
          setActiveRunId(payload.run_id);
        }
      }
      // Clear transient reasoning when step executes tools or completes (only for the owning run, Bug 2)
      if ((payload.tool_calls && payload.tool_calls.length > 0) || payload.is_finished) {
        setActiveReasoning((prev) => (prev && prev.runId === payload.run_id ? null : prev));
      }
    });

    // Listen to real-time agent reasoning deltas from provider streaming
    const unlistenReasoningPromise = listen<AgentReasoningEvent>("agent:reasoning", (event) => {
      const payload = event.payload;
      if (!payload || !payload.run_id) return;

      const currentConversation = activeConvIdRef.current;
      if (
        payload.conversation_id &&
        currentConversation &&
        payload.conversation_id !== currentConversation
      ) {
        return;
      }

      const currentRunId = activeRunIdRef.current;
      if (currentRunId) {
        const isMatchingRun =
          payload.run_id === currentRunId || payload.parent_run_id === currentRunId;
        if (!isMatchingRun) return;
      }

      const isTerminal = payload.state === "completed" || payload.state === "cancelled";

      setActiveReasoning((prev) => {
        const isSameContext = prev && prev.runId === payload.run_id && prev.step === payload.step;
        const newText = payload.accumulated
          ? payload.accumulated
          : isSameContext
          ? prev.text + payload.delta
          : payload.delta;

        return {
          runId: payload.run_id,
          conversationId: payload.conversation_id,
          agentId: payload.agent_id,
          parentRunId: payload.parent_run_id,
          isChildRun: payload.is_child_run,
          step: payload.step,
          text: newText,
          isStreaming: !isTerminal,
        };
      });
    });

    // Governance & Takeover polling
    const pollInterval = setInterval(async () => {
      try {
        const [tStatus, approvals] = await Promise.all([
          invoke<TakeoverStatus>("get_takeover_status"),
          invoke<ApprovalRequest[]>("list_pending_approvals"),
        ]);
        if (tStatus) setTakeover(tStatus);
        if (Array.isArray(approvals)) {
          setPendingApprovals(approvals.filter((a) => a.status === "pending"));
        }
      } catch (e) {
        // ignore
      }
    }, 1000);

    return () => {
      window.removeEventListener("keydown", handleGlobalKeyDown);
      unlistenPromise.then((fn) => fn());
      unlistenReasoningPromise.then((fn) => fn());
      clearInterval(pollInterval);
    };
  }, []);

  useEffect(() => {
    if (activeConvId) {
      loadMessages(activeConvId);
    } else {
      setMessages([]);
    }
    // Background runs of OTHER conversations are still in flight and keep
    // their run identity (Bug 1). This conversation displays its newest
    // unresolved run, or nothing when it has none.
    setActiveReasoning(null);
    syncAgentBusy();
    setActivePlan((prev) => (prev && prev.conversationId === activeConvId ? prev : (activeConvId ? plansByConvRef.current.get(activeConvId) || null : null)));
  }, [activeConvId]);

  useEffect(() => {
    if (!sessionToDelete) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isDeletingSession) {
        handleCancelDeleteSession();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [sessionToDelete, isDeletingSession]);

  /**
   * The live steps that belong to the conversation that is currently open.
   *
   * When a specific run is active its steps are shown; otherwise the most
   * recently updated run of the active conversation is used.  Runs belonging
   * to other conversations are never shown here.
   */
  const liveSteps = useMemo<AgentStepEvent[]>(() => {
    const byConversation = Object.values(liveStepsByRun).filter((steps) => {
      const first = steps[0];
      if (!first) return false;
      if (!first.conversation_id) return false;
      return first.conversation_id === activeConvId;
    });
    if (activeRunId && liveStepsByRun[activeRunId]) {
      const candidate = liveStepsByRun[activeRunId];
      const first = candidate[0];
      if (!first?.conversation_id || first.conversation_id === activeConvId) {
        return candidate;
      }
    }
    if (byConversation.length === 0) return [];
    return byConversation.reduce((latest, current) => {
      const latestStep = latest[0];
      const currentStep = current[0];
      if (!latestStep || !currentStep) return current;
      return currentStep.step >= latestStep.step ? current : latest;
    });
  }, [liveStepsByRun, activeRunId, activeConvId]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, liveSteps]);

  /**
   * Clears the live steps of a single run.
   *
   * The steps of other runs are left untouched, so clearing one run never
   * removes the timeline of a concurrent one.
   */
  const clearRunSteps = (runId: string | null) => {
    if (!runId) return;
    setLiveStepsByRun((prev) => {
      if (!(runId in prev)) return prev;
      const next = { ...prev };
      delete next[runId];
      return next;
    });
  };

  const sendNotification = (title: string, body: string) => {
    if ("Notification" in window && Notification.permission === "granted") {
      try {
        new Notification(title, { body });
      } catch (e) {
        console.warn("Desktop notification error:", e);
      }
    }
  };

  /**
   * Truthful notifications (Bug 6): each plan announces a terminal outcome
   * exactly once, derived from the committed plan state.
   */
  const notifiedPlanIdsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const plan = activePlan;
    if (!plan || notifiedPlanIdsRef.current.has(plan.id)) return;
    switch (plan.status) {
      case "completed":
        notifiedPlanIdsRef.current.add(plan.id);
        sendNotification("Kyro Task Completed", plan.goal.slice(0, 80));
        break;
      case "failed":
        notifiedPlanIdsRef.current.add(plan.id);
        sendNotification("Kyro Task Failed", "The run reported execution failures.");
        break;
      case "unverified":
        notifiedPlanIdsRef.current.add(plan.id);
        sendNotification(
          "Kyro Task Finished (unverified)",
          "The run ended without a successful verification check."
        );
        break;
      case "cancelled":
        notifiedPlanIdsRef.current.add(plan.id);
        sendNotification("Kyro Task Stopped", "The run was cancelled per your request.");
        break;
      default:
        break;
    }
  }, [activePlan?.id, activePlan?.status]);

  const loadDefaultBot = async () => {
    try {
      // The active profile persists in the settings store (selected_agent);
      // an unknown or disabled id resolves back to the bundled default.
      const [savedId, rawProfiles] = await Promise.all([
        invoke<string>("get_selected_agent"),
        invoke<AgentProfileRecord[]>("list_agent_profiles"),
      ]);
      const profiles = Array.isArray(rawProfiles) ? rawProfiles : [];
      setAllProfiles(profiles);
      const profile = profiles.find((p) => p.id === savedId && p.enabled && !p.validation_error);
      if (profile) {
        setActiveBot(profile);
      } else {
        const fallback = profiles.find(
          (p) => p.id === DEFAULT_PROFILE.id && p.enabled && !p.validation_error,
        );
        if (fallback) {
          setActiveBot(fallback);
          invoke("save_selected_agent", { agentId: fallback.id }).catch(console.error);
        }
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleSelectAgent = (profile: AgentProfileRecord) => {
    setActiveBot(profile);
    invoke("save_selected_agent", { agentId: profile.id }).catch(console.error);
  };

  const loadConversations = async () => {
    try {
      const list = await invoke<Conversation[]>("list_conversations");
      if (Array.isArray(list)) {
        setConversations(list);
        const currentId = activeConvIdRef.current;
        if (list.length === 0) {
          setActiveConvId(null);
          setMessages([]);
          setLiveStepsByRun({});
        } else if (!currentId || !list.some((c) => c.id === currentId)) {
          setActiveConvId(list[0].id);
        }
      }
    } catch (err) {
      console.error("Failed to load conversations:", err);
    }
  };

  const handlePromptDeleteSession = (conv: Conversation) => {
    setSessionToDelete(conv);
    setSessionDeleteError(null);
  };

  const handleCancelDeleteSession = () => {
    if (isDeletingSession) return;
    setSessionToDelete(null);
    setSessionDeleteError(null);
  };

  const handleConfirmDeleteSession = async () => {
    if (!sessionToDelete || isDeletingSession) return;

    setIsDeletingSession(true);
    setSessionDeleteError(null);

    const targetId = sessionToDelete.id;
    const wasActive = activeConvId === targetId;

    try {
      const success = await invoke<boolean>("delete_conversation", { id: targetId });
      if (!success) {
        setSessionDeleteError("Session not found or already deleted.");
        await loadConversations();
        return;
      }

      // Reconcile list from persistent storage
      const updatedList = await invoke<Conversation[]>("list_conversations");
      setConversations(updatedList);

      if (wasActive) {
        setActivePlan(null);
        if (updatedList.length > 0) {
          const nextId = updatedList[0].id;
          setActiveConvId(nextId);
          await loadMessages(nextId);
        } else {
          setActiveConvId(null);
          setMessages([]);
          setLiveStepsByRun({});
        }
      } else {
        if (activePlan && activePlan.conversationId === targetId) {
          setActivePlan(null);
        }
        plansByConvRef.current.delete(targetId);
        if (!updatedList.some((c) => c.id === activeConvIdRef.current)) {
          if (updatedList.length > 0) {
            const nextId = updatedList[0].id;
            setActiveConvId(nextId);
            await loadMessages(nextId);
          } else {
            setActiveConvId(null);
            setMessages([]);
            setLiveStepsByRun({});
          }
        }
      }

      setSessionToDelete(null);
    } catch (err: any) {
      const msg = typeof err === "string" ? err : err?.message || "Failed to delete session.";
      setSessionDeleteError(msg);
    } finally {
      setIsDeletingSession(false);
    }
  };

  const loadMessages = async (convId: string) => {
    try {
      const msgs = await invoke<Message[]>("get_messages", { conversationId: convId });
      if (activeConvIdRef.current === convId) {
        setMessages(msgs || []);
        // The live steps of the runs of this conversation are kept, so that a
        // background run is not interrupted by reloading the history.
      }
    } catch (err) {
      console.error("Failed to load messages:", err);
    }
  };

  const handleNewConversation = async () => {
    const id = "conv_" + crypto.randomUUID();
    const title = "New Task " + (conversations.length + 1);
    try {
      const newConv = await invoke<Conversation>("create_conversation", {
        id,
        title,
        model: selectedModel,
      });
      setConversations([newConv, ...conversations]);
      setActiveConvId(id);
      setMessages([]);
      setLiveStepsByRun({});
    } catch (err) {
      console.error("Failed to create conversation:", err);
    }
  };

  const handleOpenTeachTask = (
    tab: "skills" | "record" | "routines" = "skills",
    taskId?: string
  ) => {
    setTeachTaskInitialTab(tab);
    setTeachTaskHighlightId(taskId);
    setIsTeachTaskOpen(true);
  };

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
      await invoke("resolve_approval", { id, decision, resolvedBy: "desktop_user" });
      setPendingApprovals((prev) => prev.filter((a) => a.id !== id));
    } catch (err) {
      console.error("Failed to resolve approval:", err);
    }
  };

  const handleCancelTaskForApproval = async (approvalId: string, runId: string) => {
    try {
      await handleResolveApproval(approvalId, "denied");
      await invoke("cancel_agent_run", { runId }).catch(console.error);
      await invoke("abort_computer_use").catch(console.error);
      setActivePlan((prev) =>
        prev && prev.runId === runId
          ? {
              ...prev,
              status: "cancelled" as const,
              steps: prev.steps.map((s) =>
                s.status === "running" ? { ...s, status: "failed" as const, done: false, evidence: "Cancelled" } : s
              ),
              remaining: ["Task cancelled from approval banner"],
            }
          : prev
      );
      setNotice({ tone: "info", message: "Action denied and task was cancelled." });
    } catch (err) {
      console.error("Failed to cancel task from approval:", err);
    }
  };

  const toggleVoiceDictation = () => {
    if (!recognitionRef.current) {
      setNotice({
        tone: "error",
        message: "Speech recognition is not supported in this browser/webview engine.",
      });
      return;
    }
    if (isListening) {
      try {
        recognitionRef.current.stop();
      } catch (e) {}
      setIsListening(false);
    } else {
      try {
        recognitionRef.current.start();
        setIsListening(true);
      } catch (e) {
        console.warn("Speech recognition start failed:", e);
        setIsListening(false);
      }
    }
  };

  const handleSpeak = (msgId: string, text: string) => {
    if (!("speechSynthesis" in window)) return;
    if (speakingMessageId === msgId) {
      window.speechSynthesis.cancel();
      setSpeakingMessageId(null);
    } else {
      window.speechSynthesis.cancel();
      const utterance = new SpeechSynthesisUtterance(text);
      utterance.onend = () => setSpeakingMessageId(null);
      utterance.onerror = () => setSpeakingMessageId(null);
      window.speechSynthesis.speak(utterance);
      setSpeakingMessageId(msgId);
    }
  };

  const handleCopyMessage = async (msgId: string, text: string) => {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        setCopiedMessageId(msgId);
        setTimeout(() => setCopiedMessageId(null), 2000);
      }
    } catch (e) {
      console.warn("Copy message error:", e);
    }
  };

  /**
   * Attachment policy gate (Bug 19): explicit allowlist and size caps drive
   * what may be attached at all. Images travel on the binary/base64 path;
   * searchable PDFs are decoded through the backend's real document parser
   * (`extract_attachment_text`); supported text/code files are decoded as
   * UTF-8 only after a binary-content check. Everything else — including
   * mismatched MIME/extension pairs and empty files — is rejected with a
   * clear error before submission.
   */
  const handleFileSelect = async (file: File) => {
    if (!file) return;

    const selectionGen = ++attachmentSelectionGenRef.current;

    const classification = classifyAttachment(file);
    if (classification.error || !classification.kind) {
      setNotice({ tone: "error", message: classification.error || "Unsupported attachment." });
      return;
    }

    const readAsDataURL = () =>
      new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error || new Error("File read failed"));
        reader.readAsDataURL(file);
      });
    const readAsArrayBuffer = () =>
      new Promise<ArrayBuffer>((resolve, reject) => {
        if (typeof file.arrayBuffer === "function") {
          file.arrayBuffer().then(resolve, reject);
          return;
        }
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as ArrayBuffer);
        reader.onerror = () => reject(reader.error || new Error("File read failed"));
        (reader as any).readAsArrayBuffer(file);
      });

    try {
      if (classification.kind === "image") {
        const dataUrl = await readAsDataURL();
        if (attachmentSelectionGenRef.current !== selectionGen) return;
        setAttachedFile({
          name: file.name,
          content: `[Attached image file: ${file.name}]`,
          base64: dataUrl.split(",")[1],
          mimeType: file.type || "image/png",
        });
        return;
      }

      if (classification.kind === "pdf") {
        const dataUrl = await readAsDataURL();
        const base64 = dataUrl.split(",")[1];
        // Route through the production document parser — the raw PDF is
        // never decoded as text on the client.
        const extracted = await invoke<{
          text: string;
          format: string;
          page_count: number | null;
          file_name: string;
          word_count: number;
          character_count: number;
          truncated: boolean;
        }>("extract_attachment_text", { fileName: file.name, base64Bytes: base64 });
        if (attachmentSelectionGenRef.current !== selectionGen) return;
        const pageNote = extracted.page_count ? ` (extracted from ${extracted.page_count} PDF pages)` : "";
        setAttachedFile({
          name: file.name,
          content: `[Attached PDF document: ${file.name}${pageNote}]\n${extracted.text}`,
          mimeType: "application/pdf",
        });
        return;
      }

      // Text/code: read as ArrayBuffer, verify binary magic bytes and strict UTF-8
      const buffer = await readAsArrayBuffer();
      if (detectBinaryMagicBytes(buffer)) {
        setNotice({
          tone: "error",
          message: `"${file.name}" contains binary data and cannot be attached as text.`,
        });
        return;
      }

      let text: string;
      try {
        text = validateStrictUtf8(buffer);
      } catch (decodeErr: any) {
        setNotice({
          tone: "error",
          message: `"${file.name}" is not valid UTF-8 text and cannot be attached: ${decodeErr?.message || decodeErr}`,
        });
        return;
      }

      if (text.includes("\0")) {
        setNotice({
          tone: "error",
          message: `"${file.name}" contains binary data and cannot be attached as text.`,
        });
        return;
      }

      if (attachmentSelectionGenRef.current !== selectionGen) return;
      setAttachedFile({
        name: file.name,
        content: text,
        mimeType: file.type || "text/plain",
      });
    } catch (err: any) {
      if (attachmentSelectionGenRef.current !== selectionGen) return;
      const message = typeof err === "string" ? err : (err?.message ?? "Attachment could not be processed");
      setNotice({ tone: "error", message: `Attachment rejected: ${message}` });
    }
  };

  /**
   * Runs a turn through the backend.
   *
   * Concurrency truthfulness (Bugs 2 & 4):
   *  - a per-conversation synchronous lock is acquired BEFORE the first
   *    await, so a second submission for the same conversation can never be
   *    in flight; different conversations may still run concurrently;
   *  - state updates are functional and every response/error path verifies
   *    that the conversation it captured still owns the visible state;
   *  - only the submission that owns the lock releases it.
   */
  const handleSubmit = async (formEvent?: React.FormEvent) => {
    formEvent?.preventDefault?.();

    if (!prompt.trim() && !attachedFile) return;
    const lockKey = activeConvId ?? "_new_conversation";
    // Synchronous acquisition: ref mutation happens before any await.
    if (submitLocksRef.current.has(lockKey)) return;
    const submitId = "submit_" + crypto.randomUUID();
    submitLocksRef.current.set(lockKey, submitId);
    // The run this submission registers, so `finally` can always clean up
    // exactly what this submission created (and nothing else).
    let myRunId: string | null = null;
    let registeredConvId: string | null = null;

    const typedAsk = prompt;
    let convId = activeConvId;
    try {
      if (!convId) {
        convId = "conv_" + crypto.randomUUID();
        const newConv = await invoke<Conversation>("create_conversation", {
          id: convId,
          title: typedAsk.slice(0, 30) || attachedFile?.name || "New Task",
          model: selectedModel,
        });
        // Atomically (synchronously) move the submission lock to the real
        // conversation id before any other work.
        submitLocksRef.current.delete(lockKey);
        submitLocksRef.current.set(convId, submitId);
        setConversations((prev) => [newConv, ...prev.filter((c) => c.id !== convId)]);
        setActiveConvId(convId);
      }

      let fullPromptText = typedAsk;
      let imageAttachment: { base64: string; mimeType: string; name: string } | null = null;
      if (attachedFile) {
        if (attachedFile.base64) {
          imageAttachment = {
            base64: attachedFile.base64,
            mimeType: attachedFile.mimeType || "image/png",
            name: attachedFile.name,
          };
          fullPromptText = `[Attachment: ${attachedFile.name}]\n${typedAsk}`;
        } else {
          fullPromptText = `[Attachment: ${attachedFile.name}]\n\`\`\`\n${attachedFile.content}\n\`\`\`\n\n${typedAsk}`;
        }
        setAttachedFile(null);
      }

      const userMsg: Message = {
        id: "msg_" + crypto.randomUUID(),
        conversation_id: convId,
        role: "user",
        content: fullPromptText,
        created_at: Date.now(),
      };

      setMessages((prev) => [...prev, userMsg]);
      await invoke("add_message", { msg: userMsg }).catch(console.error);

      setPrompt("");
      const runId = "run_" + crypto.randomUUID();
      const conversationId = convId!;
      // This run is durably registered before the engine call so UI state,
      // events and cancellation all agree about who owns what (Bug 1/2).
      inFlightRunsRef.current.set(runId, conversationId);
      submitLocksRef.current.set(conversationId, submitId);
      myRunId = runId;
      registeredConvId = conversationId;
      clearRunSteps(runId);
      setActiveReasoning(null);
      syncAgentBusy();

      // Stale plans never survive a new task: cleared for prompts that do
      // not qualify as well (Bug 5).
      setActivePlan(null);
      const intent = classifyTaskIntent(typedAsk);
      if (intent.isConsequential) {
        const plan: TaskPlan = {
          id: "plan_" + Date.now(),
          runId,
          conversationId,
          // Client-side template: labeled a SUGGESTED workflow, never a
          // backend-execution plan (Bug 5).
          isBackendPlan: false,
          goal: typedAsk,
          understanding: `Suggested workflow for: "${typedAsk.trim()}". Kyro's engine plans, executes and verifies the task itself; this checkpoint preview is client-side guidance, not an engine plan.`,
          resources: suggestedResourcesForIntent(intent, activeBot?.name),
          steps: [
            { id: "step_inspect", text: "Inspect and analyze relevant resources and parameters", status: "running", done: false },
            { id: "step_execute", text: "Execute requested workflow safely with rollback records", status: "pending", done: false },
            { id: "step_verify", text: "Verify execution results and validate state integrity", status: "pending", done: false },
          ],
          status: "executing",
          accomplished: [],
          remaining: [
            "Inspect and analyze relevant resources and parameters",
            "Execute requested workflow safely with rollback records",
            "Verify execution results and validate state integrity",
          ],
        };
        activePlanRef.current = plan;
        setActivePlan(plan);
      }

      // The history snapshot is taken synchronously from the conversation the
      // run belongs to, so a later conversation switch cannot poison it.
      const historySnapshot = [...messages, userMsg].map((m) => ({
        role: m.role,
        content: m.content,
        tool_calls: m.tool_calls_json ? JSON.parse(m.tool_calls_json) : null,
        tool_call_id: null,
      }));

      const lastMessageContent = imageAttachment
        ? [
            {
              type: "text",
              text: fullPromptText.trim() ? fullPromptText : `Image attached: ${imageAttachment.name}`,
            },
            {
              type: "image_url",
              image_url: {
                url: `data:${imageAttachment.mimeType};base64,${imageAttachment.base64}`,
              },
            },
          ]
        : fullPromptText;

      const history = historySnapshot.slice(0, -1).concat([
        {
          role: userMsg.role,
          content: lastMessageContent as any,
          tool_calls: null,
          tool_call_id: null,
        },
      ]);

      let runErrored = false;
      let runCancelled = false;
      let finalReply = "";
      try {
        // The profile prompt is resolved engine-side (Stage C); the client can
        // only choose an agent id, never supply instructions.
        finalReply = (await invoke<string>("run_agent_turn", {
          runId,
          conversationId,
          model: selectedModel,
          botId: activeBot.id,
          history,
          systemPrompt: null,
        })) as string;
      } catch (err: any) {
        runErrored = true;
        const message = typeof err === "string" ? err : err?.message || String(err);
        runCancelled = /cancel/i.test(message);
        const assistantMsg: Message = {
          id: "msg_" + crypto.randomUUID(),
          conversation_id: conversationId,
          role: "assistant",
          content: runCancelled
            ? "Run cancelled by user request."
            : "Error running turn: " + message,
          created_at: Date.now(),
        };
        // Late results route to their owning conversation only: appended to
        // the visible list only when that conversation is still displayed
        // (Bug 2); persistence happens for the owning conversation regardless.
        if (activeConvIdRef.current === conversationId) {
          setMessages((prev) => [...prev, assistantMsg]);
        }
        await invoke("add_message", { msg: assistantMsg }).catch(console.error);
        if (runCancelled) {
          sendNotification("Kyro Task Stopped", "The run was cancelled per your request.");
        } else {
          sendNotification("Kyro Task Failed", message.slice(0, 120));
        }
        setActivePlan((prev) => {
          if (!prev || prev.runId !== runId) return prev;
          const next = runCancelled
            ? {
                ...prev,
                status: "cancelled" as const,
                steps: prev.steps.map((s) =>
                  s.status === "running" ? { ...s, status: "failed" as const, done: false, evidence: "Cancelled" } : s
                ),
                remaining: ["Execution cancelled by user request"],
              }
            : {
                ...prev,
                status: "failed" as const,
                steps: prev.steps.map((s) =>
                  s.status === "running" ? { ...s, status: "failed" as const, done: false } : s
                ),
                remaining: [`Encountered execution failure: ${message}`],
              };
          activePlanRef.current = next;
          return next;
        });
      }

      if (!runErrored) {
        const trimmedReply = (finalReply || "").trim();
        const hasValidReply = trimmedReply.length > 0;
        const assistantContent = hasValidReply
          ? trimmedReply
          : "No response was generated by the model.";

        const assistantMsg: Message = {
          id: "msg_" + crypto.randomUUID(),
          conversation_id: conversationId,
          role: "assistant",
          content: assistantContent,
          created_at: Date.now(),
        };
        if (activeConvIdRef.current === conversationId) {
          setMessages((prev) => [...prev, assistantMsg]);
        }
        await invoke("add_message", { msg: assistantMsg }).catch(console.error);

        // Final plan updates only from the run that owns the plan (Bug 6).
        // Functional so the freshest step-event evidence wins even when the
        // turn promise and queued step updates resolve in the same tick.
        setActivePlan((prev) => {
          if (!prev || prev.runId !== runId) return prev;
          const isVerified =
            prev.steps.find((s) => s.id === "step_verify")?.status === "completed";
          const hasFailedStep = prev.steps.some((s) => s.status === "failed");
          const hasUncertainStep = prev.steps.some((s) => s.status === "uncertain");
          const finalStatus: TaskPlan["status"] = (hasFailedStep || !hasValidReply)
            ? (hasValidReply && !hasFailedStep ? (isVerified ? "completed" : "unverified") : "failed")
            : hasUncertainStep
            ? "uncertain"
            : isVerified
            ? "completed"
            : "unverified";

          const remainingNotes = [...prev.remaining];
          if (!hasValidReply && !remainingNotes.some((n) => n.includes("No response generated"))) {
            remainingNotes.push("No response was generated by the model");
          }
          if (hasUncertainStep && !remainingNotes.some((n) => n.toLowerCase().includes("uncertain"))) {
            remainingNotes.push("One or more tool outcomes were inconclusive (uncertain postcondition)");
          }
          if (!isVerified && !hasFailedStep && !hasUncertainStep && hasValidReply && !remainingNotes.some((n) => n.toLowerCase().includes("verification"))) {
            remainingNotes.push("Explicit verification check not executed");
          }

          const next: TaskPlan = {
            ...prev,
            status: finalStatus,
            steps: prev.steps.map((s) => {
              if (s.id === "step_verify" && s.status !== "completed" && s.status !== "failed") {
                if (hasFailedStep) return s;
                return { ...s, status: "unverified" as const, done: false, evidence: "Unverified" };
              }
              if (s.status === "running") {
                return {
                  ...s,
                  status: (hasFailedStep ? "failed" : "completed") as TaskStep["status"],
                  done: !hasFailedStep,
                };
              }
              return s;
            }),
            remaining: remainingNotes,
          };
          activePlanRef.current = next;
          return next;
        });

        // Ordinary questions produce no plan; truthfully report the reply.
        if (!intent.isConsequential) {
          sendNotification("Kyro Reply Ready", finalReply.slice(0, 80));
        }
        // Planned runs report (Bug 6) once the plan reaches a terminal status —
        // handled by the notification effect below.
      }
    } catch (err: any) {
      // Submission-level failure (conversation creation, messaging, etc.).
      const message = typeof err === "string" ? err : err?.message || String(err);
      setNotice({ tone: "error", message: `Task submission failed: ${message}` });
    } finally {
      // Release ONLY the lock owned by this submission (Bug 2), and drop the
      // run from the in-flight registry whatever the outcome was.
      if (registeredConvId && submitLocksRef.current.get(registeredConvId) === submitId) {
        submitLocksRef.current.delete(registeredConvId);
      } else if (!registeredConvId && submitLocksRef.current.get(lockKey) === submitId) {
        submitLocksRef.current.delete(lockKey);
      }
      if (myRunId) {
        inFlightRunsRef.current.delete(myRunId);
      }
      syncAgentBusy();
      setActiveReasoning(null);
    }
  };

  /**
   * Stops the displayed run (Bug 1).
   *
   * Cancellation is only claimed when the backend confirms the transition:
   *  - `true`  → confirmed cancelled; UI may mark the plan cancelled;
   *  - `false` → the run was already terminal; state is reconciled instead
   *              of pretending that a cancellation happened;
   *  - `Err`   → either the run is still inside its setup window (the
   *              cancellation was registered and WILL abort its first
   *              execution step) or cancelling failed. The UI reports the
   *              truth and does not fake a cancelled state.
   */
  const handleAbort = async () => {
    const runId = activeRunIdRef.current ?? activeRunId;
    if (!runId) {
      await invoke("abort_computer_use").catch(console.error);
      setNotice({ tone: "info", message: "There is no running task to stop in this conversation." });
      return;
    }

    let confirmResult: boolean | null = null;
    let failureMessage = "";
    try {
      confirmResult = await invoke<boolean>("cancel_agent_run", { runId });
    } catch (err: any) {
      failureMessage = typeof err === "string" ? err : err?.message || String(err);
    }

    // Any ghost/overlay capture is aborted regardless of the run outcome.
    await invoke("abort_computer_use").catch(console.error);

    if (confirmResult === true) {
      // Backend CONFIRMED the transition — only now is "cancelled" truthful.
      setActivePlan((prev) =>
        prev && prev.runId === runId
          ? {
              ...prev,
              status: "cancelled" as const,
              steps: prev.steps.map((s) =>
                s.status === "running" ? { ...s, status: "failed" as const, done: false, evidence: "Cancelled" } : s
              ),
              remaining: ["Execution cancelled by user request"],
            }
          : prev
      );
      inFlightRunsRef.current.delete(runId);
      syncAgentBusy();
      setActiveReasoning(null);
      setNotice({ tone: "info", message: "Task stopped." });
      return;
    }

    if (confirmResult === false) {
      // Already terminal (succeeded/failed/cancelled): reconcile the busy
      // state instead of claiming a fresh cancellation.
      inFlightRunsRef.current.delete(runId);
      syncAgentBusy();
      setActiveReasoning(null);
      setNotice({ tone: "info", message: "The task had already finished; nothing was cancelled." });
      return;
    }

    if (failureMessage.includes("still being created")) {
      // The cancellation is registered against the run id and will abort the
      // run's first execution step: requested-but-not-yet-confirmed is
      // truthful here. The submitting handler stays in flight and will
      // resolve with the cancelled outcome.
      setNotice({ tone: "info", message: `Stopping: ${failureMessage}` });
      return;
    }

    setNotice({ tone: "error", message: `Could not stop the task: ${failureMessage}` });
  };

  const handleCaptureScreen = async () => {
    try {
      const res = await invoke<any>("take_screenshot");
      setScreenPreview(`data:image/png;base64,${res.base64_png}`);
    } catch (err) {
      setNotice({ tone: "error", message: "Screen capture error: " + err });
    }
  };

  const handleToggleGhostMode = async () => {
    try {
      const next = !isGhostMode;
      await invoke("toggle_ghost_mode", { enabled: next });
      setIsGhostMode(next);
    } catch (err: any) {
      setNotice({ tone: "error", message: "Error toggling Ghost Mode: " + err });
    }
  };

  const handleCaptureGrounded = async () => {
    try {
      const res = await invoke<GroundedScreenResult>("capture_grounded_screen");
      setGroundedResult(res);
    } catch (err: any) {
      setNotice({ tone: "error", message: "Set-of-Marks grounding error: " + err });
    }
  };

  const resolveAgentDisplayName = (): string => {
    if (activeReasoning?.isChildRun && activeReasoning.agentId) {
      const cleanId = activeReasoning.agentId.replace(/^@/, "");
      const matched = allProfiles.find(
        (p) => p.id === cleanId || p.name.toLowerCase() === cleanId.toLowerCase()
      );
      if (matched) return `@${matched.name}`;
      return `@${cleanId}`;
    }
    if (activeBot?.name && activeBot.name !== "Kyro") {
      return `@${activeBot.name}`;
    }
    return "Kyro";
  };

  return (
    <div className="flex h-screen w-screen bg-[#0a0a0a] text-neutral-100 overflow-hidden font-sans select-none">
      {/* Sidebar */}
      <div className="w-64 lg:w-72 bg-[#0c0c0c] border-r border-[#1f1f1f] flex flex-col justify-between shrink-0 select-none">
        <div className="p-3.5 flex flex-col h-full overflow-hidden">
          {/* Brand & Action Header */}
          <div className="flex items-center justify-between pb-3.5 border-b border-[#1f1f1f]">
            <div className="flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-lg bg-[#141414] border border-[#242424] flex items-center justify-center p-1.5 shadow-sm overflow-hidden shrink-0">
                <KyroLogo className="w-full h-full" />
              </div>
              <div className="min-w-0">
                <h1 className="font-semibold text-sm tracking-wide text-neutral-100 leading-none">KYRO</h1>
                <p className="text-[10px] text-neutral-400 font-mono mt-1">Autonomous OS</p>
              </div>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={() => setIsCommandPaletteOpen(true)}
                className="p-1.5 text-neutral-400 hover:text-neutral-100 hover:bg-[#1a1a1a] rounded-lg transition"
                title="Quick Actions & Search (Ctrl+K)"
              >
                <Command className="w-4 h-4" />
              </button>
              <button
                onClick={() => invoke("toggle_hud_window")}
                className="p-1.5 text-neutral-400 hover:text-neutral-100 hover:bg-[#1a1a1a] rounded-lg transition"
                title="Toggle Floating HUD Widget"
              >
                <ExternalLink className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Experience Mode Switcher */}
          <div className="my-2.5 p-1 bg-[#141414] border border-[#222222] rounded-xl grid grid-cols-2 gap-1 text-xs">
            <button
              onClick={() => handleSetExperienceMode("everyday")}
              className={`py-1.5 px-2 rounded-lg font-medium flex items-center justify-center gap-1.5 transition ${
                experienceMode === "everyday"
                  ? "bg-[#222222] text-neutral-100 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200"
              }`}
              title="Kyro Everyday: Screen intelligence, file organizer, document review & automations"
            >
              <Sparkles className="w-3.5 h-3.5 text-amber-400" />
              <span>Everyday</span>
            </button>
            <button
              onClick={() => handleSetExperienceMode("dev")}
              className={`py-1.5 px-2 rounded-lg font-medium flex items-center justify-center gap-1.5 transition ${
                experienceMode === "dev"
                  ? "bg-purple-950/70 text-purple-200 border border-purple-800/60 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200"
              }`}
              title="Kyro Dev: Autonomous developer agent, dev servers, diffs, checkpoints & visual regression"
            >
              <Code2 className="w-3.5 h-3.5 text-purple-400" />
              <span>Dev Mode</span>
            </button>
          </div>

          {/* New Chat Button */}
          <button
            onClick={handleNewConversation}
            className="mb-3 w-full py-2 px-3 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 text-xs font-semibold flex items-center justify-center gap-1.5 transition shadow-sm"
          >
            <Plus className="w-4 h-4" /> New Session
          </button>

          {/* Active Teammate & Model Identity Card */}
          <div className="mb-3 p-2.5 bg-[#121212] border border-[#1f1f1f] rounded-xl space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[9px] uppercase font-mono tracking-wider text-neutral-400">
                Active Teammate
              </span>
              <button
                onClick={() => setIsTeammatesOpen(true)}
                className="text-[10px] text-neutral-400 hover:text-neutral-200 hover:underline flex items-center gap-0.5"
              >
                Change
              </button>
            </div>
            <div
              onClick={() => setIsTeammatesOpen(true)}
              className="flex items-center gap-2.5 cursor-pointer hover:opacity-90 transition"
            >
              <AgentAvatar
                avatar={activeBot.avatar}
                name={activeBot.name}
                className="w-7 h-7 rounded-lg bg-black/40 border border-[#242424] flex items-center justify-center text-neutral-200 shrink-0"
                iconClassName="w-3.5 h-3.5"
              />
              <div className="min-w-0">
                <div className="text-xs font-semibold text-neutral-100 truncate">@{activeBot.name}</div>
                <div className="text-[10px] text-neutral-400 truncate">{activeBot.role}</div>
              </div>
            </div>

            {(() => {
              const info = getModelDisplayInfo(selectedModel);
              return (
                <div
                  onClick={() => setIsSettingsOpen(true)}
                  className="pt-2 border-t border-[#1f1f1f] flex items-center justify-between text-[10px] font-mono text-neutral-400 cursor-pointer hover:text-neutral-200 transition"
                  title={`Active model: ${selectedModel}. Click to configure in Settings.`}
                >
                  <span className="flex items-center gap-1.5 truncate">
                    {info.isNvidia ? (
                      <NvidiaIcon className="w-3.5 h-3.5 text-[#76B900] shrink-0" />
                    ) : (
                      <Cpu className="w-3.5 h-3.5 text-purple-400 shrink-0" />
                    )}
                    <span className="truncate">{info.label}</span>
                  </span>
                  <span className="text-[9px] px-1 py-0.2 bg-neutral-900 rounded border border-[#242424] text-neutral-300 shrink-0 ml-1">
                    {info.badge}
                  </span>
                </div>
              );
            })()}
          </div>

          {/* Navigation Destinations */}
          <div className="space-y-1 mb-2.5">
            {experienceMode === "everyday" ? (
              <>
                {/* Everyday Primary Destinations */}
                <div className="space-y-0.5 mb-2">
                  <div className="px-2 py-0.5 text-[9px] font-mono uppercase tracking-wider text-neutral-400">
                    Destinations
                  </div>
                  <button
                    onClick={() => setEverydayTab("ask")}
                    className={`w-full px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center justify-between transition ${
                      everydayTab === "ask"
                        ? "bg-[#1c1c1c] text-neutral-100 font-semibold border border-[#282828] shadow-sm"
                        : "text-neutral-400 hover:text-neutral-200 hover:bg-[#141414] border border-transparent"
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      <MessageSquare className="w-3.5 h-3.5 text-neutral-300" /> Ask Kyro
                    </span>
                    {everydayTab === "ask" && <span className="w-1.5 h-1.5 rounded-full bg-neutral-300" />}
                  </button>

                  <button
                    onClick={() => setEverydayTab("today")}
                    className={`w-full px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center justify-between transition ${
                      everydayTab === "today"
                        ? "bg-[#1c1c1c] text-neutral-100 font-semibold border border-[#282828] shadow-sm"
                        : "text-neutral-400 hover:text-neutral-200 hover:bg-[#141414] border border-transparent"
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      <Calendar className="w-3.5 h-3.5 text-sky-400" /> Today
                    </span>
                    {everydayTab === "today" && <span className="w-1.5 h-1.5 rounded-full bg-sky-400" />}
                  </button>

                  <button
                    onClick={() => setEverydayTab("tools")}
                    className={`w-full px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center justify-between transition ${
                      everydayTab === "tools"
                        ? "bg-[#1c1c1c] text-neutral-100 font-semibold border border-[#282828] shadow-sm"
                        : "text-neutral-400 hover:text-neutral-200 hover:bg-[#141414] border border-transparent"
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      <Wrench className="w-3.5 h-3.5 text-amber-400" /> My Tools
                    </span>
                    {everydayTab === "tools" && <span className="w-1.5 h-1.5 rounded-full bg-amber-400" />}
                  </button>

                  <button
                    onClick={() => setEverydayTab("routines")}
                    className={`w-full px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center justify-between transition ${
                      everydayTab === "routines"
                        ? "bg-[#1c1c1c] text-neutral-100 font-semibold border border-[#282828] shadow-sm"
                        : "text-neutral-400 hover:text-neutral-200 hover:bg-[#141414] border border-transparent"
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      <Clock className="w-3.5 h-3.5 text-purple-400" /> My Routines
                    </span>
                    {everydayTab === "routines" && <span className="w-1.5 h-1.5 rounded-full bg-purple-400" />}
                  </button>
                </div>

                <div className="space-y-0.5 pt-1.5 border-t border-[#1a1a1a]">
                  <button
                    onClick={() => setIsScreenExplainerOpen(true)}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-amber-200 bg-amber-950/20 hover:bg-amber-950/40 border border-amber-800/40 flex items-center gap-2 transition"
                  >
                    <Camera className="w-3.5 h-3.5 text-amber-400" /> Explain Screen
                  </button>
                  <button
                    onClick={() => setIsFileOrganizerOpen(true)}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                  >
                    <FolderTree className="w-3.5 h-3.5 text-neutral-400" /> Organize Files
                  </button>
                  <button
                    onClick={() => setIsDocumentAssistantOpen(true)}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                  >
                    <FileText className="w-3.5 h-3.5 text-neutral-400" /> Document Assistant
                  </button>
                  <button
                    onClick={() => handleOpenTeachTask("routines")}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                  >
                    <Workflow className="w-3.5 h-3.5 text-neutral-400" /> Automations & Routines
                  </button>
                  <button
                    onClick={() => setIsArtifactsOpen(true)}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                  >
                    <Layers className="w-3.5 h-3.5 text-neutral-400" /> Artifacts Registry
                  </button>
                  <button
                    onClick={() => setIsConnectionsOpen(true)}
                    className="w-full px-2.5 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                  >
                    <Plug className="w-3.5 h-3.5 text-neutral-400" /> App Connections
                  </button>
                </div>
              </>
            ) : (
              <>
                <button
                  onClick={() => setIsDevWorkbenchOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-semibold text-purple-200 bg-purple-950/40 hover:bg-purple-950/60 border border-purple-800/70 flex items-center justify-between transition shadow-sm"
                >
                  <span className="flex items-center gap-2">
                    <Code2 className="w-4 h-4 text-purple-400" /> Dev Workbench
                  </span>
                  <span className="text-[9px] font-mono uppercase bg-purple-900/60 px-1 py-0.5 rounded text-purple-300">
                    Active
                  </span>
                </button>
                <button
                  onClick={() => setIsConnectionsOpen(true)}
                  className="w-full px-3 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Plug className="w-3.5 h-3.5 text-neutral-400" /> App Connections
                </button>
                <button
                  onClick={() => setIsArtifactsOpen(true)}
                  className="w-full px-3 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Layers className="w-3.5 h-3.5 text-neutral-400" /> Artifacts Registry
                </button>
                <button
                  onClick={() => handleOpenTeachTask("skills")}
                  className="w-full px-3 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Workflow className="w-3.5 h-3.5 text-neutral-400" /> Teach-a-Task & Routines
                </button>
                <button
                  onClick={() => setIsTeammatesOpen(true)}
                  className="w-full px-3 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Users className="w-3.5 h-3.5 text-neutral-400" /> Teammate Roster
                </button>
                <button
                  onClick={() => setIsMcpOpen(true)}
                  className="w-full px-3 py-1.5 rounded-lg text-xs font-medium text-neutral-300 hover:bg-[#161616] hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Server className="w-3.5 h-3.5 text-neutral-400" /> MCP Tool Servers
                </button>
              </>
            )}
          </div>

          {/* Conversations list */}
          <div className="flex-1 overflow-y-auto space-y-1 pr-1 border-t border-[#1f1f1f] pt-2.5">
            <div className="text-[10px] uppercase font-mono tracking-wider text-neutral-400 mb-1.5 px-2">
              Recent Sessions
            </div>
            {conversations.map((c) => (
              <div
                key={c.id}
                onClick={() => setActiveConvId(c.id)}
                className={`group px-2.5 py-1.5 rounded-lg text-xs cursor-pointer truncate transition flex items-center justify-between ${
                  activeConvId === c.id
                    ? "bg-[#1c1c1c] text-neutral-100 border border-[#282828] font-medium"
                    : "text-neutral-400 hover:bg-[#141414] hover:text-neutral-200 border border-transparent"
                }`}
              >
                <span className="truncate">{c.title}</span>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    handlePromptDeleteSession(c);
                  }}
                  title="Delete session"
                  aria-label="Delete session"
                  data-testid={`delete-session-button-${c.id}`}
                  className="opacity-0 group-hover:opacity-100 focus:opacity-100 p-1 text-neutral-400 hover:text-red-400 rounded transition shrink-0 ml-2"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
        </div>

        {/* Bottom Sidebar: Settings */}
        <div className="p-3 border-t border-[#1f1f1f] bg-[#0a0a0a]">
          <button
            onClick={() => {
              setSettingsInitialTab("model");
              setIsSettingsOpen(true);
            }}
            className="w-full px-3 py-2 rounded-lg text-xs text-neutral-400 hover:text-neutral-100 hover:bg-[#141414] border border-transparent hover:border-[#222222] transition flex items-center justify-between"
          >
            <span className="flex items-center gap-2">
              <Settings className="w-4 h-4 text-neutral-400" /> Vault & Settings
            </span>
            <ChevronRight className="w-3.5 h-3.5 text-neutral-500" />
          </button>
        </div>
      </div>

      {/* Main Workspace Area */}
      <div className="flex-1 flex flex-col h-full overflow-hidden bg-[#0a0a0a]">
        {/* Top Navbar */}
        <div className="h-14 px-6 border-b border-[#1f1f1f] flex items-center justify-between bg-[#0a0a0a]">
          <div className="flex items-center gap-3">
            <AgentAvatar
              avatar={activeBot.avatar}
              name={activeBot.name}
              className="w-8 h-8 rounded-lg bg-[#141414] border border-[#242424] flex items-center justify-center text-neutral-200 shrink-0"
              iconClassName="w-4 h-4"
            />
            <div>
              <div className="text-xs font-semibold text-neutral-100 flex items-center gap-1.5">
                <span>@{activeBot.name}</span>
                <span className="text-[10px] text-neutral-400 font-normal">• {activeBot.role}</span>
              </div>
              <div className="text-[10px] text-neutral-400 font-mono flex items-center gap-1.5">
                <NvidiaIcon className="w-3 h-3 text-[#76B900]" />
                <span>nvidia/nemotron-3-ultra-550b-a55b</span>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {experienceMode === "everyday" ? (
              <>
                <button
                  onClick={() => setIsOnboardingOpen(true)}
                  className="px-2.5 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition font-medium"
                  title="Start Interactive User Guide"
                >
                  <HelpCircle className="w-3.5 h-3.5 text-neutral-400" /> Guide
                </button>
                <button
                  onClick={() => setIsScreenExplainerOpen(true)}
                  className="px-3 py-1.5 bg-amber-950/30 hover:bg-amber-950/50 border border-amber-800/50 rounded-lg text-xs text-amber-200 flex items-center gap-1.5 transition font-medium"
                  title="Explain My Screen"
                >
                  <Camera className="w-3.5 h-3.5 text-amber-400" /> Explain Screen
                </button>
                <button
                  onClick={() => setIsFileOrganizerOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Organize Files"
                >
                  <FolderTree className="w-3.5 h-3.5 text-neutral-400" /> Organize
                </button>
                <button
                  onClick={() => setIsDocumentAssistantOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Local Document Assistant"
                >
                  <FileText className="w-3.5 h-3.5 text-neutral-400" /> Documents
                </button>
                <button
                  onClick={() => setIsTimelineOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Inspect step-by-step history"
                >
                  <History className="w-3.5 h-3.5 text-neutral-400" /> Timeline
                </button>
                <button
                  onClick={handleCaptureScreen}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Capture display snapshot"
                >
                  <Camera className="w-3.5 h-3.5 text-neutral-400" /> Screen
                </button>
              </>
            ) : (
              <>
                <button
                  onClick={() => setIsDevWorkbenchOpen(true)}
                  className="px-3 py-1.5 bg-purple-950/50 hover:bg-purple-900/50 border border-purple-800/70 rounded-lg text-xs text-purple-200 flex items-center gap-1.5 transition font-medium shadow-sm"
                  title="Open Developer Workbench (Projects, Dev Servers, Checkpoints, Build Doctor)"
                >
                  <Code2 className="w-3.5 h-3.5 text-purple-300" /> Workbench
                </button>
                <button
                  onClick={handleToggleGhostMode}
                  className={`px-3 py-1.5 border rounded-lg text-xs font-medium flex items-center gap-1.5 transition ${
                    isGhostMode
                      ? "bg-purple-950/40 text-purple-200 border-purple-700/60 shadow-sm"
                      : "bg-[#121212] hover:bg-[#1a1a1a] text-neutral-400 border-[#242424]"
                  }`}
                  title="Runs actions inside isolated virtual display (:99). Your physical mouse & screen stay 100% free!"
                >
                  <Ghost
                    className={`w-3.5 h-3.5 ${isGhostMode ? "text-purple-300 animate-pulse" : "text-neutral-400"}`}
                  />
                  {isGhostMode ? "Ghost (:99)" : "Ghost"}
                </button>
                <button
                  onClick={handleCaptureGrounded}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Detect UI elements and render numbered markers [1], [2], [3] for zero-miss clicks"
                >
                  <Target className="w-3.5 h-3.5 text-neutral-400" /> Ground
                </button>
                <button
                  onClick={() => setIsTimelineOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Inspect step-by-step before/after diffs with click target rings"
                >
                  <History className="w-3.5 h-3.5 text-neutral-400" /> Timeline
                </button>
                <button
                  onClick={handleCaptureScreen}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Capture display snapshot"
                >
                  <Camera className="w-3.5 h-3.5 text-neutral-400" /> Screen
                </button>
                <button
                  onClick={() => invoke("toggle_overlay_window", { show: true })}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Snip region of screen"
                >
                  <Crop className="w-3.5 h-3.5 text-neutral-400" /> Snip
                </button>
                <button
                  onClick={() => setShowTerminal(!showTerminal)}
                  className={`px-3 py-1.5 border rounded-lg text-xs flex items-center gap-1.5 transition ${
                    showTerminal
                      ? "bg-neutral-800 text-neutral-100 border-neutral-700"
                      : "bg-[#121212] hover:bg-[#1a1a1a] text-neutral-300 border-[#242424]"
                  }`}
                >
                  <Terminal className="w-3.5 h-3.5 text-neutral-400" /> PTY
                </button>
              </>
            )}

            {/* Quick Actions / Palette */}
            <button
              onClick={() => setIsCommandPaletteOpen(true)}
              className="px-2.5 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-[#242424] rounded-lg text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
              title="Global Command Palette (Ctrl+K)"
            >
              <Command className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Human Takeover Alert Banner */}
        {takeover?.is_active && (
          <div className="px-6 py-2.5 bg-neutral-900 border-b border-neutral-800 flex items-center justify-between text-xs text-neutral-200">
            <div className="flex items-center gap-2">
              <AlertTriangle className="w-4 h-4 text-amber-400 animate-bounce" />
              <span>
                <strong>Human Takeover Active</strong>:{" "}
                {takeover.reason || "Autonomous execution paused for manual verification."}
              </span>
            </div>
            <button
              onClick={handleResumeTakeover}
              className="px-3 py-1 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium flex items-center gap-1.5 shadow-sm transition"
            >
              <Play className="w-3.5 h-3.5 fill-current" /> Resume Autonomous Agent
            </button>
          </div>
        )}

        {/* Actionable Governance Approval Queue Banner */}
        {pendingApprovals.length > 0 && (
          <div className="px-6 py-3 bg-[#111111] border-b border-neutral-800 space-y-3" data-testid="approval-queue-banner">
            <div className="flex items-center justify-between text-xs font-semibold text-neutral-300">
              <span className="flex items-center gap-1.5">
                <ShieldAlert className="w-4 h-4 text-amber-400" />
                Action Authorization Required ({pendingApprovals.length} pending)
              </span>
              <span className="text-[10px] text-neutral-400 font-normal">
                Consequential operations paused until approved or denied
              </span>
            </div>
            {pendingApprovals.map((req) => {
              const details = parseApprovalArgs(req.args_json);
              const isExpanded = !!expandedApprovalIds[req.id];
              const isRaw = !!showRawArgs[req.id];
              const isDestructive = req.risk_level === "destructive";
              const nowSec = Math.floor(Date.now() / 1000);
              const expiresAtSec = req.expires_at > 1e11 ? Math.floor(req.expires_at / 1000) : req.expires_at;
              const isExpired = expiresAtSec > 0 && nowSec > expiresAtSec;

              return (
                <div
                  key={req.id}
                  data-testid="approval-card"
                  className="bg-[#161616] p-3.5 rounded-xl border border-neutral-700/80 text-xs shadow-md space-y-2.5"
                >
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex items-start gap-3 min-w-0 flex-1">
                      <ShieldAlert
                        className={`w-5 h-5 flex-shrink-0 mt-0.5 ${
                          isDestructive ? "text-red-400 animate-pulse" : "text-amber-400"
                        }`}
                      />
                      <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-semibold text-neutral-100">Tool Approval Required:</span>
                          <span
                            className={`px-2 py-0.5 rounded font-mono text-[10px] uppercase font-bold ${
                              isDestructive
                                ? "bg-red-950/70 text-red-300 border border-red-800/80"
                                : req.risk_level === "moderate"
                                ? "bg-amber-950/70 text-amber-300 border border-amber-800/80"
                                : "bg-neutral-800 text-neutral-300 border border-neutral-700"
                            }`}
                          >
                            {req.risk_level} Risk
                          </span>
                          <span
                            className={`px-1.5 py-0.5 rounded font-mono text-[10px] uppercase ${
                              isDestructive
                                ? "bg-red-900/30 text-red-400 border border-red-800/40"
                                : "bg-neutral-800 text-neutral-400 border border-neutral-700"
                            }`}
                          >
                            {isDestructive ? "Irreversible" : "Reversible"}
                          </span>
                          <span className="font-mono text-neutral-200 text-[11px] font-semibold bg-black/40 px-2 py-0.5 rounded border border-neutral-800">
                            {req.tool_name}
                          </span>
                          {isExpired && (
                            <span className="px-2 py-0.5 rounded font-mono text-[10px] bg-red-950 text-red-400 border border-red-800">
                              EXPIRED
                            </span>
                          )}
                        </div>

                        {details.targetDetail && (
                          <div className="font-mono text-[11px] text-amber-200 bg-neutral-900/90 px-2 py-1 rounded border border-neutral-800 break-all select-text">
                            {details.targetDetail}
                          </div>
                        )}

                        <p className="text-neutral-300 text-[11px] break-words select-text">
                          {req.summary}
                        </p>

                        {req.reason && (
                          <p className="text-neutral-400 text-[10px] italic">
                            Policy: {req.reason}
                          </p>
                        )}
                      </div>
                    </div>

                    {/* Action buttons */}
                    <div className="flex flex-col sm:flex-row items-end sm:items-center gap-2 flex-shrink-0">
                      <button
                        type="button"
                        onClick={() =>
                          setExpandedApprovalIds((prev) => ({
                            ...prev,
                            [req.id]: !prev[req.id],
                          }))
                        }
                        className="px-2.5 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border border-neutral-700 text-[11px] font-medium transition"
                        aria-label="Inspect arguments"
                      >
                        {isExpanded ? "Hide Arguments" : "Inspect Arguments"}
                      </button>
                      <button
                        type="button"
                        onClick={() => handleResolveApproval(req.id, "approved")}
                        disabled={isExpired}
                        className="px-3 py-1.5 rounded-lg bg-white hover:bg-neutral-100 disabled:opacity-40 text-neutral-950 font-semibold text-xs flex items-center gap-1 shadow-sm transition"
                      >
                        <CheckCircle className="w-3.5 h-3.5" /> Approve
                      </button>
                      <button
                        type="button"
                        onClick={() => handleResolveApproval(req.id, "denied")}
                        className="px-3 py-1.5 rounded-lg bg-neutral-850 hover:bg-neutral-800 text-neutral-200 border border-neutral-700 font-medium text-xs flex items-center gap-1 transition"
                      >
                        <XCircle className="w-3.5 h-3.5" /> Deny
                      </button>
                      <button
                        type="button"
                        onClick={() => handleCancelTaskForApproval(req.id, req.run_id)}
                        className="px-3 py-1.5 rounded-lg bg-red-950/40 hover:bg-red-900/60 text-red-300 border border-red-800/50 font-medium text-xs flex items-center gap-1 transition"
                      >
                        <X className="w-3.5 h-3.5" /> Cancel Task
                      </button>
                    </div>
                  </div>

                  {/* Expanded Inspection view */}
                  {isExpanded && (
                    <div className="pt-2 border-t border-neutral-800/80 space-y-2">
                      <div className="flex items-center justify-between text-[10px] text-neutral-400">
                        <span>Exact Payload Arguments:</span>
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() =>
                              setShowRawArgs((prev) => ({
                                ...prev,
                                [req.id]: !prev[req.id],
                              }))
                            }
                            className="text-neutral-400 hover:text-neutral-200 underline cursor-pointer"
                          >
                            {isRaw ? "Show Redacted" : "Show Raw JSON"}
                          </button>
                          <button
                            type="button"
                            onClick={async () => {
                              try {
                                await navigator.clipboard.writeText(
                                  isRaw ? req.args_json : details.formattedRedacted
                                );
                                setNotice({ tone: "success", message: "Arguments copied to clipboard" });
                              } catch {}
                            }}
                            className="text-neutral-400 hover:text-neutral-200 underline cursor-pointer"
                          >
                            Copy
                          </button>
                        </div>
                      </div>
                      <pre className="p-2.5 bg-black rounded-lg border border-neutral-800 text-neutral-300 overflow-x-auto max-h-48 font-mono text-[11px] leading-relaxed select-text whitespace-pre-wrap break-all m-0">
                        {isRaw ? req.args_json : details.formattedRedacted}
                      </pre>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* Screen Preview Modal / Bar */}
        {screenPreview && (
          <div className="p-3 bg-black/80 border-b border-neutral-800 flex items-center justify-between gap-4">
            <div className="flex items-center gap-3">
              <img
                src={screenPreview}
                alt="Screenshot"
                className="h-16 rounded border border-neutral-700 shadow"
              />
              <span className="text-xs text-neutral-300">Display Snapshot ready</span>
            </div>
            <button
              onClick={() => setScreenPreview(null)}
              className="text-xs text-neutral-400 hover:text-white px-2 py-1 transition-colors"
            >
              Dismiss
            </button>
          </div>
        )}

        {/* Set-of-Marks Grounded Preview Bar */}
        {groundedResult && (
          <div className="p-3 bg-neutral-900 border-b border-neutral-800 flex items-center justify-between gap-4">
            <div className="flex items-center gap-3">
              <img
                src={`data:image/png;base64,${groundedResult.base64_png}`}
                alt="Grounded Screenshot"
                className="h-16 rounded border border-neutral-700 shadow"
              />
              <div className="text-xs text-neutral-200">
                <div className="font-semibold">Set-of-Marks (SoM) Grounding Active</div>
                <div className="text-[11px] text-neutral-400">
                  {(groundedResult.marks || []).length} grounded element
                  {(groundedResult.marks || []).length === 1 ? "" : "s"} detected (
                  {groundedResult.source}). Each tag carries the measurements that produced it; a
                  tag becomes stale when the screen changes.
                </div>
              </div>
            </div>
            <button
              onClick={() => setGroundedResult(null)}
              className="text-xs text-neutral-400 hover:text-white px-2 py-1 transition-colors"
            >
              Dismiss
            </button>
          </div>
        )}

        {/* Non-blocking operation feedback */}
        {notice && (
          <div className="px-4 pt-3">
            <InlineNotice
              tone={notice.tone}
              message={notice.message}
              onDismiss={() => setNotice(null)}
            />
          </div>
        )}

        {/* Main Workspace Content */}
        {experienceMode === "everyday" && everydayTab === "today" ? (
          <div className="flex-1 overflow-y-auto">
            <TodayDashboardView
              onOpenTool={(toolId, initialTab) => {
                if (toolId === "routines") {
                  if (initialTab === "history") {
                    setRoutinesInitialTab("history");
                  } else {
                    setRoutinesInitialTab("routines");
                  }
                  setEverydayTab("routines");
                } else {
                  if (toolId === "files" || toolId === "study" || toolId === "screen" || toolId === "web" || toolId === "connected") {
                    setToolsInitialTab(toolId);
                  }
                  setEverydayTab("tools");
                }
              }}
              onAskPrompt={(p) => {
                setPrompt(p);
                setEverydayTab("ask");
              }}
              onOpenConnections={() => setIsConnectionsOpen(true)}
              onOpenSettings={(tab) => {
                if (tab) setSettingsInitialTab(tab);
                setIsSettingsOpen(true);
              }}
            />
          </div>
        ) : experienceMode === "everyday" && everydayTab === "tools" ? (
          <div className="flex-1 overflow-y-auto">
            <MyToolsView
              initialTab={toolsInitialTab}
              onAskPrompt={(p) => {
                setPrompt(p);
                setEverydayTab("ask");
              }}
              onOpenConnections={() => setIsConnectionsOpen(true)}
              onOpenScreenExplainer={() => setIsScreenExplainerOpen(true)}
              onOpenFileOrganizer={() => setIsFileOrganizerOpen(true)}
              onOpenDocAssistant={() => setIsDocumentAssistantOpen(true)}
            />
          </div>
        ) : experienceMode === "everyday" && everydayTab === "routines" ? (
          <div className="flex-1 overflow-y-auto">
            <MyRoutinesView
              initialTab={routinesInitialTab}
              onAskPrompt={(p) => {
                setPrompt(p);
                setEverydayTab("ask");
              }}
            />
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto p-6 space-y-6">
            {/* Structured Task Plan Card — single source of truth: the
                 TaskProgress component renders the same plan state the
                 submission/step logic maintains (Bug 18). A client-side plan
                 is labeled a Suggested Workflow, never an engine plan. */}
            {activePlan && (
              <TaskProgress
                plan={activePlan}
                activeConversationId={activeConvId}
                onDismiss={() => setActivePlan(null)}
              />
            )}

            {(!messages || messages.length === 0) && (!liveSteps || liveSteps.length === 0) ? (
            <div className="h-full flex flex-col items-center justify-center max-w-3xl mx-auto py-10 px-6 space-y-7 text-center">
              {/* Brand and Teammate Header */}
              <div className="flex flex-col items-center space-y-3">
                <div className="relative">
                  <div className="w-16 h-16 rounded-2xl bg-[#141414] border border-[#242424] flex items-center justify-center p-3 shadow-xl">
                    <KyroLogo className="w-full h-full" />
                  </div>
                  <div className="absolute -bottom-1 -right-1 p-1 rounded-full bg-[#1c1c1c] border border-[#2e2e2e] shadow flex items-center justify-center">
                    <AgentAvatar
                      avatar={activeBot.avatar}
                      name={activeBot.name}
                      className="w-4 h-4 rounded-full bg-transparent flex items-center justify-center text-neutral-200"
                      iconClassName="w-3 h-3"
                    />
                  </div>
                </div>

                {experienceMode === "everyday" ? (
                  <div className="space-y-2 max-w-xl">
                    <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-[#141414] border border-[#262626] text-xs font-medium text-neutral-300">
                      <Sparkles className="w-3.5 h-3.5 text-amber-400" />
                      <span>Kyro Everyday Assistant</span>
                    </div>
                    <h2 className="text-2xl sm:text-3xl font-bold tracking-tight text-neutral-100">
                      What should Kyro take care of?
                    </h2>
                    <p className="text-sm text-neutral-400 leading-relaxed">
                      Understand screens, safely organize downloads, analyze documents, and automate repetitive tasks with zero developer jargon.
                    </p>
                  </div>
                ) : (
                  <div className="space-y-2 max-w-xl">
                    <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-purple-950/40 border border-purple-800/60 text-xs font-medium text-purple-300">
                      <Code2 className="w-3.5 h-3.5 text-purple-400" />
                      <span>Kyro Autonomous Workspace</span>
                      <span className="px-1.5 py-0.2 rounded text-[9px] font-mono bg-purple-900/60 text-purple-200 font-bold uppercase">DEV</span>
                    </div>
                    <h2 className="text-2xl sm:text-3xl font-bold tracking-tight text-neutral-100">
                      Autonomous Developer Environment
                    </h2>
                    <p className="text-sm text-neutral-400 leading-relaxed">
                      Local-first developer agent with managed dev-servers, reversible checkpoints, Build Doctor diagnostics, and responsive visual regression.
                    </p>
                  </div>
                )}

                {/* Subdued teammate context */}
                <div className="flex items-center justify-center gap-2 pt-1 text-xs text-neutral-400">
                  <div className="flex items-center gap-1.5 px-3 py-1 rounded-lg bg-[#111111] border border-[#1f1f1f]">
                    <span className="text-neutral-500 font-mono text-[11px]">Ready with:</span>
                    <span className="text-neutral-200 font-medium">@{activeBot.name}</span>
                    <span className="text-neutral-500">•</span>
                    <span className="text-neutral-400 text-[11px] truncate max-w-[240px]">{activeBot.role}</span>
                  </div>
                </div>
              </div>

              {/* 4 Curated Task Cards */}
              {experienceMode === "everyday" ? (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 text-left w-full pt-1">
                  <button
                    onClick={() => setIsScreenExplainerOpen(true)}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-amber-700/50 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-amber-950/30 border border-amber-800/40 text-amber-400 group-hover:bg-amber-950/50 transition">
                          <Camera className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Explain Screen
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Diagnose visual errors, find buttons, and get grounded explanations.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => setIsFileOrganizerOpen(true)}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <FolderTree className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Organize Files
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Categorize cluttered folders, find SHA-256 duplicates with safe undo.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => setIsDocumentAssistantOpen(true)}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <FileText className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Document Assistant
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Parse text and PDFs locally. Extract key dates, actions & compare docs.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => handleOpenTeachTask("routines")}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <Workflow className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Automate Routine
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Record and replay step-by-step browser & desktop automations.
                      </p>
                    </div>
                  </button>
                </div>
              ) : (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 text-left w-full pt-1">
                  <button
                    onClick={() => {
                      setDevWorkbenchTab("projects");
                      setIsDevWorkbenchOpen(true);
                    }}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-purple-800/60 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-purple-950/40 border border-purple-800/50 text-purple-400 group-hover:bg-purple-950/60 transition">
                          <Server className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Dev Servers
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Discover local projects, start managed servers, and stream logs.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => {
                      setDevWorkbenchTab("checkpoints");
                      setIsDevWorkbenchOpen(true);
                    }}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <GitCommit className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Code Checkpoints
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Record Git-aware reversible snapshots with 1-click safe rollback.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => {
                      setDevWorkbenchTab("doctor");
                      setIsDevWorkbenchOpen(true);
                    }}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <Stethoscope className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Build Doctor
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Parse compiler errors (Rust, TS, Vitest) and get instant repair patches.
                      </p>
                    </div>
                  </button>

                  <button
                    onClick={() => {
                      setDevWorkbenchTab("audit");
                      setIsDevWorkbenchOpen(true);
                    }}
                    className="p-4 rounded-xl bg-[#121212] hover:bg-[#181818] border border-[#242424] hover:border-neutral-700 text-left transition group shadow-sm flex flex-col justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2.5 mb-2">
                        <div className="p-2 rounded-lg bg-[#1a1a1a] border border-[#282828] text-neutral-300 group-hover:bg-[#222] transition">
                          <Eye className="w-4 h-4" />
                        </div>
                        <span className="font-semibold text-sm text-neutral-200 group-hover:text-white transition">
                          Visual Baselines
                        </span>
                      </div>
                      <p className="text-xs text-neutral-400 leading-relaxed">
                        Audit routes across viewports and compare against visual baselines.
                      </p>
                    </div>
                  </button>
                </div>
              )}
            </div>
          ) : (
            <AgentActivityFeed
              messages={messages || []}
              steps={liveSteps}
              isAgentRunning={isAgentRunning}
              agentName={resolveAgentDisplayName()}
              activeReasoning={
                activeReasoning
                  ? { text: activeReasoning.text, isStreaming: activeReasoning.isStreaming }
                  : null
              }
              pendingApprovalsCount={pendingApprovals.length}
              messagesEndRef={messagesEndRef}
              onCopyMessage={handleCopyMessage}
              onSpeakMessage={handleSpeak}
              copiedMessageId={copiedMessageId}
              speakingMessageId={speakingMessageId}
              className="max-w-3xl mx-auto"
            />
          )}
        </div>
      )}

        {/* Collapsible Terminal Drawer */}
        {showTerminal && (
          <div className="h-64 border-t border-neutral-800 bg-[#0a0a0a] flex flex-col">
            <div className="px-4 py-1.5 bg-[#141414] border-b border-neutral-800 flex items-center justify-between text-xs text-neutral-400">
              <span className="flex items-center gap-1.5 font-mono text-[11px]">
                <Terminal className="w-3.5 h-3.5 text-neutral-400" /> Host Terminal PTY (Interactive ANSI Stream)
              </span>
              <button
                onClick={() => setShowTerminal(false)}
                className="hover:text-neutral-100 transition-colors"
              >
                Close
              </button>
            </div>
            <div className="flex-1 p-2 overflow-hidden">
              <TerminalView />
            </div>
          </div>
        )}

        {/* Quick Everyday Task Prompt Pills */}
        {experienceMode === "everyday" && (
          <div className="px-6 py-2.5 border-t border-[#1f1f1f] bg-[#0c0c0c] flex flex-wrap items-center gap-2 text-xs">
            <span className="text-neutral-400 font-medium shrink-0">Try asking:</span>
            {[
              "Find the invoice I downloaded last month",
              "Organize my college documents into folders",
              "Explain this PDF in simple language and create revision notes",
              "Summarize my unread emails and draft replies",
              "Help me organize tomorrow's schedule",
              "Compare these products and explain which offers better value",
              "Every Friday, organize my new documents and show me what changed",
            ].map((p, idx) => (
              <button
                key={idx}
                type="button"
                onClick={() => {
                  setPrompt(p);
                  setEverydayTab("ask");
                }}
                className="px-2.5 py-1 rounded-lg bg-[#141414] hover:bg-[#1c1c1c] text-neutral-300 hover:text-white border border-[#242424] transition text-xs"
              >
                {p}
              </button>
            ))}
          </div>
        )}

        {/* Production composer: shared component for prompt input, attachment
            policy gate (Bug 19) and truthful stop affordance (Bug 1). */}
        <div className="p-4 border-t border-[#1f1f1f] bg-[#0a0a0a]">
          <TaskComposer
            prompt={prompt}
            onPromptChange={setPrompt}
            onSubmit={() => handleSubmit()}
            onAbort={handleAbort}
            isAgentRunning={isAgentRunning}
            activeBotName={activeBot.name}
            attachedFile={attachedFile ? { name: attachedFile.name, base64: attachedFile.base64, mimeType: attachedFile.mimeType } : null}
            onSelectFile={handleFileSelect}
            onRemoveFile={() => setAttachedFile(null)}
            isListening={isListening}
            onToggleVoice={toggleVoiceDictation}
            placeholder={
              isListening
                ? "Listening to your voice..."
                : `Ask @${activeBot.name} to automate a desktop task, run terminal commands, or teach a skill...`
            }
            attachButtonTitle="Attach a local text or image file"
            voiceButtonTitle="Voice Dictation (Speech to Text)"
            acceptAttribute={ACCEPTED_ATTACHMENT_TYPES}
          />
        </div>
      </div>

      {/* Modals & Drawers */}
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        selectedModel={selectedModel}
        onSelectModel={handleSelectModel}
        initialTab={settingsInitialTab}
      />

      <TeachTaskModal
        isOpen={isTeachTaskOpen}
        onClose={() => {
          setIsTeachTaskOpen(false);
          setTeachTaskHighlightId(undefined);
        }}
        initialTab={teachTaskInitialTab}
        highlightTaskId={teachTaskHighlightId}
      />

      <TeammateRosterModal
        isOpen={isTeammatesOpen}
        onClose={() => setIsTeammatesOpen(false)}
        selectedBotId={activeBot.id}
        onSelectBot={(bot) => {
          handleSelectAgent(bot);
          setIsTeammatesOpen(false);
        }}
      />

      <McpModal
        isOpen={isMcpOpen}
        onClose={() => setIsMcpOpen(false)}
      />

      <ConnectionsModal
        isOpen={isConnectionsOpen}
        onClose={() => setIsConnectionsOpen(false)}
      />

      <TimelineDrawer
        isOpen={isTimelineOpen}
        onClose={() => setIsTimelineOpen(false)}
      />

      <ArtifactsModal
        isOpen={isArtifactsOpen}
        onClose={() => setIsArtifactsOpen(false)}
        conversationId={activeConvId || undefined}
      />

      <FileOrganizerModal
        isOpen={isFileOrganizerOpen}
        onClose={() => setIsFileOrganizerOpen(false)}
      />

      <DocumentAssistantModal
        isOpen={isDocumentAssistantOpen}
        onClose={() => setIsDocumentAssistantOpen(false)}
      />

      <ScreenExplainerModal
        isOpen={isScreenExplainerOpen}
        onClose={() => setIsScreenExplainerOpen(false)}
        initialScreenshotBase64={screenPreview ? screenPreview.replace(/^data:image\/[a-z]+;base64,/, "") : null}
      />

      <DevWorkbenchModal
        isOpen={isDevWorkbenchOpen}
        onClose={() => setIsDevWorkbenchOpen(false)}
        initialTab={devWorkbenchTab}
      />

      <OnboardingModal
        isOpen={isOnboardingOpen}
        onClose={() => setIsOnboardingOpen(false)}
        onTryExample={(p) => {
          setPrompt(p);
          setEverydayTab("ask");
          setIsOnboardingOpen(false);
        }}
        onOpenConnections={() => {
          setIsConnectionsOpen(true);
          setIsOnboardingOpen(false);
        }}
      />

      <CommandPalette
        isOpen={isCommandPaletteOpen}
        onClose={() => setIsCommandPaletteOpen(false)}
        onNewSession={handleNewConversation}
        onToggleGhost={handleToggleGhostMode}
        onToggleTerminal={() => setShowTerminal((prev) => !prev)}
        onOpenTeammates={() => setIsTeammatesOpen(true)}
        onOpenTeachTask={handleOpenTeachTask}
        onOpenMcp={() => setIsMcpOpen(true)}
        onOpenConnections={() => setIsConnectionsOpen(true)}
        onOpenTimeline={() => setIsTimelineOpen(true)}
        onOpenArtifacts={() => setIsArtifactsOpen(true)}
        onOpenSettings={(tab) => {
          if (tab) setSettingsInitialTab(tab);
          setIsSettingsOpen(true);
        }}
        onSelectConversation={(id) => setActiveConvId(id)}
        onSelectBot={handleSelectAgent}
      />

      {/* Confirmation Dialog for Session Deletion */}
      {sessionToDelete && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 backdrop-blur-md p-4">
          <div
            className="w-full max-w-md bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl p-6 space-y-4"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-session-dialog-title"
          >
            <div className="flex items-center justify-between border-b border-[#262626] pb-3">
              <h3
                id="delete-session-dialog-title"
                className="text-base font-semibold text-neutral-100"
              >
                Delete session?
              </h3>
              <button
                type="button"
                onClick={handleCancelDeleteSession}
                disabled={isDeletingSession}
                className="p-1 text-neutral-400 hover:text-neutral-100 rounded-lg disabled:opacity-50 transition"
                aria-label="Cancel deletion"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="text-xs text-neutral-300 leading-relaxed space-y-2">
              <p>
                Are you sure you want to delete &ldquo;{sessionToDelete.title}&rdquo;? Its conversation messages will be permanently deleted. This action cannot be undone.
              </p>
            </div>

            {sessionDeleteError && (
              <div
                role="alert"
                className="p-3 bg-red-950/40 border border-red-500/40 rounded-xl text-xs text-red-300 flex items-center gap-2"
              >
                <AlertTriangle className="w-4 h-4 text-red-400 shrink-0" />
                <span>{sessionDeleteError}</span>
              </div>
            )}

            <div className="flex items-center justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={handleCancelDeleteSession}
                disabled={isDeletingSession}
                data-testid="cancel-delete-session-button"
                className="px-4 py-2 bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border border-neutral-700 rounded-xl text-xs font-medium transition disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConfirmDeleteSession}
                disabled={isDeletingSession}
                data-testid="confirm-delete-session-button"
                className="px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-xl text-xs font-semibold flex items-center gap-2 transition shadow disabled:opacity-50"
              >
                {isDeletingSession ? "Deleting..." : "Delete session"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
