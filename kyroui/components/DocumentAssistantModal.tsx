import React, { useState } from "react";
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
} from "lucide-react";
import {
  DocumentAnalysisResult,
  DocumentComparisonResult,
} from "../types/agent";

interface DocumentAssistantModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const DocumentAssistantModal: React.FC<DocumentAssistantModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [activeTab, setActiveTab] = useState<"analyze" | "compare">("analyze");
  const [docPath, setDocPath] = useState("");
  const [docPathB, setDocPathB] = useState("");
  const [loading, setLoading] = useState(false);
  const [analysis, setAnalysis] = useState<DocumentAnalysisResult | null>(null);
  const [comparison, setComparison] = useState<DocumentComparisonResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

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
          ) : (
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

          {!analysis && !comparison && (
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
