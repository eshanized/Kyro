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
import { ToolCallCard } from "./components/ToolCallCard";
import { evaluateVerificationOutcome } from "./components/TaskProgress";
import { ThinkingIndicator } from "./components/ThinkingIndicator";
import { TerminalView } from "./components/TerminalView";
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
import { MarkdownRenderer } from "./components/MarkdownRenderer";
import { KyroLogo } from "./components/Logo";
import { AgentAvatar } from "./components/AgentAvatar";
import { NvidiaIcon } from "./components/BrandIcons";
import {
  Send,
  Plus,
  Settings,
  Terminal,
  GitCommit,
  Eye,
  Camera,
  Crop,
  StopCircle,
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
  Mic,
  MicOff,
  Volume2,
  VolumeX,
  Paperclip,
  FileCode,
  X,
  Plug,
  Copy,
  Check,
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
  CheckCircle2,
  ListOrdered,
  RefreshCw,
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

  // Structured Task Plan before consequential operations
  const [activePlan, setActivePlan] = useState<{
    id: string;
    runId: string;
    conversationId?: string;
    isBackendPlan?: boolean;
    goal: string;
    understanding: string;
    resources: string[];
    steps: { id: string; text: string; done?: boolean; status: "pending" | "running" | "completed" | "failed" | "unverified"; evidence?: string }[];
    status: "planning" | "approved" | "executing" | "completed" | "failed" | "cancelled" | "unverified";
    accomplished: string[];
    remaining: string[];
  } | null>(null);

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
  const fileInputRef = useRef<HTMLInputElement>(null);

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

      // Update active plan evidence based on verifiable tool outcomes
      setActivePlan((prev) => {
        if (!prev || prev.runId !== payload.run_id) return prev;
        const newAccomplished = [...prev.accomplished];
        const newRemaining = [...prev.remaining];
        let hasToolError = false;
        let hasVerification = false;
        let actionCompleted = false;

        for (const res of payload.tool_results || []) {
          const outcome = evaluateVerificationOutcome(res);
          if (res.is_error) {
            hasToolError = true;
            const errSummary = `Error in ${res.name}: ${res.output.slice(0, 80)}`;
            if (!newRemaining.includes(errSummary)) {
              newRemaining.push(errSummary);
            }
          } else if (outcome.failed) {
            hasToolError = true;
            const errSummary = outcome.reason || `Error in ${res.name}: ${res.output.slice(0, 80)}`;
            if (!newRemaining.includes(errSummary)) {
              newRemaining.push(errSummary);
            }
          } else {
            const accSummary = `Executed ${res.name}`;
            if (!newAccomplished.includes(accSummary)) {
              newAccomplished.push(accSummary);
            }
            if (outcome.verified) {
              hasVerification = true;
            } else {
              actionCompleted = true;
            }
          }
        }

        const updatedSteps = prev.steps.map((st) => {
          if (st.id === "step_inspect") {
            return {
              ...st,
              status: (hasToolError ? "failed" : "completed") as any,
              done: !hasToolError,
              evidence: payload.thought ? "Analyzed request context" : undefined,
            };
          }
          if (st.id === "step_execute") {
            if (hasToolError) {
              return { ...st, status: "failed" as any, done: false, evidence: "Execution error occurred" };
            }
            if (actionCompleted) {
              return { ...st, status: "completed" as any, done: true, evidence: "Actions executed" };
            }
            return { ...st, status: "running" as any };
          }
          if (st.id === "step_verify") {
            if (hasToolError) {
              return { ...st, status: "failed" as any, done: false, evidence: "Pre-empted by execution failure" };
            }
            if (hasVerification) {
              return { ...st, status: "completed" as any, done: true, evidence: "Verification check passed" };
            }
            return st;
          }
          return st;
        });

        return {
          ...prev,
          steps: updatedSteps,
          accomplished: newAccomplished,
          remaining: newRemaining.filter((r) => !newAccomplished.includes(r)),
        };
      });

      const currentConversation = activeConvIdRef.current;
      const belongsToActiveConversation =
        !payload.conversation_id || !currentConversation
          ? false
          : payload.conversation_id === currentConversation;

      if (belongsToActiveConversation) {
        setActiveRunId(payload.run_id);
      }
      // Clear transient reasoning when step executes tools or completes
      if ((payload.tool_calls && payload.tool_calls.length > 0) || payload.is_finished) {
        setActiveReasoning(null);
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
    // The live steps of the previous conversation are kept in the per-run
    // store, so switching back to it restores its timeline.  Only the
    // selection of the "current" run is reset here.
    setActiveRunId(null);
    setActiveReasoning(null);
    setActivePlan((prev) => (prev && prev.conversationId === activeConvId ? prev : null));
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

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (file.type.startsWith("image/")) {
      const reader = new FileReader();
      reader.onload = () => {
        const base64 = (reader.result as string).split(",")[1];
        setAttachedFile({
          name: file.name,
          content: `[Attached image file: ${file.name}]`,
          base64,
          mimeType: file.type || "image/png",
        });
      };
      reader.readAsDataURL(file);
    } else {
      const reader = new FileReader();
      reader.onload = () => {
        const text = reader.result as string;
        setAttachedFile({
          name: file.name,
          content: text,
        });
      };
      reader.readAsText(file);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if ((!prompt.trim() && !attachedFile) || isAgentRunning) return;

    let convId = activeConvId;
    if (!convId) {
      convId = "conv_" + crypto.randomUUID();
      const newConv = await invoke<Conversation>("create_conversation", {
        id: convId,
        title: prompt.slice(0, 30) || attachedFile?.name || "New Task",
        model: selectedModel,
      });
      setConversations([newConv, ...conversations]);
      setActiveConvId(convId);
    }

    let fullPromptText = prompt;
    let imageAttachment: { base64: string; mimeType: string; name: string } | null = null;
    if (attachedFile) {
      if (attachedFile.base64) {
        imageAttachment = {
          base64: attachedFile.base64,
          mimeType: attachedFile.mimeType || "image/png",
          name: attachedFile.name,
        };
        fullPromptText = `[Attachment: ${attachedFile.name}]\n${prompt}`;
      } else {
        fullPromptText = `[Attachment: ${attachedFile.name}]\n\`\`\`\n${attachedFile.content}\n\`\`\`\n\n${prompt}`;
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
    setActiveRunId(runId);
    setActiveReasoning(null);
    setIsAgentRunning(true);
    clearRunSteps(runId);

    const isConsequential = /(organize|move|clean|delete|email|draft|send|routine|schedule|compare|build|test|deploy|install)/i.test(fullPromptText);
    if (isConsequential) {
      const activeResources = [
        activeBot.name ? `@${activeBot.name} Workspace` : "Apex Orchestrator",
        "SQLite Audit Ledger",
      ];
      if (/email|draft|send/i.test(fullPromptText)) {
        activeResources.push("Configured Email Connector");
      }
      if (/file|folder|clean|organize|move/i.test(fullPromptText)) {
        activeResources.push("Authorized Local Directories");
      }

      setActivePlan({
        id: "plan_" + Date.now(),
        runId,
        conversationId: convId,
        goal: fullPromptText,
        understanding: `Kyro received request: "${fullPromptText.trim()}". Proposed execution plan:`,
        resources: activeResources,
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
      });
    }

    const priorHistory = messages.map((m) => ({
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

    const history = priorHistory.concat([
      {
        role: userMsg.role,
        content: lastMessageContent as any,
        tool_calls: null,
        tool_call_id: null,
      },
    ]);

    try {
      // The profile prompt is resolved engine-side (Stage C); the client can
      // only choose an agent id, never supply instructions.
      const finalReply = await invoke<string>("run_agent_turn", {
        runId,
        conversationId: convId,
        model: selectedModel,
        botId: activeBot.id,
        history,
        systemPrompt: null,
      });

      const assistantMsg: Message = {
        id: "msg_" + crypto.randomUUID(),
        conversation_id: convId,
        role: "assistant",
        content: finalReply || "Task completed.",
        created_at: Date.now(),
      };

      setMessages((prev) => [...prev, assistantMsg]);
      await invoke("add_message", { msg: assistantMsg }).catch(console.error);
      sendNotification("Kyro Task Completed", finalReply.slice(0, 80));
      setActivePlan((prev) => {
        if (!prev) return null;
        const verificationStep = prev.steps.find((s) => s.id === "step_verify");
        const isVerified = verificationStep?.status === "completed";
        const hasFailedStep = prev.steps.some((s) => s.status === "failed");

        const finalStatus = hasFailedStep
          ? "failed"
          : isVerified
          ? "completed"
          : prev.accomplished.length > 0
          ? "unverified"
          : "unverified";

        const remainingNotes = [...prev.remaining];
        if (!isVerified && !hasFailedStep && !remainingNotes.some((n) => n.toLowerCase().includes("verification"))) {
          remainingNotes.push("Explicit verification check not executed");
        }

        return {
          ...prev,
          status: finalStatus,
          steps: prev.steps.map((s) => {
            if (s.id === "step_verify" && s.status !== "completed" && !hasFailedStep) {
              return { ...s, status: "unverified" as any, done: false, evidence: "Unverified" };
            }
            if (s.status === "running") {
              return { ...s, status: (hasFailedStep ? "failed" : "completed") as any, done: !hasFailedStep };
            }
            return s;
          }),
          remaining: remainingNotes,
        };
      });
    } catch (err: any) {
      const errorMsg: Message = {
        id: "msg_" + crypto.randomUUID(),
        conversation_id: convId,
        role: "assistant",
        content: "Error running turn: " + err,
        created_at: Date.now(),
      };
      setMessages((prev) => [...prev, errorMsg]);
      sendNotification("Kyro Task Failed", String(err));
      setActivePlan((prev) =>
        prev
          ? {
              ...prev,
              status: "failed",
              steps: prev.steps.map((s) =>
                s.status === "running" ? { ...s, status: "failed" as any, done: false } : s
              ),
              remaining: [`Encountered execution failure: ${err}`],
            }
          : null
      );
    } finally {
      setIsAgentRunning(false);
      setActiveRunId(null);
      setActiveReasoning(null);
    }
  };

  const handleAbort = async () => {
    if (activeRunId) {
      await invoke("cancel_agent_run", { runId: activeRunId }).catch(console.error);
    }
    await invoke("abort_computer_use").catch(console.error);
    setActivePlan((prev) =>
      prev
        ? {
            ...prev,
            status: "cancelled",
            steps: prev.steps.map((s) =>
              s.status === "running" ? { ...s, status: "failed" as any, done: false, evidence: "Cancelled" } : s
            ),
            remaining: ["Execution cancelled by user request"],
          }
        : null
    );
    setIsAgentRunning(false);
    setActiveRunId(null);
    setActiveReasoning(null);
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
          <div className="px-6 py-3 bg-[#111111] border-b border-neutral-800 space-y-2">
            {pendingApprovals.map((req) => (
              <div
                key={req.id}
                className="flex items-center justify-between bg-[#161616] p-3 rounded-xl border border-neutral-700 text-xs"
              >
                <div className="flex items-center gap-3 min-w-0 pr-4">
                  <ShieldAlert className="w-5 h-5 text-neutral-300 flex-shrink-0 animate-pulse" />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-neutral-100">Tool Approval Required:</span>
                      <span
                        className={`px-1.5 py-0.5 rounded font-mono text-[10px] uppercase font-bold ${
                          req.risk_level === "destructive"
                            ? "bg-red-950/60 text-red-300 border border-red-800/60"
                            : req.risk_level === "moderate"
                            ? "bg-amber-950/60 text-amber-300 border border-amber-800/60"
                            : "bg-neutral-800 text-neutral-300 border border-neutral-700"
                        }`}
                      >
                        {req.risk_level} Risk
                      </span>
                      <span className="font-mono text-neutral-200 text-[11px]">
                        {req.tool_name}
                      </span>
                    </div>
                    <p className="text-neutral-300 text-[11px] truncate mt-0.5">{req.summary}</p>
                    {req.reason && (
                      <p className="text-neutral-400 text-[10px] italic mt-0.5">
                        Policy: {req.reason}
                      </p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                  <button
                    onClick={() => handleResolveApproval(req.id, "approved")}
                    className="px-3 py-1.5 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 font-medium text-xs flex items-center gap-1 shadow-sm transition"
                  >
                    <CheckCircle className="w-3.5 h-3.5" /> Approve
                  </button>
                  <button
                    onClick={() => handleResolveApproval(req.id, "denied")}
                    className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 font-medium text-xs flex items-center gap-1 transition"
                  >
                    <XCircle className="w-3.5 h-3.5" /> Deny
                  </button>
                </div>
              </div>
            ))}
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
            {/* Structured Task Plan Card */}
            {activePlan && (
              <div className="max-w-2xl mx-auto my-2 p-4 rounded-xl bg-[#141414] border border-neutral-800 space-y-3 shadow-lg">
                <div className="flex items-center justify-between pb-2 border-b border-neutral-800">
                  <div className="flex items-center gap-2">
                    <ListOrdered className="w-4 h-4 text-neutral-300" />
                    <span className="text-xs font-semibold text-neutral-200">Execution Plan</span>
                    <span className="text-[10px] font-mono text-neutral-400">({activePlan.status})</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`text-[10px] px-2 py-0.5 rounded-full font-mono uppercase font-bold ${
                        activePlan.status === "completed"
                          ? "bg-green-950/60 text-green-300 border border-green-800/60"
                          : activePlan.status === "failed"
                          ? "bg-red-950/60 text-red-300 border border-red-800/60"
                          : activePlan.status === "unverified"
                          ? "bg-yellow-950/60 text-yellow-300 border border-yellow-800/60"
                          : activePlan.status === "executing"
                          ? "bg-purple-950/60 text-purple-300 border border-purple-800/60 animate-pulse"
                          : "bg-neutral-800 text-neutral-300 border border-neutral-700"
                      }`}
                    >
                      {activePlan.status}
                    </span>
                    <button
                      type="button"
                      onClick={() => setActivePlan(null)}
                      className="text-neutral-500 hover:text-neutral-300 text-xs p-1"
                      title="Dismiss plan"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                <div className="text-xs space-y-2">
                  <div>
                    <span className="text-neutral-400">Understanding: </span>
                    <span className="text-neutral-200 font-medium">{activePlan.understanding}</span>
                  </div>

                  <div className="flex flex-wrap items-center gap-1.5 pt-1">
                    <span className="text-neutral-400 text-[11px]">Resources: </span>
                    {activePlan.resources.map((res, i) => (
                      <span
                        key={i}
                        className="px-2 py-0.5 bg-[#000000] border border-neutral-800 text-[10px] text-neutral-300 rounded font-mono"
                      >
                        {res}
                      </span>
                    ))}
                  </div>

                  <div className="space-y-1.5 pt-1">
                    <span className="text-neutral-400 block font-medium text-[11px]">Planned Steps:</span>
                    {activePlan.steps.map((st, i) => {
                      const isComplete = st.status === "completed" || (!st.status && st.done);
                      const isFailed = st.status === "failed";
                      const isRunning = st.status === "running";
                      const isUnverified = st.status === "unverified";
                      return (
                        <div key={i} className="flex items-center gap-2 text-neutral-300">
                          {isComplete ? (
                            <CheckCircle2 className="w-3.5 h-3.5 text-green-400 shrink-0" />
                          ) : isFailed ? (
                            <X className="w-3.5 h-3.5 text-red-400 shrink-0" />
                          ) : isRunning ? (
                            <RefreshCw className="w-3.5 h-3.5 text-purple-400 animate-spin shrink-0" />
                          ) : isUnverified ? (
                            <AlertTriangle className="w-3.5 h-3.5 text-yellow-400 shrink-0" />
                          ) : (
                            <span className="w-3.5 h-3.5 rounded-full border border-neutral-600 flex items-center justify-center text-[9px] font-mono text-neutral-400 shrink-0">
                              {i + 1}
                            </span>
                          )}
                          <span className={isComplete ? "line-through text-neutral-500" : ""}>{st.text}</span>
                          {st.evidence && (
                            <span className="text-[10px] text-neutral-500 font-mono">({st.evidence})</span>
                          )}
                        </div>
                      );
                    })}
                  </div>

                  {activePlan.accomplished.length > 0 && (
                    <div className="pt-2 border-t border-neutral-800/80">
                      <span className="text-[11px] font-semibold text-neutral-300 block mb-1">
                        Accomplished:
                      </span>
                      <ul className="list-disc list-inside text-[11px] text-neutral-400 space-y-0.5">
                        {activePlan.accomplished.map((item, i) => (
                          <li key={i}>{item}</li>
                        ))}
                      </ul>
                    </div>
                  )}

                  {activePlan.remaining.length > 0 && (
                    <div className="pt-1">
                      <span className="text-[11px] font-semibold text-neutral-400 block mb-1">
                        Remaining / Unfinished:
                      </span>
                      <ul className="list-disc list-inside text-[11px] text-neutral-500 space-y-0.5">
                        {activePlan.remaining.map((item, i) => (
                          <li key={i}>{item}</li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              </div>
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
            <>
              {(messages || []).map((m) => (
                <div
                  key={m.id}
                  className={`flex flex-col ${
                    m.role === "user" ? "items-end" : "items-start"
                  }`}
                >
                  <div
                    className={`max-w-2xl px-4 py-3 rounded-2xl text-sm leading-relaxed ${
                      m.role === "user"
                        ? "bg-[#1f1f1f] text-[#ededed] border border-[#2e2e2e] rounded-br-none shadow-sm"
                        : "bg-[#121212] text-[#ededed] border border-[#262626] rounded-bl-none shadow-sm"
                    }`}
                  >
                    {m.role === "assistant" ? (
                      <MarkdownRenderer content={m.content} />
                    ) : (
                      <div className="whitespace-pre-wrap">{m.content}</div>
                    )}

                    {/* Timeline tool calls for assistant messages */}
                    {(() => {
                      if (m.role !== "assistant" || !m.tool_calls_json) return null;
                      try {
                        const tcs = JSON.parse(m.tool_calls_json);
                        if (!Array.isArray(tcs) || tcs.length === 0) return null;
                        let obs: any[] = [];
                        if (m.observations_json) {
                          try {
                            obs = JSON.parse(m.observations_json);
                          } catch {}
                        }
                        return (
                          <div className="space-y-2 mt-3 pt-2 border-t border-[#1f1f1f] w-full">
                            {tcs.map((tc: any, idx: number) => {
                              const res = Array.isArray(obs)
                                ? obs.find((r: any) => r.tool_call_id === tc.id)
                                : undefined;
                              return (
                                <ToolCallCard
                                  key={`msg-tc-${tc.id || idx}`}
                                  toolCall={tc}
                                  result={res}
                                  defaultExpanded={false}
                                />
                              );
                            })}
                          </div>
                        );
                      } catch {
                        return null;
                      }
                    })()}

                    {/* Actions for assistant messages */}
                    {m.role === "assistant" && (
                      <div className="flex items-center justify-between pt-2 mt-2 border-t border-neutral-800 text-xs">
                        <span className="text-[10px] text-neutral-400 font-mono">
                          {new Date(m.created_at).toLocaleTimeString()}
                        </span>
                        <div className="flex items-center gap-3">
                          <button
                            onClick={() => handleCopyMessage(m.id, m.content)}
                            className="text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
                            title="Copy response"
                          >
                            {copiedMessageId === m.id ? (
                              <>
                                <Check className="w-3.5 h-3.5 text-neutral-200" />
                                <span className="text-[10px]">Copied</span>
                              </>
                            ) : (
                              <>
                                <Copy className="w-3.5 h-3.5" />
                                <span className="text-[10px]">Copy</span>
                              </>
                            )}
                          </button>
                          <button
                            onClick={() => handleSpeak(m.id, m.content)}
                            className="text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
                            title="Read message aloud (Local TTS)"
                          >
                            {speakingMessageId === m.id ? (
                              <>
                                <VolumeX className="w-3.5 h-3.5 text-amber-400" />
                                <span className="text-[10px]">Stop</span>
                              </>
                            ) : (
                              <>
                                <Volume2 className="w-3.5 h-3.5" />
                                <span className="text-[10px]">Speak</span>
                              </>
                            )}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              ))}

              {/* Real-time Tool Action Cards & Thoughts */}
              {liveSteps.map((step) => (
                <React.Fragment key={`${step.run_id}-${step.step}`}>
                  {step.thought && (
                    <div className="flex flex-col items-start my-2 max-w-2xl">
                      <div className="px-4 py-3 rounded-2xl rounded-bl-none bg-[#121212] text-[#ededed] border border-[#262626] shadow-sm text-sm">
                        {step.is_child_run && (
                          <div className="mb-1.5 text-[10px] uppercase tracking-wide text-neutral-500 font-mono">
                            Delegated agent
                            {step.agent_id ? ` · ${step.agent_id}` : ""}
                          </div>
                        )}
                        <MarkdownRenderer content={step.thought} />
                      </div>
                    </div>
                  )}
                  {(step.tool_calls || []).map((tc, idx) => {
                    const res = (step.tool_results || []).find((r) => r.tool_call_id === tc.id);
                    return (
                      <ToolCallCard
                        key={`${step.step}-${tc.id || idx}`}
                        toolCall={tc}
                        result={res}
                        defaultExpanded={!res}
                      />
                    );
                  })}
                </React.Fragment>
              ))}

              {isAgentRunning && (
                <div className="py-1">
                  <ThinkingIndicator
                    agentName={resolveAgentDisplayName()}
                    reasoningText={activeReasoning?.text}
                    isStreaming={activeReasoning?.isStreaming ?? true}
                    statusText="Waiting for model response…"
                  />
                </div>
              )}

              <div ref={messagesEndRef} />
            </>
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

        {/* Attached File Preview Chip */}
        {attachedFile && (
          <div className="px-4 py-1.5 bg-[#141414] border-t border-neutral-800 flex items-center justify-between text-xs text-neutral-200">
            <div className="flex items-center gap-2">
              <FileCode className="w-4 h-4 text-neutral-400" />
              <span>Attached: <strong>{attachedFile.name}</strong></span>
            </div>
            <button
              onClick={() => setAttachedFile(null)}
              className="text-neutral-400 hover:text-white transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
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

        {/* Input Form with Voice Dictation & Attachments */}
        <div className="p-4 border-t border-[#1f1f1f] bg-[#0a0a0a]">
          <form onSubmit={handleSubmit} className="flex items-end gap-2">
            {/* Hidden File Picker */}
            <input
              type="file"
              ref={fileInputRef}
              onChange={handleFileSelect}
              className="hidden"
            />

            {/* Attachment Button */}
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              className="p-3 rounded-xl bg-[#000000] hover:bg-neutral-900 border border-[#242424] text-neutral-400 hover:text-neutral-200 transition shrink-0"
              title="Attach a local text or image file"
            >
              <Paperclip className="w-4 h-4" />
            </button>

            {/* Voice Dictation Button */}
            <button
              type="button"
              onClick={toggleVoiceDictation}
              className={`p-3 rounded-xl border transition shrink-0 ${
                isListening
                  ? "bg-red-950/40 text-red-400 border-red-700/60 animate-pulse"
                  : "bg-[#000000] hover:bg-neutral-900 border-[#242424] text-neutral-400 hover:text-neutral-200"
              }`}
              title={isListening ? "Listening... click to stop" : "Voice Dictation (Speech to Text)"}
            >
              {isListening ? <Mic className="w-4 h-4 text-red-400 animate-bounce" /> : <MicOff className="w-4 h-4" />}
            </button>

            <textarea
              rows={2}
              placeholder={
                isListening
                  ? "Listening to your voice..."
                  : `Ask @${activeBot.name} to automate a desktop task, run terminal commands, or teach a skill...`
              }
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSubmit(e as any);
                }
              }}
              disabled={isAgentRunning}
              className="flex-1 px-4 py-2.5 bg-[#000000] border border-[#242424] rounded-xl text-sm text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-500 resize-none transition"
            />

            {isAgentRunning ? (
              <button
                type="button"
                onClick={handleAbort}
                className="px-5 py-3 rounded-xl bg-red-600 hover:bg-red-500 text-white text-xs font-semibold uppercase tracking-wider flex items-center gap-2 shadow-sm transition shrink-0"
              >
                <StopCircle className="w-4 h-4 fill-current" /> Stop
              </button>
            ) : (
              <button
                type="submit"
                disabled={!prompt.trim() && !attachedFile}
                className="px-5 py-3 rounded-xl bg-neutral-100 hover:bg-white disabled:opacity-40 text-neutral-950 text-xs font-semibold flex items-center gap-2 shadow-sm transition shrink-0"
              >
                <Send className="w-4 h-4" /> Send
              </button>
            )}
          </form>
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
