import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  FileText,
  Calendar,
  CheckSquare,
  Shield,
  GitCompare,
  X,
  FileCheck,
  Search,
  Sparkles,
  Layers,
  Database,
  RefreshCw,
  Trash2,
  Copy,
  Check,
} from "lucide-react";
import {
  DocumentAnalysisResult,
  DocumentComparisonResult,
  RagStatus,
  RetrievalResult,
} from "../types/agent";

interface DocumentAssistantModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const DocumentAssistantModal: React.FC<DocumentAssistantModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [activeTab, setActiveTab] = useState<"analyze" | "compare" | "rag">("analyze");
  const [docPath, setDocPath] = useState("");
  const [docPathB, setDocPathB] = useState("");
  const [loading, setLoading] = useState(false);
  const [analysis, setAnalysis] = useState<DocumentAnalysisResult | null>(null);
  const [comparison, setComparison] = useState<DocumentComparisonResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // RAG state
  const [ragStatus, setRagStatus] = useState<RagStatus | null>(null);
  const [ragQuery, setRagQuery] = useState("");
  const [ragResults, setRagResults] = useState<RetrievalResult[]>([]);
  const [isSearchingRag, setIsSearchingRag] = useState(false);
  const [isIndexingChunks, setIsIndexingChunks] = useState(false);
  const [copiedCitation, setCopiedCitation] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen && activeTab === "rag") {
      loadRagStatus();
    }
  }, [isOpen, activeTab]);

  const loadRagStatus = async () => {
    try {
      const res = await invoke<RagStatus>("get_rag_status");
      setRagStatus(res);
    } catch (e: any) {
      console.error("Failed to load RAG status:", e);
    }
  };

  const handleIndexChunks = async () => {
    setIsIndexingChunks(true);
    setError(null);
    setSuccess(null);
    try {
      const count = await invoke<number>("index_authorized_chunks");
      await loadRagStatus();
      setSuccess(`Successfully indexed ${count} format-aware chunks from authorized documents.`);
    } catch (e: any) {
      setError(e?.toString() || "Failed to index chunks.");
    } finally {
      setIsIndexingChunks(false);
    }
  };

  const handleSearchRag = async () => {
    if (!ragQuery.trim()) return;
    setIsSearchingRag(true);
    setError(null);
    try {
      const results = await invoke<RetrievalResult[]>("search_deep_rag", {
        query: ragQuery.trim(),
        maxResults: 10,
      });
      setRagResults(results);
      if (results.length === 0) {
        setSuccess("No matching chunks found for this query in authorized folders.");
      } else {
        setSuccess(`Found ${results.length} relevant chunk(s).`);
      }
    } catch (e: any) {
      setError(e?.toString() || "Search failed.");
    } finally {
      setIsSearchingRag(false);
    }
  };

  const handleClearRagIndex = async () => {
    try {
      await invoke("clear_rag_index");
      await loadRagStatus();
      setRagResults([]);
      setSuccess("Chunk index cleared. You can re-index anytime.");
    } catch (e: any) {
      setError(e?.toString() || "Failed to clear chunk index.");
    }
  };

  if (!isOpen) return null;

  const handleAnalyze = async () => {
    if (!docPath.trim()) {
      setError("Please specify a document path.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const res = await invoke<DocumentAnalysisResult>("analyze_document", {
        path: docPath.trim(),
        model: null,
      });
      setAnalysis(res);
      setSuccess(`Analyzed ${res.metadata.file_name} (${res.metadata.format.toUpperCase()})`);
    } catch (e: any) {
      setError(e?.toString() || "Failed to analyze document.");
    } finally {
      setLoading(false);
    }
  };

  const handleCompare = async () => {
    if (!docPath.trim() || !docPathB.trim()) {
      setError("Please specify paths for both Document A and Document B.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const res = await invoke<DocumentComparisonResult>("compare_documents", {
        pathA: docPath.trim(),
        pathB: docPathB.trim(),
      });
      setComparison(res);
      setSuccess("Comparison report generated and saved as artifact.");
    } catch (e: any) {
      setError(e?.toString() || "Failed to compare documents.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-4xl max-h-[85vh] flex flex-col rounded-xl border border-zinc-700 bg-zinc-900 shadow-2xl text-zinc-100 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-sky-500/10 text-sky-400">
              <FileText className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold">Local Document Assistant</h2>
              <p className="text-xs text-zinc-400">
                Text & PDF extraction, date analysis, action items, and comparison
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tab Controls */}
        <div className="flex border-b border-zinc-800 px-6 bg-zinc-950/60">
          <button
            onClick={() => setActiveTab("analyze")}
            className={`flex items-center space-x-2 py-3 px-4 border-b-2 text-xs font-medium transition cursor-pointer ${
              activeTab === "analyze"
                ? "border-sky-500 text-sky-400"
                : "border-transparent text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <Search className="w-4 h-4" />
            <span>Analyze & Extract</span>
          </button>
          <button
            onClick={() => setActiveTab("compare")}
            className={`flex items-center space-x-2 py-3 px-4 border-b-2 text-xs font-medium transition cursor-pointer ${
              activeTab === "compare"
                ? "border-sky-500 text-sky-400"
                : "border-transparent text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <GitCompare className="w-4 h-4" />
            <span>Compare Two Documents</span>
          </button>
          <button
            onClick={() => setActiveTab("rag")}
            className={`flex items-center space-x-2 py-3 px-4 border-b-2 text-xs font-medium transition cursor-pointer ${
              activeTab === "rag"
                ? "border-sky-500 text-sky-400"
                : "border-transparent text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <Layers className="w-4 h-4" />
            <span>Deep Local RAG & Chunks</span>
          </button>
        </div>

        {/* Input Bar */}
        <div className="p-6 border-b border-zinc-800/80 bg-zinc-900/40 space-y-3">
          {activeTab === "analyze" ? (
            <div className="flex gap-3">
              <input
                type="text"
                placeholder="Enter path to text document or PDF file (e.g. /home/user/docs/spec.pdf)"
                value={docPath}
                onChange={(e) => setDocPath(e.target.value)}
                className="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-sky-500 transition"
              />
              <button
                onClick={handleAnalyze}
                disabled={loading || !docPath.trim()}
                className="flex items-center space-x-2 bg-sky-600 hover:bg-sky-500 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-sm font-medium transition cursor-pointer"
              >
                <Sparkles className="w-4 h-4" />
                <span>{loading ? "Parsing..." : "Analyze"}</span>
              </button>
            </div>
          ) : activeTab === "compare" ? (
            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3">
                <input
                  type="text"
                  placeholder="Document A path (original)"
                  value={docPath}
                  onChange={(e) => setDocPath(e.target.value)}
                  className="bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-xs text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-sky-500"
                />
                <input
                  type="text"
                  placeholder="Document B path (revised)"
                  value={docPathB}
                  onChange={(e) => setDocPathB(e.target.value)}
                  className="bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-xs text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-sky-500"
                />
              </div>
              <div className="flex justify-end">
                <button
                  onClick={handleCompare}
                  disabled={loading || !docPath.trim() || !docPathB.trim()}
                  className="flex items-center space-x-2 bg-sky-600 hover:bg-sky-500 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-xs font-medium transition cursor-pointer"
                >
                  <GitCompare className="w-4 h-4" />
                  <span>{loading ? "Comparing..." : "Generate Comparison Report"}</span>
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="flex gap-3">
                <input
                  type="text"
                  placeholder="Search code symbols, functions, notes, error messages, and docs across authorized folders..."
                  value={ragQuery}
                  onChange={(e) => setRagQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleSearchRag()}
                  className="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-sky-500 transition"
                />
                <button
                  onClick={handleSearchRag}
                  disabled={isSearchingRag || !ragQuery.trim()}
                  className="flex items-center space-x-2 bg-sky-600 hover:bg-sky-500 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-sm font-medium transition cursor-pointer"
                >
                  <Search className="w-4 h-4" />
                  <span>{isSearchingRag ? "Searching..." : "Search"}</span>
                </button>
              </div>
              <div className="flex items-center justify-between text-xs text-zinc-400">
                <div className="flex items-center gap-2">
                  <span className="flex items-center gap-1 text-zinc-300">
                    <Database className="w-3.5 h-3.5 text-sky-400" />
                    {ragStatus?.total_chunks_indexed || 0} chunks in index
                  </span>
                  <span>•</span>
                  <span>Mode: {ragStatus?.embedding_mode || "lexical_only"}</span>
                  <span>•</span>
                  <span>{ragStatus?.authorized_folder_count || 0} authorized folder(s)</span>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={handleIndexChunks}
                    disabled={isIndexingChunks}
                    className="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded text-xs flex items-center gap-1 transition"
                  >
                    <RefreshCw className={`w-3 h-3 ${isIndexingChunks ? "animate-spin" : ""}`} />
                    <span>{isIndexingChunks ? "Indexing..." : "Reindex Chunks"}</span>
                  </button>
                  <button
                    onClick={handleClearRagIndex}
                    className="px-2.5 py-1 bg-zinc-800 hover:bg-red-950/60 hover:text-red-300 text-zinc-400 rounded text-xs flex items-center gap-1 transition"
                  >
                    <Trash2 className="w-3 h-3" />
                    <span>Clear Index</span>
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Notices */}
        {error && (
          <div className="mx-6 mt-4 p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-300 text-xs">
            {error}
          </div>
        )}
        {success && (
          <div className="mx-6 mt-4 p-3 rounded-lg bg-sky-500/10 border border-sky-500/30 text-sky-300 text-xs">
            {success}
          </div>
        )}

        {/* Content View */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {activeTab === "analyze" && analysis && (
            <div className="space-y-5">
              {/* Privacy Disclosure Banner */}
              <div className="flex items-center space-x-2 p-3 rounded-lg bg-zinc-950 border border-zinc-800 text-xs text-zinc-400">
                <Shield className="w-4 h-4 text-green-400 shrink-0" />
                <span>{analysis.privacy_disclosure}</span>
              </div>

              {/* Summary Card */}
              <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-medium text-zinc-400">Document Summary</span>
                  <span className="text-[11px] px-2 py-0.5 rounded bg-sky-500/10 text-sky-400 border border-sky-500/20 uppercase font-mono">
                    {analysis.metadata.format} • {analysis.metadata.word_count} words
                  </span>
                </div>
                <p className="text-sm text-zinc-200 leading-relaxed">{analysis.summary}</p>
              </div>

              {/* Action Items */}
              <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800 space-y-3">
                <div className="flex items-center space-x-2 text-xs font-semibold text-zinc-300">
                  <CheckSquare className="w-4 h-4 text-green-400" />
                  <span>Action Items & Tasks ({analysis.action_items.length})</span>
                </div>
                {analysis.action_items.length > 0 ? (
                  <ul className="space-y-2 text-xs">
                    {analysis.action_items.map((act, idx) => (
                      <li key={idx} className="flex items-start space-x-2 text-zinc-300">
                        <span className="mt-0.5 w-1.5 h-1.5 rounded-full bg-green-400 shrink-0" />
                        <span className="flex-1">{act.text}</span>
                        {act.priority === "high" && (
                          <span className="text-[10px] uppercase font-bold text-red-400 px-1.5 py-0.5 rounded bg-red-400/10">
                            High Priority
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="text-xs text-zinc-500">No explicit action items found in document.</p>
                )}
              </div>

              {/* Dates & Deadlines */}
              <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800 space-y-3">
                <div className="flex items-center space-x-2 text-xs font-semibold text-zinc-300">
                  <Calendar className="w-4 h-4 text-amber-400" />
                  <span>Dates & Deadlines ({analysis.extracted_dates.length})</span>
                </div>
                {analysis.extracted_dates.length > 0 ? (
                  <div className="grid grid-cols-2 gap-2 text-xs">
                    {analysis.extracted_dates.map((d, idx) => (
                      <div key={idx} className="p-2.5 rounded-lg bg-zinc-900/60 border border-zinc-800/80">
                        <div className="font-mono text-amber-400 font-medium">{d.raw_date}</div>
                        <div className="text-zinc-400 text-[11px] truncate mt-1">"{d.context}"</div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-zinc-500">No explicit calendar dates found in document.</p>
                )}
              </div>
            </div>
          )}

          {activeTab === "compare" && comparison && (
            <div className="space-y-4">
              <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800">
                <h3 className="text-xs font-medium text-zinc-400 mb-1">Comparison Overview</h3>
                <p className="text-sm text-zinc-200">{comparison.summary_differences}</p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800">
                  <h4 className="text-xs font-semibold text-green-400 mb-2">Added Content (Doc B)</h4>
                  <ul className="space-y-1 text-xs text-zinc-300 font-mono">
                    {comparison.added_in_b.map((line, i) => (
                      <li key={i} className="text-green-300 truncate">+ {line}</li>
                    ))}
                  </ul>
                </div>
                <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800">
                  <h4 className="text-xs font-semibold text-red-400 mb-2">Removed Content (Doc A)</h4>
                  <ul className="space-y-1 text-xs text-zinc-300 font-mono">
                    {comparison.removed_from_a.map((line, i) => (
                      <li key={i} className="text-red-300 truncate">- {line}</li>
                    ))}
                  </ul>
                </div>
              </div>
            </div>
          )}

          {activeTab === "rag" && (
            <div className="space-y-4">
              {ragResults.length === 0 ? (
                <div className="h-64 flex flex-col items-center justify-center text-center text-zinc-500 space-y-2">
                  <Layers className="w-10 h-10 mb-2 opacity-40 text-sky-400" />
                  <p className="text-sm text-zinc-300 font-medium">Deep Local RAG Engine</p>
                  <p className="text-xs text-zinc-500 max-w-md">
                    Search notes, Markdown docs, PDFs, and codebases across your authorized folders.
                    Click &ldquo;Reindex Chunks&rdquo; above to extract format-aware sections.
                  </p>
                </div>
              ) : (
                <div className="space-y-3">
                  <div className="flex items-center justify-between text-xs text-zinc-400 pb-1 border-b border-zinc-800">
                    <span>Ranked Results ({ragResults.length})</span>
                    <span className="text-[11px] text-zinc-500">Citations respect authorized folder scopes</span>
                  </div>
                  {ragResults.map((chunk) => {
                    const citation = `${chunk.document_path}${
                      chunk.start_line ? `#L${chunk.start_line}-L${chunk.end_line || chunk.start_line}` : ""
                    }`;

                    return (
                      <div
                        key={chunk.chunk_id}
                        className="p-4 rounded-xl bg-zinc-950 border border-zinc-800 space-y-2.5 text-xs transition hover:border-zinc-700"
                      >
                        <div className="flex items-center justify-between gap-3">
                          <div className="flex items-center gap-2 truncate">
                            <span className="font-mono text-sky-400 font-semibold truncate" title={chunk.document_path}>
                              {chunk.document_path.split("/").pop()}
                            </span>
                            {chunk.start_line && (
                              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-zinc-800 text-zinc-300">
                                L{chunk.start_line}{chunk.end_line && chunk.end_line !== chunk.start_line ? `-${chunk.end_line}` : ""}
                              </span>
                            )}
                            {chunk.heading && (
                              <span className="text-zinc-400 truncate max-w-[200px]" title={chunk.heading}>
                                • {chunk.heading}
                              </span>
                            )}
                          </div>
                          <div className="flex items-center gap-2 shrink-0">
                            <span className="px-2 py-0.5 rounded text-[10px] font-mono uppercase bg-sky-950/60 text-sky-300 border border-sky-500/30">
                              {chunk.match_type}
                            </span>
                            <span className="text-zinc-400 font-mono text-[11px]">
                              {(chunk.score * 100).toFixed(0)}%
                            </span>
                            <button
                              onClick={() => {
                                navigator.clipboard.writeText(citation);
                                setCopiedCitation(chunk.chunk_id);
                                setTimeout(() => setCopiedCitation(null), 2000);
                              }}
                              className="p-1 rounded text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition"
                              title="Copy citation path"
                            >
                              {copiedCitation === chunk.chunk_id ? (
                                <Check className="w-3.5 h-3.5 text-green-400" />
                              ) : (
                                <Copy className="w-3.5 h-3.5" />
                              )}
                            </button>
                          </div>
                        </div>
                        <div className="text-zinc-500 text-[11px] font-mono truncate">{chunk.document_path}</div>
                        <pre className="p-3 rounded-lg bg-zinc-900/80 border border-zinc-800/80 text-zinc-300 text-xs font-mono overflow-x-auto whitespace-pre-wrap leading-relaxed max-h-48">
                          {chunk.content}
                        </pre>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {activeTab !== "rag" && !analysis && !comparison && (
            <div className="h-64 flex flex-col items-center justify-center text-center text-zinc-500">
              <FileCheck className="w-10 h-10 mb-3 opacity-40" />
              <p className="text-sm">Select a document above to begin local analysis or comparison.</p>
              <p className="text-xs text-zinc-600 mt-1">Supports Markdown, Text, Code, CSV, and searchable PDFs.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
