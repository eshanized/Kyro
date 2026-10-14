import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { McpServerRecord } from "../types/agent";
import { Plug, Plus, Trash2, X, Server } from "lucide-react";
import { ConfirmDialog } from "./ConfirmDialog";
import { InlineNotice } from "./InlineNotice";

interface McpModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const McpModal: React.FC<McpModalProps> = ({ isOpen, onClose }) => {
  const [servers, setServers] = useState<McpServerRecord[]>([]);
  const [isAdding, setIsAdding] = useState(false);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("npx");
  const [args, setArgs] = useState("-y @modelcontextprotocol/server-filesystem /home/snigdha");
  const [error, setError] = useState<string | null>(null);
  const [serverPendingDeletion, setServerPendingDeletion] = useState<McpServerRecord | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadServers();
    }
  }, [isOpen]);

  const loadServers = async () => {
    try {
      const list = await invoke<McpServerRecord[]>("list_mcp_servers");
      setServers(list);
    } catch (e) {
      console.error(e);
    }
  };

  const handleAdd = async () => {
    if (!name.trim() || !command.trim()) return;
    try {
      const argsArray = args.trim().split(/\s+/).filter(Boolean);
      await invoke("create_mcp_server", {
        server: {
          id: `mcp-${Date.now()}`,
          name,
          command,
          args_json: JSON.stringify(argsArray),
          env_json: JSON.stringify({}),
          enabled: true,
        },
      });
      setIsAdding(false);
      setName("");
      setCommand("npx");
      setArgs("");
      setError(null);
      await loadServers();
    } catch (err: any) {
      setError("Failed to add MCP server: " + err);
    }
  };

  const handleDelete = async (id: string) => {
    setIsDeleting(true);
    try {
      await invoke("delete_mcp_server", { id });
      // The row is only removed once the backend confirmed the deletion.
      setServers((prev) => prev.filter((s) => s.id !== id));
      setError(null);
    } catch (err: any) {
      setError("Failed to delete MCP server: " + err);
    } finally {
      setIsDeleting(false);
      setServerPendingDeletion(null);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md p-4">
      <div className="w-full max-w-2xl bg-[#121212] border border-[#262626] rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#262626] bg-[#141414]">
          <div className="flex items-center gap-2.5">
            <Plug className="w-5 h-5 text-neutral-300" />
            <h2 className="text-base font-semibold text-neutral-100">Model Context Protocol (MCP) Tools</h2>
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
              <Server className="w-4 h-4 text-neutral-300" /> Native MCP Host Integration
            </div>
            Connect any standard Model Context Protocol (MCP) server. Kyro dynamically queries tools via JSON-RPC 2.0 and exposes them to NVIDIA Nemotron-3 Ultra 550B during task execution.
          </div>

          <div className="flex items-center justify-between">
            <span className="text-xs text-neutral-400 font-medium">Configured Tool Servers ({servers.length})</span>
            <button
              onClick={() => setIsAdding(!isAdding)}
              className="px-3 py-1.5 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-medium flex items-center gap-1.5 shadow transition"
            >
              <Plus className="w-3.5 h-3.5" /> {isAdding ? "Cancel" : "Add MCP Server"}
            </button>
          </div>

          {/* Add Server Form */}
          {isAdding && (
            <div className="p-4 bg-[#171717] border border-[#262626] rounded-xl space-y-3">
              <div className="text-xs font-semibold text-neutral-200">Connect New Server</div>
              <div>
                <label className="block text-xs text-neutral-300 mb-1">Server Name</label>
                <input
                  type="text"
                  placeholder="e.g. Local Filesystem or Brave Search"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400"
                />
              </div>
              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-xs text-neutral-300 mb-1">Command</label>
                  <input
                    type="text"
                    placeholder="npx or python3"
                    value={command}
                    onChange={(e) => setCommand(e.target.value)}
                    className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 font-mono"
                  />
                </div>
                <div className="col-span-2">
                  <label className="block text-xs text-neutral-300 mb-1">Arguments</label>
                  <input
                    type="text"
                    placeholder="-y @modelcontextprotocol/server-filesystem /path"
                    value={args}
                    onChange={(e) => setArgs(e.target.value)}
                    className="w-full px-3 py-2 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 font-mono"
                  />
                </div>
              </div>
              <button
                onClick={handleAdd}
                className="px-4 py-2 bg-[#ededed] hover:bg-neutral-300 text-[#0a0a0a] rounded-lg text-xs font-semibold shadow transition"
              >
                Save & Connect Server
              </button>
            </div>
          )}

          {/* Servers list */}
          {servers.length === 0 ? (
            <div className="p-8 text-center border border-dashed border-[#262626] rounded-xl text-neutral-500 text-xs">
              No custom MCP servers connected. Default built-in tools (Screen Capture, Terminal Shell, Input Control) are active.
            </div>
          ) : (
            <div className="space-y-2.5">
              {servers.map((s) => (
                <div
                  key={s.id}
                  className="p-4 bg-[#171717] border border-[#262626] rounded-xl flex items-center justify-between text-xs text-neutral-300"
                >
                  <div className="flex items-center gap-3">
                    <span className="w-2 h-2 rounded-full bg-neutral-400"></span>
                    <div>
                      <div className="font-semibold text-neutral-100 flex items-center gap-2">
                        {s.name}
                        <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-black/60 text-neutral-300 border border-neutral-800">
                          {s.command}
                        </span>
                      </div>
                      <div className="text-[11px] text-neutral-400 font-mono mt-0.5">
                        Args: {s.args_json}
                      </div>
                    </div>
                  </div>
                  <button
                    onClick={() => setServerPendingDeletion(s)}
                    aria-label={`Delete ${s.name}`}
                    className="p-1.5 text-neutral-400 hover:text-red-400 rounded-lg hover:bg-neutral-800 transition"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {error && (
          <div className="px-4 py-3 border-t border-[#262626]">
            <InlineNotice tone="error" message={error} onDismiss={() => setError(null)} />
          </div>
        )}
      </div>

      <ConfirmDialog
        isOpen={serverPendingDeletion !== null}
        title="Remove this MCP server?"
        message={
          serverPendingDeletion
            ? `"${serverPendingDeletion.name}" will be removed from Kyro and its process will be stopped. This cannot be undone.`
            : ""
        }
        confirmLabel="Remove server"
        destructive
        isBusy={isDeleting}
        onCancel={() => setServerPendingDeletion(null)}
        onConfirm={() => {
          if (serverPendingDeletion) handleDelete(serverPendingDeletion.id);
        }}
      />
    </div>
  );
};
