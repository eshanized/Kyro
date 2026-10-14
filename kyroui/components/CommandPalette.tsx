import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SearchResultItem, BotRecord } from "../types/agent";
import {
  Search,
  Plus,
  Ghost,
  Terminal,
  Users,
  Wrench,
  History,
  Layers,
  Settings,
  Activity,
  MessageSquare,
  FileText,
  Brain,
  Clock,
  ArrowRight,
  Plug,
  Zap,
  Globe,
  Workflow,
} from "lucide-react";

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  onNewSession: () => void;
  onToggleGhost: () => void;
  onToggleTerminal: () => void;
  onOpenTeammates: () => void;
  onOpenTeachTask: (tab?: "skills" | "record" | "routines", taskId?: string) => void;
  onOpenMcp: () => void;
  onOpenConnections?: () => void;
  onOpenTimeline: () => void;
  onOpenArtifacts: () => void;
  onOpenSettings: (tab?: string) => void;
  onSelectConversation: (id: string) => void;
  onSelectBot: (bot: BotRecord) => void;
}

interface ActionCommand {
  id: string;
  title: string;
  subtitle: string;
  icon: React.ReactNode;
  category: "Actions" | "Teammates" | "Views";
  execute: () => void;
}

export const CommandPalette: React.FC<CommandPaletteProps> = ({
  isOpen,
  onClose,
  onNewSession,
  onToggleGhost,
  onToggleTerminal,
  onOpenTeammates,
  onOpenTeachTask,
  onOpenMcp,
  onOpenConnections,
  onOpenTimeline,
  onOpenArtifacts,
  onOpenSettings,
  onSelectConversation,
  onSelectBot,
}) => {
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResultItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSearchResults([]);
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (isOpen) {
          onClose();
        } else {
          // Trigger open handled by parent
        }
      }
      if (isOpen && e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  useEffect(() => {
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const res = await invoke<SearchResultItem[]>("search_workspace", {
          query: query.trim(),
        });
        setSearchResults(res);
        setSelectedIndex(0);
      } catch (err) {
        console.error("Search failed:", err);
      }
    }, 150);
    return () => clearTimeout(timer);
  }, [query]);

  if (!isOpen) return null;

  const defaultActions: ActionCommand[] = [
    {
      id: "new-session",
      title: "New Autonomous Session",
      subtitle: "Start a fresh task thread",
      icon: <Plus className="w-4 h-4 text-neutral-300" />,
      category: "Actions",
      execute: () => {
        onNewSession();
        onClose();
      },
    },
    {
      id: "toggle-ghost",
      title: "Toggle Ghost Mode",
      subtitle: "Run desktop actions in isolated virtual display (:99)",
      icon: <Ghost className="w-4 h-4 text-neutral-300" />,
      category: "Actions",
      execute: () => {
        onToggleGhost();
        onClose();
      },
    },
    {
      id: "toggle-terminal",
      title: "Toggle PTY Host Terminal",
      subtitle: "Interactive ANSI shell drawer",
      icon: <Terminal className="w-4 h-4 text-neutral-300" />,
      category: "Actions",
      execute: () => {
        onToggleTerminal();
        onClose();
      },
    },
    {
      id: "artifacts-registry",
      title: "Open Artifacts Registry",
      subtitle: "Inspect, preview, and export locally generated files",
      icon: <Layers className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenArtifacts();
        onClose();
      },
    },
    {
      id: "teach-task",
      title: "Teach-a-Task & Routines",
      subtitle: "Record workflows and schedule recurring cron tasks",
      icon: <Workflow className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenTeachTask();
        onClose();
      },
    },
    {
      id: "teammates",
      title: "Teammate Roster",
      subtitle: "Apex, Forge, Scout agent delegation statuses",
      icon: <Users className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenTeammates();
        onClose();
      },
    },
    {
      id: "app-connections",
      title: "App Connections & Integrations",
      subtitle: "Connect GitHub, remote MCP, and browse integration catalog",
      icon: <Plug className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        if (onOpenConnections) onOpenConnections();
        onClose();
      },
    },
    {
      id: "mcp-servers",
      title: "MCP Tool Servers",
      subtitle: "Manage Model Context Protocol tool connections",
      icon: <Wrench className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenMcp();
        onClose();
      },
    },
    {
      id: "timeline",
      title: "Step Timeline & Visual Diffs",
      subtitle: "Pixel diffs, click markers, and step history",
      icon: <History className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenTimeline();
        onClose();
      },
    },
    {
      id: "health",
      title: "System Health & Diagnostics",
      subtitle: "Probe SQLite WAL, Docker, Xvfb, Keyring, and CLI tools",
      icon: <Activity className="w-4 h-4 text-neutral-300" />,
      category: "Views",
      execute: () => {
        onOpenSettings("diagnostics");
        onClose();
      },
    },
    {
      id: "settings",
      title: "Settings & Vault",
      subtitle: "API Keys, Telegram bridge, and Governance policy",
      icon: <Settings className="w-4 h-4 text-neutral-400" />,
      category: "Views",
      execute: () => {
        onOpenSettings("vault");
        onClose();
      },
    },
    {
      id: "switch-apex",
      title: "Switch to @Apex",
      subtitle: "General Workspace & Strategic Orchestrator",
      icon: <Zap className="w-4 h-4 text-neutral-300" />,
      category: "Teammates",
      execute: () => {
        onSelectBot({
          id: "apex-orchestrator",
          name: "Apex",
          avatar: "zap",
          role: "General Workspace & Strategic Orchestrator",
          system_prompt: "",
          is_default: true,
        });
        onClose();
      },
    },
    {
      id: "switch-forge",
      title: "Switch to @Forge",
      subtitle: "DevOps, Code & Terminal Specialist",
      icon: <Wrench className="w-4 h-4 text-neutral-300" />,
      category: "Teammates",
      execute: () => {
        onSelectBot({
          id: "forge-devops",
          name: "Forge",
          avatar: "wrench",
          role: "DevOps, Code & Terminal Specialist",
          system_prompt: "",
          is_default: false,
        });
        onClose();
      },
    },
    {
      id: "switch-scout",
      title: "Switch to @Scout",
      subtitle: "Browser Automation & Web Researcher",
      icon: <Globe className="w-4 h-4 text-neutral-300" />,
      category: "Teammates",
      execute: () => {
        onSelectBot({
          id: "scout-web",
          name: "Scout",
          avatar: "globe",
          role: "Browser Automation & Web Researcher",
          system_prompt: "",
          is_default: false,
        });
        onClose();
      },
    },
  ];

  const handleKeyDown = (e: React.KeyboardEvent) => {
    const totalItems = query.trim() ? searchResults.length : defaultActions.length;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % Math.max(1, totalItems));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + totalItems) % Math.max(1, totalItems));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (query.trim()) {
        const item = searchResults[selectedIndex];
        if (item) {
          handleSelectSearchResult(item);
        }
      } else {
        const act = defaultActions[selectedIndex];
        if (act) {
          act.execute();
        }
      }
    }
  };

  const handleSelectSearchResult = (item: SearchResultItem) => {
    onClose();
    if (item.category === "conversation" || item.category === "message") {
      onSelectConversation(item.id.startsWith("msg_") ? item.title.replace("Message in ", "") : item.id);
    } else if (item.category === "artifact") {
      onOpenArtifacts();
    } else if (item.category === "skill") {
      onOpenTeachTask("skills", item.id);
    } else if (item.category === "routine") {
      onOpenTeachTask("routines", item.id);
    } else if (item.category === "memory") {
      onOpenSettings("memory");
    }
  };

  const getResultIcon = (cat: string) => {
    switch (cat) {
      case "conversation":
        return <MessageSquare className="w-4 h-4 text-neutral-400" />;
      case "message":
        return <MessageSquare className="w-4 h-4 text-neutral-400" />;
      case "artifact":
        return <Layers className="w-4 h-4 text-neutral-400" />;
      case "skill":
        return <Workflow className="w-4 h-4 text-neutral-400" />;
      case "routine":
        return <Clock className="w-4 h-4 text-neutral-400" />;
      case "memory":
        return <Brain className="w-4 h-4 text-neutral-400" />;
      default:
        return <FileText className="w-4 h-4 text-neutral-400" />;
    }
  };

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-start justify-center pt-20 bg-black/75 backdrop-blur-sm p-4 select-none"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-[#111111] border border-neutral-800 w-full max-w-2xl rounded-2xl shadow-2xl overflow-hidden flex flex-col"
      >
        {/* Search Bar */}
        <div className="p-4 border-b border-neutral-800 flex items-center gap-3 bg-[#141414]">
          <Search className="w-5 h-5 text-neutral-400" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Type a command or search conversations, messages, artifacts, memories... (Esc to close)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            className="flex-1 bg-transparent text-sm text-neutral-100 placeholder-neutral-500 focus:outline-none"
          />
          <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-neutral-800 text-neutral-400 border border-neutral-700">
            Ctrl+K
          </span>
        </div>

        {/* Results or Actions List */}
        <div className="max-h-[60vh] overflow-y-auto p-2 space-y-1">
          {query.trim() ? (
            searchResults.length === 0 ? (
              <div className="p-8 text-center text-xs text-neutral-500">
                No matching results for "{query}"
              </div>
            ) : (
              searchResults.map((item, idx) => (
                <div
                  key={`${item.category}-${item.id}-${idx}`}
                  onClick={() => handleSelectSearchResult(item)}
                  className={`p-3 rounded-xl cursor-pointer transition flex items-center justify-between border ${
                    selectedIndex === idx
                      ? "bg-neutral-800 text-neutral-100 border-neutral-700"
                      : "border-transparent hover:bg-surface-hover text-neutral-300"
                  }`}
                >
                  <div className="flex items-center gap-3 min-w-0 pr-2">
                    {getResultIcon(item.category)}
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-semibold text-neutral-100 truncate">
                          {item.title}
                        </span>
                        <span className="text-[9px] uppercase font-mono px-1.5 py-0.2 rounded bg-neutral-900 text-neutral-400 border border-neutral-800">
                          {item.category}
                        </span>
                      </div>
                      <div className="text-[11px] text-neutral-400 truncate mt-0.5">
                        {item.snippet}
                      </div>
                    </div>
                  </div>
                  <ArrowRight className="w-3.5 h-3.5 text-neutral-500 flex-shrink-0" />
                </div>
              ))
            )
          ) : (
            defaultActions.map((act, idx) => (
              <div
                key={act.id}
                onClick={act.execute}
                className={`p-3 rounded-xl cursor-pointer transition flex items-center justify-between border ${
                  selectedIndex === idx
                    ? "bg-neutral-800 text-neutral-100 border-neutral-700"
                    : "border-transparent hover:bg-surface-hover text-neutral-300"
                }`}
              >
                <div className="flex items-center gap-3">
                  <div className="p-1.5 rounded-lg bg-neutral-900 border border-neutral-800/60">{act.icon}</div>
                  <div>
                    <div className="text-xs font-semibold text-neutral-100">{act.title}</div>
                    <div className="text-[10px] text-neutral-400">{act.subtitle}</div>
                  </div>
                </div>
                <span className="text-[9px] uppercase font-mono px-2 py-0.5 rounded bg-neutral-900 text-neutral-400 border border-neutral-800">
                  {act.category}
                </span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
