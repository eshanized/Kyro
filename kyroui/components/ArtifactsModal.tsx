import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArtifactRecord } from "../types/agent";
import {
  X,
  FileText,
  Code,
  FileJson,
  Image as ImageIcon,
  Download,
  Trash2,
  Search,
  Layers,
  FileCheck,
} from "lucide-react";

interface ArtifactsModalProps {
  isOpen: boolean;
  onClose: () => void;
  runId?: string;
  conversationId?: string;
}

export const ArtifactsModal: React.FC<ArtifactsModalProps> = ({
  isOpen,
  onClose,
  runId,
  conversationId,
}) => {
  const [artifacts, setArtifacts] = useState<ArtifactRecord[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState<ArtifactRecord | null>(null);
  const [previewContent, setPreviewContent] = useState<string>("");
  const [searchFilter, setSearchFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<string>("all");
  const [exportPath, setExportPath] = useState("");
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      loadArtifacts();
    }
  }, [isOpen, runId, conversationId]);

  const loadArtifacts = async () => {
    try {
      const list = await invoke<ArtifactRecord[]>("list_artifacts", {
        runId: runId || null,
        conversationId: conversationId || null,
      });
      setArtifacts(list);
      if (list.length > 0 && !selectedArtifact) {
        selectArtifact(list[0]);
      }
    } catch (err) {
      console.error("Failed to load artifacts:", err);
    }
  };

  const selectArtifact = async (art: ArtifactRecord) => {
    setSelectedArtifact(art);
    setStatusMessage(null);
    try {
      const content = await invoke<string>("read_artifact_content", { id: art.id });
      setPreviewContent(content);
    } catch (e: any) {
      setPreviewContent(`(Cannot preview content: ${e})`);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("delete_artifact", { id });
      setArtifacts((prev) => prev.filter((a) => a.id !== id));
      if (selectedArtifact?.id === id) {
        setSelectedArtifact(null);
        setPreviewContent("");
      }
      setStatusMessage("Artifact deleted successfully.");
    } catch (e: any) {
      setStatusMessage(`Error deleting artifact: ${e}`);
    }
  };

  const handleExport = async () => {
    if (!selectedArtifact || !exportPath.trim()) return;
    try {
      await invoke("export_artifact", {
        id: selectedArtifact.id,
        destinationPath: exportPath.trim(),
      });
      setStatusMessage(`Exported successfully to: ${exportPath}`);
    } catch (e: any) {
      setStatusMessage(`Export error: ${e}`);
    }
  };

  if (!isOpen) return null;

  const filteredArtifacts = artifacts.filter((a) => {
    const matchesSearch =
      a.name.toLowerCase().includes(searchFilter.toLowerCase()) ||
      a.file_path.toLowerCase().includes(searchFilter.toLowerCase());
    const matchesType = typeFilter === "all" || a.artifact_type === typeFilter;
    return matchesSearch && matchesType;
  });

  const getIcon = (type: string) => {
    switch (type) {
      case "code":
        return <Code className="w-4 h-4 text-neutral-300" />;
      case "json":
        return <FileJson className="w-4 h-4 text-neutral-300" />;
      case "image":
        return <ImageIcon className="w-4 h-4 text-neutral-300" />;
      default:
        return <FileText className="w-4 h-4 text-neutral-300" />;
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-6 select-text">
      <div className="bg-[#121212] border border-[#262626] w-full max-w-5xl h-[80vh] rounded-2xl flex flex-col shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="px-6 py-4 border-b border-[#262626] flex items-center justify-between bg-[#141414]">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-xl bg-neutral-800 border border-neutral-700 text-neutral-200">
              <Layers className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-bold text-neutral-100 flex items-center gap-2">
                Local Artifact Registry
                <span className="text-xs px-2 py-0.5 rounded-full bg-neutral-800 text-neutral-300 border border-neutral-700 font-mono">
                  {artifacts.length} Files
                </span>
              </h2>
              <p className="text-xs text-neutral-400">
                Locally generated code, scripts, markdown reports, structured data, and logs
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-neutral-400 hover:text-white hover:bg-neutral-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 flex overflow-hidden">
          {/* Left: Artifact List */}
          <div className="w-80 border-r border-[#262626] flex flex-col bg-[#0f0f0f]">
            {/* Filters */}
            <div className="p-3 border-b border-[#262626] space-y-2">
              <div className="relative">
                <Search className="w-3.5 h-3.5 text-neutral-400 absolute left-2.5 top-2.5" />
                <input
                  type="text"
                  placeholder="Filter artifacts..."
                  value={searchFilter}
                  onChange={(e) => setSearchFilter(e.target.value)}
                  className="w-full pl-8 pr-3 py-1.5 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder-neutral-500 focus:outline-none focus:border-neutral-400"
                />
              </div>
              <div className="flex gap-1 overflow-x-auto pb-1 text-[11px]">
                {["all", "code", "markdown", "json", "document"].map((t) => (
                  <button
                    key={t}
                    onClick={() => setTypeFilter(t)}
                    className={`px-2 py-1 rounded-md capitalize transition whitespace-nowrap ${
                      typeFilter === t
                        ? "bg-[#ededed] text-[#0a0a0a] font-semibold"
                        : "bg-[#1a1a1a] text-neutral-400 hover:text-neutral-200"
                    }`}
                  >
                    {t}
                  </button>
                ))}
              </div>
            </div>

            {/* List */}
            <div className="flex-1 overflow-y-auto p-2 space-y-1">
              {filteredArtifacts.length === 0 ? (
                <div className="p-6 text-center text-xs text-neutral-500">
                  No artifacts found.
                </div>
              ) : (
                filteredArtifacts.map((art) => (
                  <div
                    key={art.id}
                    onClick={() => selectArtifact(art)}
                    className={`p-2.5 rounded-xl cursor-pointer transition flex items-start gap-2.5 ${
                      selectedArtifact?.id === art.id
                        ? "bg-neutral-800 border border-neutral-600 text-neutral-100"
                        : "bg-[#141414] hover:bg-neutral-800/60 border border-transparent text-neutral-300"
                    }`}
                  >
                    <div className="mt-0.5">{getIcon(art.artifact_type)}</div>
                    <div className="min-w-0 flex-1">
                      <div className="text-xs font-semibold truncate text-neutral-100">
                        {art.name}
                      </div>
                      <div className="text-[10px] text-neutral-400 flex items-center justify-between mt-0.5 font-mono">
                        <span>{formatSize(art.size_bytes)}</span>
                        <span>{new Date(art.created_at).toLocaleDateString()}</span>
                      </div>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>

          {/* Right: Preview & Details */}
          <div className="flex-1 flex flex-col bg-[#0a0a0a] overflow-hidden">
            {selectedArtifact ? (
              <div className="flex-1 flex flex-col h-full overflow-hidden">
                {/* Meta Bar */}
                <div className="p-4 border-b border-[#262626] bg-[#121212] flex items-center justify-between">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-bold text-neutral-100 truncate">
                        {selectedArtifact.name}
                      </h3>
                      <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-neutral-800 text-neutral-300 border border-neutral-700 uppercase">
                        {selectedArtifact.artifact_type}
                      </span>
                    </div>
                    <p className="text-[11px] text-neutral-400 font-mono truncate mt-0.5">
                      {selectedArtifact.file_path}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => handleDelete(selectedArtifact.id)}
                      className="px-2.5 py-1.5 rounded-lg bg-red-600/20 hover:bg-red-600/30 text-red-300 border border-red-500/30 text-xs flex items-center gap-1 transition"
                      title="Delete Artifact"
                    >
                      <Trash2 className="w-3.5 h-3.5" /> Delete
                    </button>
                  </div>
                </div>

                {/* Status Message */}
                {statusMessage && (
                  <div className="px-4 py-2 bg-neutral-900 border-b border-neutral-800 text-xs text-neutral-200 flex items-center gap-1.5">
                    <FileCheck className="w-4 h-4 text-neutral-400" />
                    <span>{statusMessage}</span>
                  </div>
                )}

                {/* Preview Window */}
                <div className="flex-1 overflow-auto p-4 font-mono text-xs text-neutral-200 bg-[#000000] whitespace-pre-wrap leading-relaxed select-text">
                  {previewContent || "(Empty or binary content)"}
                </div>

                {/* Export Bar */}
                <div className="p-3 border-t border-[#262626] bg-[#121212] flex items-center gap-2">
                  <input
                    type="text"
                    placeholder="Enter destination file path to export (e.g. /home/user/output.py)..."
                    value={exportPath}
                    onChange={(e) => setExportPath(e.target.value)}
                    className="flex-1 px-3 py-1.5 bg-[#0a0a0a] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400 font-mono"
                  />
                  <button
                    onClick={handleExport}
                    disabled={!exportPath.trim()}
                    className="px-4 py-1.5 rounded-lg bg-[#ededed] hover:bg-neutral-300 disabled:opacity-40 text-[#0a0a0a] text-xs font-semibold flex items-center gap-1.5 transition shadow"
                  >
                    <Download className="w-3.5 h-3.5" /> Export
                  </button>
                </div>
              </div>
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-center text-neutral-500 p-6">
                <FileText className="w-12 h-12 text-neutral-600 mb-2" />
                <p className="text-sm">Select an artifact to preview contents</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
