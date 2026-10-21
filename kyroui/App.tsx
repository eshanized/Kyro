import React, { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Conversation,
  Message,
  AgentStepEvent,
  BotRecord,
  TakeoverStatus,
  ApprovalRequest,
  GroundedScreenResult,
} from "./types/agent";
import { ToolCallCard } from "./components/ToolCallCard";
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
} from "lucide-react";

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

  // Multi-Bot & Governance
  const [activeBot, setActiveBot] = useState<BotRecord>({
    id: "apex-orchestrator",
    name: "Apex",
    avatar: "zap",
    role: "General Workspace & Strategic Orchestrator",
    system_prompt: "",
    is_default: true,
  });

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

    // Request notification permission if supported
    if ("Notification" in window && Notification.permission === "default") {
      Notification.requestPermission().catch(() => {});
    }

    // Initialize Web Speech API for zero-cloud voice dictation
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

      const currentConversation = activeConvIdRef.current;
      const belongsToActiveConversation =
        !payload.conversation_id || !currentConversation
          ? false
          : payload.conversation_id === currentConversation;

      if (belongsToActiveConversation) {
        setActiveRunId(payload.run_id);
      }
    });

    // Governance & Takeover polling
    const pollInterval = setInterval(async () => {
      try {
        const [tStatus, approvals] = await Promise.all([
          invoke<TakeoverStatus>("get_takeover_status"),
          invoke<ApprovalRequest[]>("list_pending_approvals"),
        ]);
        setTakeover(tStatus);
        setPendingApprovals(approvals.filter((a) => a.status === "pending"));
      } catch (e) {
        // ignore
      }
    }, 1000);

    return () => {
      window.removeEventListener("keydown", handleGlobalKeyDown);
      unlistenPromise.then((fn) => fn());
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
      const bots = await invoke<BotRecord[]>("list_bots");
      if (Array.isArray(bots) && bots.length > 0) {
        setActiveBot(bots[0]);
      }
    } catch (e) {
      console.error(e);
    }
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
        setMessages(msgs);
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
    setIsAgentRunning(true);
    clearRunSteps(runId);

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
      const systemPrompt = activeBot.system_prompt
        ? `${activeBot.system_prompt}\nYou are responding as @${activeBot.name}.`
        : null;

      const finalReply = await invoke<string>("run_agent_turn", {
        runId,
        conversationId: convId,
        model: selectedModel,
        botId: activeBot.id,
        history,
        systemPrompt,
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
    } finally {
      setIsAgentRunning(false);
      setActiveRunId(null);
    }
  };

  const handleAbort = async () => {
    if (activeRunId) {
      await invoke("cancel_agent_run", { runId: activeRunId }).catch(console.error);
    }
    await invoke("abort_computer_use").catch(console.error);
    setIsAgentRunning(false);
    setActiveRunId(null);
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

  return (
    <div className="flex h-screen w-screen bg-[#0a0a0a] text-neutral-100 overflow-hidden font-sans select-none">
      {/* Sidebar */}
      <div className="w-64 bg-[#0a0a0a] border-r border-neutral-800 flex flex-col justify-between">
        <div className="p-4 flex flex-col h-full">
          {/* Logo & HUD trigger */}
          <div className="flex items-center justify-between pb-4 border-b border-neutral-800">
            <div className="flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-lg bg-[#141414] border border-neutral-800/80 flex items-center justify-center p-1 shadow-sm overflow-hidden">
                <KyroLogo className="w-full h-full" />
              </div>
              <div>
                <h1 className="font-semibold text-sm tracking-wide text-neutral-100">KYRO</h1>
                <p className="text-[10px] text-neutral-400 font-mono">Autonomous OS</p>
              </div>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={() => setIsCommandPaletteOpen(true)}
                className="p-1.5 text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800 rounded-lg transition"
                title="Quick Actions & Search (Ctrl+K)"
              >
                <Command className="w-4 h-4" />
              </button>
              <button
                onClick={() => invoke("toggle_hud_window")}
                className="p-1.5 text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800 rounded-lg transition"
                title="Toggle Floating HUD Widget"
              >
                <ExternalLink className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Experience Mode Switcher */}
          <div className="my-2 p-1 bg-[#141414] border border-neutral-800 rounded-xl grid grid-cols-2 gap-1 text-xs">
            <button
              onClick={() => handleSetExperienceMode("everyday")}
              className={`py-1.5 px-2 rounded-lg font-medium flex items-center justify-center gap-1.5 transition ${
                experienceMode === "everyday"
                  ? "bg-neutral-800 text-neutral-100 shadow-sm"
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
            className="mb-3 w-full py-2 px-3 rounded-lg bg-neutral-100 hover:bg-white text-neutral-950 text-xs font-medium flex items-center justify-center gap-1.5 transition shadow-sm"
          >
            <Plus className="w-4 h-4" /> New Session
          </button>

          {/* Active Teammate & Model Badge */}
          <div className="mb-3 p-3 bg-[#111111] border border-neutral-800 rounded-xl space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[10px] uppercase font-mono tracking-wider text-neutral-400">
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
                className="w-8 h-8 rounded-lg bg-black/40 border border-neutral-800/60 flex items-center justify-center text-neutral-200"
                iconClassName="w-4 h-4"
              />
              <div className="min-w-0">
                <div className="text-xs font-semibold text-neutral-100">@{activeBot.name}</div>
                <div className="text-[10px] text-neutral-400 truncate">{activeBot.role}</div>
              </div>
            </div>

            <div className="pt-2 border-t border-neutral-800 flex items-center justify-between text-[10px] font-mono text-neutral-400">
              <span className="flex items-center gap-1.5">
                <NvidiaIcon className="w-3.5 h-3.5 text-[#76B900]" /> Nemotron-3 Ultra
              </span>
              <span className="text-[9px] px-1 py-0.2 bg-neutral-900 rounded border border-neutral-800 text-neutral-300">
                550B
              </span>
            </div>
          </div>

          {/* Navigation Tools */}
          <div className="space-y-1 mb-3">
            {experienceMode === "everyday" ? (
              <>
                <button
                  onClick={() => setIsScreenExplainerOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-amber-200 bg-amber-950/20 hover:bg-amber-950/40 border border-amber-800/40 flex items-center gap-2 transition"
                >
                  <Camera className="w-4 h-4 text-amber-400" /> Explain Screen
                </button>
                <button
                  onClick={() => setIsFileOrganizerOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-neutral-800/60 flex items-center gap-2 transition"
                >
                  <FolderTree className="w-4 h-4 text-neutral-400" /> Organize Files
                </button>
                <button
                  onClick={() => setIsDocumentAssistantOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-neutral-800/60 flex items-center gap-2 transition"
                >
                  <FileText className="w-4 h-4 text-neutral-400" /> Document Assistant
                </button>
                <button
                  onClick={() => handleOpenTeachTask("routines")}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Workflow className="w-4 h-4 text-neutral-400" /> Automations & Routines
                </button>
                <button
                  onClick={() => setIsArtifactsOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Layers className="w-4 h-4 text-neutral-400" /> Artifacts Registry
                </button>
                <button
                  onClick={() => setIsConnectionsOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Plug className="w-4 h-4 text-neutral-400" /> App Connections
                </button>
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
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-neutral-800/80 flex items-center gap-2 transition"
                >
                  <Plug className="w-4 h-4 text-neutral-400" /> App Connections
                </button>
                <button
                  onClick={() => setIsArtifactsOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Layers className="w-4 h-4 text-neutral-400" /> Artifacts Registry
                </button>
                <button
                  onClick={() => handleOpenTeachTask("skills")}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Workflow className="w-4 h-4 text-neutral-400" /> Teach-a-Task & Routines
                </button>
                <button
                  onClick={() => setIsTeammatesOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Users className="w-4 h-4 text-neutral-400" /> Teammate Roster
                </button>
                <button
                  onClick={() => setIsMcpOpen(true)}
                  className="w-full px-3 py-2 rounded-lg text-xs font-medium text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100 border border-transparent flex items-center gap-2 transition"
                >
                  <Server className="w-4 h-4 text-neutral-400" /> MCP Tool Servers
                </button>
              </>
            )}
          </div>

          {/* Conversations list */}
          <div className="flex-1 overflow-y-auto space-y-1 pr-1 border-t border-neutral-800 pt-3">
            <div className="text-[10px] uppercase font-mono tracking-wider text-neutral-500 mb-1.5 px-2">
              Recent Sessions
            </div>
            {conversations.map((c) => (
              <div
                key={c.id}
                onClick={() => setActiveConvId(c.id)}
                className={`group px-3 py-2 rounded-lg text-xs cursor-pointer truncate transition flex items-center justify-between ${
                  activeConvId === c.id
                    ? "bg-neutral-800 text-neutral-100 border border-neutral-700 font-medium"
                    : "text-neutral-400 hover:bg-neutral-900 hover:text-neutral-200 border border-transparent"
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
        <div className="p-3 border-t border-neutral-800 bg-[#0a0a0a]">
          <button
            onClick={() => {
              setSettingsInitialTab("model");
              setIsSettingsOpen(true);
            }}
            className="w-full px-3 py-2 rounded-lg text-xs text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800 transition flex items-center justify-between"
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
        <div className="h-14 px-6 border-b border-neutral-800 flex items-center justify-between bg-[#0a0a0a]">
          <div className="flex items-center gap-3">
            <AgentAvatar
              avatar={activeBot.avatar}
              name={activeBot.name}
              className="w-8 h-8 rounded-lg bg-neutral-900 border border-neutral-800 flex items-center justify-center text-neutral-200"
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
                  onClick={() => setIsScreenExplainerOpen(true)}
                  className="px-3 py-1.5 bg-amber-950/30 hover:bg-amber-950/50 border border-amber-800/50 rounded-lg text-xs text-amber-200 flex items-center gap-1.5 transition font-medium"
                  title="Explain My Screen"
                >
                  <Camera className="w-3.5 h-3.5 text-amber-400" /> Explain Screen
                </button>
                <button
                  onClick={() => setIsFileOrganizerOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Organize Files"
                >
                  <FolderTree className="w-3.5 h-3.5 text-neutral-400" /> Organize
                </button>
                <button
                  onClick={() => setIsDocumentAssistantOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Local Document Assistant"
                >
                  <FileText className="w-3.5 h-3.5 text-neutral-400" /> Documents
                </button>
                <button
                  onClick={() => setIsTimelineOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Inspect step-by-step history"
                >
                  <History className="w-3.5 h-3.5 text-neutral-400" /> Timeline
                </button>
                <button
                  onClick={handleCaptureScreen}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
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
                      : "bg-[#121212] hover:bg-[#1a1a1a] text-neutral-400 border-neutral-800"
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
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Detect UI elements and render numbered markers [1], [2], [3] for zero-miss clicks"
                >
                  <Target className="w-3.5 h-3.5 text-neutral-400" /> Ground
                </button>
                <button
                  onClick={() => setIsTimelineOpen(true)}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Inspect step-by-step before/after diffs with click target rings"
                >
                  <History className="w-3.5 h-3.5 text-neutral-400" /> Timeline
                </button>
                <button
                  onClick={handleCaptureScreen}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Capture display snapshot"
                >
                  <Camera className="w-3.5 h-3.5 text-neutral-400" /> Screen
                </button>
                <button
                  onClick={() => invoke("toggle_overlay_window", { show: true })}
                  className="px-3 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-300 flex items-center gap-1.5 transition"
                  title="Snip region of screen"
                >
                  <Crop className="w-3.5 h-3.5 text-neutral-400" /> Snip
                </button>
                <button
                  onClick={() => setShowTerminal(!showTerminal)}
                  className={`px-3 py-1.5 border rounded-lg text-xs flex items-center gap-1.5 transition ${
                    showTerminal
                      ? "bg-neutral-800 text-neutral-100 border-neutral-700"
                      : "bg-[#121212] hover:bg-[#1a1a1a] text-neutral-300 border-neutral-800"
                  }`}
                >
                  <Terminal className="w-3.5 h-3.5 text-neutral-400" /> PTY
                </button>
              </>
            )}

            {/* Quick Actions / Palette */}
            <button
              onClick={() => setIsCommandPaletteOpen(true)}
              className="px-2.5 py-1.5 bg-[#121212] hover:bg-[#1a1a1a] border border-neutral-800 rounded-lg text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1 transition"
              title="Global Command Palette (Ctrl+K)"
            >
              <Command className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Human Takeover Alert Banner */}
        {takeover.is_active && (
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
                  {groundedResult.marks.length} grounded element
                  {groundedResult.marks.length === 1 ? "" : "s"} detected (
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

        {/* Message Thread & Live Steps */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {messages.length === 0 && liveSteps.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center text-center max-w-lg mx-auto space-y-4">
              <div className="relative">
                <div className="w-16 h-16 rounded-2xl bg-[#141414] border border-neutral-800 flex items-center justify-center p-3 shadow-lg">
                  <KyroLogo className="w-full h-full" />
                </div>
                <div className="absolute -bottom-1 -right-1 p-1 rounded-full bg-[#1c1c1c] border border-neutral-700/80 shadow flex items-center justify-center">
                  <AgentAvatar
                    avatar={activeBot.avatar}
                    name={activeBot.name}
                    className="w-4 h-4 rounded-full bg-transparent flex items-center justify-center text-neutral-200"
                    iconClassName="w-3 h-3"
                  />
                </div>
              </div>

              {experienceMode === "everyday" ? (
                <>
                  <div>
                    <h2 className="text-lg font-semibold text-neutral-100">
                      Kyro Everyday Assistant
                    </h2>
                    <p className="text-xs text-neutral-400 leading-relaxed mt-1">
                      Understand screens, safely organize downloads, analyze documents, and automate repetitive tasks with zero developer jargon.
                    </p>
                  </div>
                  <div className="grid grid-cols-2 gap-2.5 text-left w-full pt-1">
                    <button
                      onClick={() => setIsScreenExplainerOpen(true)}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-amber-800/40 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <Camera className="w-4 h-4 text-amber-400" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Explain Screen
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Diagnose visual errors, find buttons, and get grounded explanations.
                      </p>
                    </button>

                    <button
                      onClick={() => setIsFileOrganizerOpen(true)}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <FolderTree className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Organize Files
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Categorize cluttered folders, find SHA-256 duplicates with safe undo.
                      </p>
                    </button>

                    <button
                      onClick={() => setIsDocumentAssistantOpen(true)}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <FileText className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Document Assistant
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Parse text and PDFs locally. Extract key dates, actions & compare docs.
                      </p>
                    </button>

                    <button
                      onClick={() => handleOpenTeachTask("routines")}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <Workflow className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Automate Routine
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Record and replay step-by-step browser & desktop automations.
                      </p>
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <div>
                    <h2 className="text-lg font-semibold text-neutral-100 flex items-center justify-center gap-2">
                      <span>Kyro Autonomous Workspace</span>
                      <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-purple-900/40 text-purple-300 border border-purple-800/60 font-semibold">
                        DEV
                      </span>
                    </h2>
                    <p className="text-xs text-neutral-400 leading-relaxed mt-1">
                      Local-first developer agent with managed dev-servers, reversible checkpoints, Build Doctor diagnostics, and responsive visual regression.
                    </p>
                  </div>
                  <div className="grid grid-cols-2 gap-2.5 text-left w-full pt-1">
                    <button
                      onClick={() => {
                        setDevWorkbenchTab("projects");
                        setIsDevWorkbenchOpen(true);
                      }}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-purple-800/40 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <Server className="w-4 h-4 text-purple-400" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Dev Servers
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Discover local projects, start managed servers, and stream logs.
                      </p>
                    </button>

                    <button
                      onClick={() => {
                        setDevWorkbenchTab("checkpoints");
                        setIsDevWorkbenchOpen(true);
                      }}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <GitCommit className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Code Checkpoints
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Record Git-aware reversible snapshots with 1-click safe rollback.
                      </p>
                    </button>

                    <button
                      onClick={() => {
                        setDevWorkbenchTab("doctor");
                        setIsDevWorkbenchOpen(true);
                      }}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <Stethoscope className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Build Doctor
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Parse compiler errors (Rust, TS, Vitest) and get instant repair patches.
                      </p>
                    </button>

                    <button
                      onClick={() => {
                        setDevWorkbenchTab("audit");
                        setIsDevWorkbenchOpen(true);
                      }}
                      className="p-3 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm"
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <Eye className="w-4 h-4 text-neutral-300" />
                        <span className="font-semibold text-xs text-neutral-200 group-hover:text-white">
                          Visual Baselines
                        </span>
                      </div>
                      <p className="text-[11px] text-neutral-400 leading-snug">
                        Audit routes across viewports and compare against visual baselines.
                      </p>
                    </button>
                  </div>
                </>
              )}
            </div>
          ) : (
            <>
              {messages.map((m) => (
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
                  {step.tool_calls.map((tc, idx) => (
                    <ToolCallCard
                      key={`${step.step}-${tc.id || idx}`}
                      toolCall={tc}
                      result={step.tool_results.find((r) => r.tool_call_id === tc.id)}
                    />
                  ))}
                </React.Fragment>
              ))}

              {isAgentRunning && (
                <div className="flex items-center gap-2 text-xs text-neutral-400 font-mono py-1">
                  <span className="w-2 h-2 rounded-full bg-neutral-400 animate-pulse" />
                  <span>Kyro is processing...</span>
                </div>
              )}

              <div ref={messagesEndRef} />
            </>
          )}
        </div>

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

        {/* Input Form with Voice Dictation & Attachments */}
        <div className="p-4 border-t border-neutral-800 bg-[#0a0a0a]">
          <form onSubmit={handleSubmit} className="flex items-center gap-2">
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
              className="p-3 rounded-xl bg-[#000000] hover:bg-neutral-900 border border-neutral-800 text-neutral-400 hover:text-neutral-200 transition"
              title="Attach a local text or image file"
            >
              <Paperclip className="w-4 h-4" />
            </button>

            {/* Voice Dictation Button */}
            <button
              type="button"
              onClick={toggleVoiceDictation}
              className={`p-3 rounded-xl border transition ${
                isListening
                  ? "bg-red-950/40 text-red-400 border-red-700/60 animate-pulse"
                  : "bg-[#000000] hover:bg-neutral-900 border-neutral-800 text-neutral-400 hover:text-neutral-200"
              }`}
              title={isListening ? "Listening... click to stop" : "Voice Dictation (Speech to Text)"}
            >
              {isListening ? <Mic className="w-4 h-4 text-red-400 animate-bounce" /> : <MicOff className="w-4 h-4" />}
            </button>

            <input
              type="text"
              placeholder={
                isListening
                  ? "Listening to your voice..."
                  : `Ask @${activeBot.name} to automate a desktop task, run terminal commands, or teach a skill...`
              }
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              disabled={isAgentRunning}
              className="flex-1 px-4 py-3 bg-[#000000] border border-neutral-800 rounded-xl text-sm text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-500 transition"
            />

            {isAgentRunning ? (
              <button
                type="button"
                onClick={handleAbort}
                className="px-5 py-3 rounded-xl bg-red-600 hover:bg-red-500 text-white text-xs font-semibold uppercase tracking-wider flex items-center gap-2 shadow-sm transition"
              >
                <StopCircle className="w-4 h-4 fill-current" /> Stop
              </button>
            ) : (
              <button
                type="submit"
                disabled={!prompt.trim() && !attachedFile}
                className="px-5 py-3 rounded-xl bg-neutral-100 hover:bg-white disabled:opacity-40 text-neutral-950 text-xs font-medium flex items-center gap-2 shadow-sm transition"
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
        onSelectModel={setSelectedModel}
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
          setActiveBot(bot);
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
        onSelectBot={(bot) => setActiveBot(bot)}
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
