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
  MessageSquare,
  Columns,
  Monitor,
} from "lucide-react";
import { ConfirmDialog } from "./ConfirmDialog";

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
  const [compareArtifact, setCompareArtifact] = useState<ArtifactRecord | null>(null);
  const [compareContent, setCompareContent] = useState<string>("");
  const [commentInput, setCommentInput] = useState<string>("");
  const [isAddingComment, setIsAddingComment] = useState(false);
  const [searchFilter, setSearchFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<string>("all");
  const [exportPath, setExportPath] = useState("");
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [artifactPendingDeletion, setArtifactPendingDeletion] = useState<ArtifactRecord | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

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
    setCompareArtifact(null);
    setCompareContent("");
    setCommentInput("");
    try {
      const content = await invoke<string>("read_artifact_content", { id: art.id });
      setPreviewContent(content);
    } catch (e: any) {
      setPreviewContent(`(Cannot preview content: ${e})`);
    }
  };

  const handleCompareSelect = async (otherId: string) => {
    if (!otherId) {
      setCompareArtifact(null);
      setCompareContent("");
      return;
    }
    const other = artifacts.find((a) => a.id === otherId);
    if (other) {
      setCompareArtifact(other);
      try {
        const content = await invoke<string>("read_artifact_content", { id: other.id });
        setCompareContent(content);
      } catch (e: any) {
        setCompareContent("");
      }
    }
  };

  const handleAddComment = async () => {
    if (!selectedArtifact || !commentInput.trim()) return;
    setIsAddingComment(true);
    setStatusMessage(null);
    try {
      await invoke("add_artifact_comment", {
        id: selectedArtifact.id,
        comment: commentInput.trim(),
        feedToAgent: true,
      });
      setStatusMessage("Comment saved and queued into conversation history for the next agent turn.");
      setCommentInput("");
      const list = await invoke<ArtifactRecord[]>("list_artifacts", {
        runId: runId || null,
        conversationId: conversationId || null,
      });
      setArtifacts(list);
      const updated = list.find((a) => a.id === selectedArtifact.id);
      if (updated) {
        setSelectedArtifact(updated);
      }
    } catch (e: any) {
      const errMsg = typeof e === "string" ? e : e?.message || String(e);
      setStatusMessage(`Error adding comment: ${errMsg}`);
    } finally {
      setIsAddingComment(false);
    }
  };

  const parseMetadata = (art: ArtifactRecord | null) => {
    if (!art?.metadata_json) return { comments: [] };
    try {
      return JSON.parse(art.metadata_json);
    } catch {
      return { comments: [] };
    }
  };

  const getViewportBadge = (name?: string, meta?: any) => {
    if (meta?.viewport) return meta.viewport;
    const match = (name || "").match(/(\d+x\d+)/);
    if (match) return match[1];
    return null;
  };

  const handleDelete = async (id: string) => {
    setIsDeleting(true);
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
    } finally {
      setIsDeleting(false);
      setArtifactPendingDeletion(null);
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
    const name = a.name || "";
    const filePath = a.file_path || "";
    const matchesSearch =
      name.toLowerCase().includes(searchFilter.toLowerCase()) ||
      filePath.toLowerCase().includes(searchFilter.toLowerCase());
    const matchesType = typeFilter === "all" || a.artifact_type === typeFilter;
    return matchesSearch && matchesType;
  });

  const imageArtifacts = artifacts.filter(
    (a) => a.artifact_type === "image" || (a.mime_type && a.mime_type.startsWith("image/"))
  );

  const getIcon = (type: string) => {
    switch (type) {
      case "code":
        return <Code className="w-4 h-4 text-neutral-300" />;
      case "json":
        return <FileJson className="w-4 h-4 text-neutral-300" />;
      case "image":
        return <ImageIcon className="w-4 h-4 text-blue-400" />;
      default:
        return <FileText className="w-4 h-4 text-neutral-300" />;
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const isImage =
    selectedArtifact?.artifact_type === "image" ||
    (previewContent || "").startsWith("data:image/") ||
    (selectedArtifact?.mime_type || "").startsWith("image/");

  const metadata = parseMetadata(selectedArtifact);
  const viewportBadge = selectedArtifact ? getViewportBadge(selectedArtifact.name, metadata) : null;
  const comments: Array<{ timestamp: number; comment: string }> = metadata.comments || [];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-6 select-text">
      <div className="bg-[#121212] border border-[#262626] w-full max-w-5xl h-[85vh] rounded-2xl flex flex-col shadow-2xl overflow-hidden">
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
                Visual audit screenshots, code, markdown reports, structured data, and evidence
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
                {["all", "image", "markdown", "code", "json", "document"].map((t) => (
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
                  <div className="min-w-0 flex-1 mr-4">
                    <div className="flex items-center gap-2 flex-wrap">
                      <h3 className="text-sm font-bold text-neutral-100 truncate">
                        {selectedArtifact.name}
                      </h3>
                      <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-neutral-800 text-neutral-300 border border-neutral-700 uppercase">
                        {selectedArtifact.artifact_type}
                      </span>
                      {viewportBadge && (
                        <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-blue-950/60 text-blue-300 border border-blue-800/50 flex items-center gap-1">
                          <Monitor className="w-3 h-3" />
                          {viewportBadge}
                        </span>
                      )}
                      {metadata.route && (
                        <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-neutral-800/80 text-neutral-400 border border-neutral-700 truncate max-w-[200px]">
                          Route: {metadata.route}
                        </span>
                      )}
                    </div>
                    <p className="text-[11px] text-neutral-400 font-mono truncate mt-0.5">
                      {selectedArtifact.file_path}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    {isImage && imageArtifacts.length > 1 && (
                      <div className="flex items-center gap-1.5 bg-neutral-900 border border-neutral-800 px-2 py-1 rounded-lg text-xs text-neutral-300">
                        <Columns className="w-3.5 h-3.5 text-neutral-400" />
                        <span className="text-[11px] text-neutral-400">Compare:</span>
                        <select
                          value={compareArtifact?.id || ""}
                          onChange={(e) => handleCompareSelect(e.target.value)}
                          className="bg-transparent text-xs text-neutral-200 focus:outline-none cursor-pointer max-w-[140px] truncate"
                        >
                          <option value="">(None)</option>
                          {imageArtifacts
                            .filter((a) => a.id !== selectedArtifact.id)
                            .map((a) => (
                              <option key={a.id} value={a.id} className="bg-neutral-900 text-neutral-200">
                                {a.name}
                              </option>
                            ))}
                        </select>
                      </div>
                    )}
                    <button
                      onClick={() => setArtifactPendingDeletion(selectedArtifact)}
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

                {/* Main Content View (Image or Text) */}
                {isImage ? (
                  <div className="flex-1 flex flex-col overflow-hidden">
                    <div className="flex-1 overflow-auto p-4 bg-[#050505]">
                      {compareArtifact && compareContent ? (
                        /* Side-by-Side Comparison */
                        <div className="grid grid-cols-2 gap-4 h-full min-h-[300px]">
                          <div className="flex flex-col border border-[#262626] rounded-xl p-3 bg-[#0c0c0c] overflow-hidden">
                            <div className="text-xs font-semibold text-neutral-300 mb-2 truncate flex items-center justify-between">
                              <span className="truncate">{selectedArtifact.name}</span>
                              {viewportBadge && (
                                <span className="text-[10px] font-mono text-blue-400 bg-blue-950/40 px-1.5 py-0.5 rounded border border-blue-900">
                                  {viewportBadge}
                                </span>
                              )}
                            </div>
                            <div className="flex-1 flex items-center justify-center overflow-auto bg-black rounded-lg p-2">
                              <img
                                src={previewContent}
                                alt={selectedArtifact.name}
                                className="max-w-full max-h-full object-contain rounded"
                              />
                            </div>
                          </div>

                          <div className="flex flex-col border border-[#262626] rounded-xl p-3 bg-[#0c0c0c] overflow-hidden">
                            <div className="text-xs font-semibold text-neutral-300 mb-2 truncate flex items-center justify-between">
                              <span className="truncate">{compareArtifact.name}</span>
                              {getViewportBadge(compareArtifact.name, parseMetadata(compareArtifact)) && (
                                <span className="text-[10px] font-mono text-blue-400 bg-blue-950/40 px-1.5 py-0.5 rounded border border-blue-900">
                                  {getViewportBadge(compareArtifact.name, parseMetadata(compareArtifact))}
                                </span>
                              )}
                            </div>
                            <div className="flex-1 flex items-center justify-center overflow-auto bg-black rounded-lg p-2">
                              <img
                                src={compareContent}
                                alt={compareArtifact.name}
                                className="max-w-full max-h-full object-contain rounded"
                              />
                            </div>
                          </div>
                        </div>
                      ) : (
                        /* Single Visual View */
                        <div className="flex items-center justify-center min-h-full">
                          <img
                            src={previewContent}
                            alt={selectedArtifact.name}
                            className="max-w-full max-h-[60vh] object-contain rounded-xl border border-neutral-800 shadow-2xl bg-black"
                          />
                        </div>
                      )}
                    </div>

                    {/* Visual Artifact Feedback & Comments */}
                    <div className="border-t border-[#262626] bg-[#0e0e0e] p-3 space-y-2">
                      {comments.length > 0 && (
                        <div className="space-y-1.5 max-h-24 overflow-y-auto">
                          {comments.map((c, i) => (
                            <div
                              key={i}
                              className="text-xs bg-[#161616] border border-neutral-800 p-2 rounded-lg text-neutral-200 flex items-start justify-between gap-2"
                            >
                              <div className="flex items-center gap-1.5">
                                <MessageSquare className="w-3.5 h-3.5 text-neutral-400 mt-0.5 shrink-0" />
                                <span>{c.comment}</span>
                              </div>
                              <span className="text-[10px] text-neutral-500 font-mono shrink-0">
                                {new Date(c.timestamp).toLocaleTimeString()}
                              </span>
                            </div>
                          ))}
                        </div>
                      )}

                      <div className="flex items-center gap-2">
                        <input
                          type="text"
                          placeholder="Add comment or feedback (feeds directly into active agent task)..."
                          value={commentInput}
                          onChange={(e) => setCommentInput(e.target.value)}
                          onKeyDown={(e) => e.key === "Enter" && handleAddComment()}
                          className="flex-1 px-3 py-1.5 bg-[#080808] border border-[#262626] rounded-lg text-xs text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-neutral-400"
                        />
                        <button
                          onClick={handleAddComment}
                          disabled={!commentInput.trim() || isAddingComment}
                          className="px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 disabled:opacity-40 text-neutral-200 text-xs font-semibold flex items-center gap-1.5 transition border border-neutral-700 shrink-0"
                        >
                          <MessageSquare className="w-3.5 h-3.5 text-neutral-400" />
                          Send Feedback
                        </button>
                      </div>
                    </div>
                  </div>
                ) : (
                  /* Monospace Text/Markdown/Code Preview */
                  <div className="flex-1 overflow-auto p-4 font-mono text-[12px] text-neutral-200 bg-[#000000] whitespace-pre-wrap leading-relaxed select-text">
                    {previewContent || "(Empty content)"}
                  </div>
                )}

                {/* Export Bar */}
                <div className="p-3 border-t border-[#262626] bg-[#121212] flex items-center gap-2">
                  <input
                    type="text"
                    placeholder="Enter destination file path to export (e.g. /home/user/evidence.png)..."
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

      <ConfirmDialog
        isOpen={artifactPendingDeletion !== null}
        title="Delete this artifact?"
        message={
          artifactPendingDeletion
            ? `"${artifactPendingDeletion.name}" will be removed from the artifact registry and its file will be deleted from disk. This cannot be undone.`
            : ""
        }
        confirmLabel="Delete artifact"
        destructive
        isBusy={isDeleting}
        onCancel={() => setArtifactPendingDeletion(null)}
        onConfirm={() => {
          if (artifactPendingDeletion) handleDelete(artifactPendingDeletion.id);
        }}
      />
    </div>
  );
};
