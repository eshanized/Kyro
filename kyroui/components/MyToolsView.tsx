import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  FolderTree,
  FileText,
  Sparkles,
  Camera,
  Globe,
  Plug,
  Search,
  FolderOpen,
  Plus,
  Trash2,
  Copy,
  RefreshCw,
  GitCompare,
} from "lucide-react";
import {
  AuthorizedFolderRecord,
  DocumentSearchResult,
  DuplicateGroup,
  StudyNotesResult,
} from "../types/agent";

interface MyToolsViewProps {
  initialTab?: "files" | "study" | "screen" | "web" | "connected";
  onAskPrompt: (prompt: string) => void;
  onOpenConnections: () => void;
  onOpenScreenExplainer: () => void;
  onOpenFileOrganizer: () => void;
  onOpenDocAssistant: () => void;
}

export const MyToolsView: React.FC<MyToolsViewProps> = ({
  initialTab = "files",
  onAskPrompt,
  onOpenConnections,
  onOpenScreenExplainer,
  onOpenFileOrganizer,
  onOpenDocAssistant,
}) => {
  const [activeCategory, setActiveCategory] = useState<"files" | "study" | "screen" | "web" | "connected">(initialTab);

  // Files Tab State
  const [fileSearchQuery, setFileSearchQuery] = useState("");
  const [formatFilter, setFormatFilter] = useState("all");
  const [searchResults, setSearchResults] = useState<DocumentSearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [authorizedFolders, setAuthorizedFolders] = useState<AuthorizedFolderRecord[]>([]);
  const [newFolderPath, setNewFolderPath] = useState("");
  const [newFolderName, setNewFolderName] = useState("");
  const [duplicates, setDuplicates] = useState<DuplicateGroup[]>([]);
  const [isScanningDuplicates, setIsScanningDuplicates] = useState(false);
  const [filesNotice, setFilesNotice] = useState<string | null>(null);

  // Study Tab State
  const [studyDocPath, setStudyDocPath] = useState("");
  const [studyTopic, setStudyTopic] = useState("");
  const [studyNotes, setStudyNotes] = useState<StudyNotesResult | null>(null);
  const [isGeneratingNotes, setIsGeneratingNotes] = useState(false);
  const [revealedCardIndex, setRevealedCardIndex] = useState<number | null>(null);
  const [passageToExplain, setPassageToExplain] = useState("");
  const [passageExplanation, setPassageExplanation] = useState<string | null>(null);

  // Web Assistant Tab State
  const [webQuery, setWebQuery] = useState("");
  const [compareCriteria, setCompareCriteria] = useState("price, performance, warranty, battery life");
  const [compareItems, setCompareItems] = useState("");

  useEffect(() => {
    if (initialTab) {
      setActiveCategory(initialTab);
    }
  }, [initialTab]);

  useEffect(() => {
    loadAuthorizedFolders();
  }, []);

  const loadAuthorizedFolders = async () => {
    try {
      const res = await invoke<AuthorizedFolderRecord[]>("list_authorized_folders");
      setAuthorizedFolders(res);
    } catch {
      // fallback
    }
  };

  const handleAddFolder = async () => {
    if (!newFolderPath.trim()) return;
    try {
      await invoke<AuthorizedFolderRecord>("add_authorized_folder", {
        path: newFolderPath.trim(),
        displayName: newFolderName.trim() || null,
      });
      setNewFolderPath("");
      setNewFolderName("");
      setFilesNotice("Folder added to authorized locations.");
      await loadAuthorizedFolders();
    } catch (e: any) {
      setFilesNotice(`Failed to add folder: ${e?.toString() || "Unknown error"}`);
    }
  };

  const handleRemoveFolder = async (id: string) => {
    try {
      await invoke("remove_authorized_folder", { id });
      setFilesNotice("Folder removed from authorized locations.");
      await loadAuthorizedFolders();
    } catch (e: any) {
      setFilesNotice(`Failed to remove folder: ${e?.toString() || "Unknown error"}`);
    }
  };

  const handleSearchFiles = async () => {
    setIsSearching(true);
    setFilesNotice(null);
    try {
      const res = await invoke<DocumentSearchResult[]>("search_authorized_documents", {
        query: fileSearchQuery.trim(),
        folderFilter: null,
        formatFilter: formatFilter === "all" ? null : formatFilter,
      });
      setSearchResults(res);
      if (res.length === 0) {
        setFilesNotice("No matching documents found in authorized folders.");
      }
    } catch (e: any) {
      setFilesNotice(`Search error: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsSearching(false);
    }
  };

  const handleScanDuplicates = async () => {
    if (authorizedFolders.length === 0) {
      setFilesNotice("Please authorize at least one folder before scanning for duplicates.");
      return;
    }
    setIsScanningDuplicates(true);
    setFilesNotice(null);
    try {
      const target = authorizedFolders[0].path;
      const res = await invoke<DuplicateGroup[]>("detect_folder_duplicates", {
        folderPath: target,
      });
      setDuplicates(res);
      if (res.length === 0) {
        setFilesNotice(`No identical duplicate files found in '${target}'.`);
      }
    } catch (e: any) {
      setFilesNotice(`Duplicate scan error: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsScanningDuplicates(false);
    }
  };

  const handleGenerateStudyNotes = async () => {
    if (!studyDocPath.trim()) {
      return;
    }
    setIsGeneratingNotes(true);
    setStudyNotes(null);
    try {
      const res = await invoke<StudyNotesResult>("generate_study_notes", {
        path: studyDocPath.trim(),
        topic: studyTopic.trim() || null,
      });
      setStudyNotes(res);
    } catch (e: any) {
      alert(`Failed to generate study notes: ${e?.toString() || "Unknown error"}`);
    } finally {
      setIsGeneratingNotes(false);
    }
  };

  const handleExplainPassage = async () => {
    if (!studyDocPath.trim() || !passageToExplain.trim()) return;
    try {
      const res = await invoke<string>("explain_document_passage", {
        path: studyDocPath.trim(),
        passage: passageToExplain.trim(),
      });
      setPassageExplanation(res);
    } catch (e: any) {
      alert(`Error explaining passage: ${e?.toString() || "Unknown error"}`);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto p-6 max-w-6xl mx-auto w-full space-y-6">
      {/* Header */}
      <div className="pb-4 border-b border-neutral-800">
        <h1 className="text-2xl font-bold text-neutral-100">My Tools</h1>
        <p className="text-xs text-neutral-400 mt-1">
          Goal-oriented capabilities for everyday personal tasks without technical configuration.
        </p>
      </div>

      {/* Category Tabs */}
      <div className="flex items-center gap-1.5 p-1 bg-neutral-900 border border-neutral-800 rounded-xl overflow-x-auto">
        <button
          onClick={() => setActiveCategory("files")}
          className={`px-3.5 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition shrink-0 ${
            activeCategory === "files"
              ? "bg-[#1f1f1f] text-neutral-100 shadow-sm border border-neutral-700/60"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          <FolderOpen className="w-4 h-4 text-amber-400" />
          Personal Files & Docs
        </button>
        <button
          onClick={() => setActiveCategory("study")}
          className={`px-3.5 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition shrink-0 ${
            activeCategory === "study"
              ? "bg-[#1f1f1f] text-neutral-100 shadow-sm border border-neutral-700/60"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          <Sparkles className="w-4 h-4 text-purple-400" />
          Study & Revision Notes
        </button>
        <button
          onClick={() => setActiveCategory("screen")}
          className={`px-3.5 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition shrink-0 ${
            activeCategory === "screen"
              ? "bg-[#1f1f1f] text-neutral-100 shadow-sm border border-neutral-700/60"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          <Camera className="w-4 h-4 text-sky-400" />
          Screen & Error Help
        </button>
        <button
          onClick={() => setActiveCategory("web")}
          className={`px-3.5 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition shrink-0 ${
            activeCategory === "web"
              ? "bg-[#1f1f1f] text-neutral-100 shadow-sm border border-neutral-700/60"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          <Globe className="w-4 h-4 text-sky-400" />
          Web Research & Compare
        </button>
        <button
          onClick={() => setActiveCategory("connected")}
          className={`px-3.5 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition shrink-0 ${
            activeCategory === "connected"
              ? "bg-[#1f1f1f] text-neutral-100 shadow-sm border border-neutral-700/60"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          <Plug className="w-4 h-4 text-rose-400" />
          Connected Services
        </button>
      </div>

      {filesNotice && (
        <div className="p-3 rounded-lg bg-neutral-900 border border-neutral-800 text-neutral-200 text-xs flex items-center justify-between">
          <span>{filesNotice}</span>
          <button onClick={() => setFilesNotice(null)} className="text-neutral-400 hover:text-neutral-200 text-xs">
            Dismiss
          </button>
        </div>
      )}

      {/* CATEGORY 1: PERSONAL FILES & DOCUMENTS */}
      {activeCategory === "files" && (
        <div className="space-y-6">
          {/* Quick Launchers */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <button
              onClick={onOpenFileOrganizer}
              className="p-4 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-amber-800/40 text-left transition group shadow-sm flex items-start gap-3"
            >
              <div className="p-2 rounded-lg bg-amber-950/40 text-amber-400 border border-amber-800/40 shrink-0">
                <FolderTree className="w-5 h-5" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-100 group-hover:text-white">
                  Safe File Organizer
                </h3>
                <p className="text-xs text-neutral-400 mt-1">
                  Categorize loose files into neat folders with an exact preview and one-click reversible undo.
                </p>
              </div>
            </button>

            <button
              onClick={handleScanDuplicates}
              disabled={isScanningDuplicates}
              className="p-4 rounded-xl bg-[#141414] hover:bg-[#1a1a1a] border border-neutral-800 text-left transition group shadow-sm flex items-start gap-3"
            >
              <div className="p-2 rounded-lg bg-purple-950/40 text-purple-400 border border-purple-800/40 shrink-0">
                <Copy className="w-5 h-5" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-neutral-100 group-hover:text-white">
                  Detect Duplicate Files
                </h3>
                <p className="text-xs text-neutral-400 mt-1">
                  Find exact identical files using SHA-256 cryptographic verification before recommending cleanup.
                </p>
              </div>
            </button>
          </div>

          {/* Search Authorized Folders */}
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-4">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2">
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Natural-Language Document Search</h3>
                <p className="text-[11px] text-neutral-400">
                  Search inside document contents, filenames, and extracted dates across authorized folders.
                </p>
              </div>
              <div className="flex items-center gap-2">
                <select
                  value={formatFilter}
                  onChange={(e) => setFormatFilter(e.target.value)}
                  className="px-2.5 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-300"
                >
                  <option value="all">All Formats</option>
                  <option value="pdf">PDF Documents</option>
                  <option value="text">Plain Text / Notes</option>
                  <option value="markdown">Markdown</option>
                  <option value="csv">Spreadsheets / CSV</option>
                </select>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <div className="relative flex-1">
                <Search className="w-4 h-4 text-neutral-400 absolute left-3 top-1/2 -translate-y-1/2" />
                <input
                  type="text"
                  value={fileSearchQuery}
                  onChange={(e) => setFileSearchQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleSearchFiles()}
                  placeholder="Search keywords, filenames, or content (e.g. 'consulting invoice', 'syllabus', 'receipt')..."
                  className="w-full pl-9 pr-4 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none focus:border-neutral-600"
                />
              </div>
              <button
                onClick={handleSearchFiles}
                disabled={isSearching}
                className="px-4 py-2 bg-amber-950/40 hover:bg-amber-950/60 border border-amber-800/60 rounded-lg text-xs font-semibold text-amber-200 flex items-center gap-1.5 transition"
              >
                {isSearching ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Search className="w-3.5 h-3.5" />}
                Search
              </button>
            </div>

            {/* Results */}
            {searchResults.length > 0 && (
              <div className="space-y-2 pt-2">
                <div className="flex items-center justify-between text-[11px] text-neutral-400">
                  <span>Found {searchResults.length} matching files:</span>
                </div>
                <div className="space-y-2 max-h-72 overflow-y-auto">
                  {searchResults.map((item, idx) => (
                    <div
                      key={idx}
                      className="p-3 rounded-lg bg-[#181818] border border-neutral-800/80 flex flex-col gap-1.5"
                    >
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          <FileText className="w-4 h-4 text-neutral-400" />
                          <span className="text-xs font-semibold text-neutral-200">{item.file_name}</span>
                          <span className="text-[10px] bg-neutral-900 px-1.5 py-0.5 rounded border border-neutral-800 text-neutral-400 uppercase font-mono">
                            {item.format}
                          </span>
                        </div>
                        <div className="flex items-center gap-2">
                          <button
                            onClick={() => {
                              setStudyDocPath(item.path);
                              setActiveCategory("study");
                            }}
                            className="text-[11px] text-purple-400 hover:text-purple-300"
                          >
                            Study Notes
                          </button>
                          <button
                            onClick={() => onAskPrompt(`Explain and summarize this file: ${item.path}`)}
                            className="px-2 py-0.5 bg-neutral-800 hover:bg-neutral-700 text-[10px] text-neutral-200 rounded"
                          >
                            Summarize
                          </button>
                        </div>
                      </div>
                      <p className="text-[10px] text-neutral-400 font-mono truncate">{item.path}</p>
                      {item.snippet && (
                        <p className="text-[11px] text-neutral-300 bg-neutral-900/60 p-2 rounded border border-neutral-800/60 italic leading-snug">
                          {item.snippet}
                        </p>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>

          {/* Duplicate Evidence Results */}
          {duplicates.length > 0 && (
            <div className="p-4 rounded-xl bg-[#121212] border border-purple-800/40 space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-sm font-semibold text-purple-200">Duplicate Files Detected</h3>
                  <p className="text-xs text-neutral-400">Cryptographically identical SHA-256 matches</p>
                </div>
                <span className="text-xs font-mono bg-purple-950/60 px-2 py-0.5 rounded text-purple-300 border border-purple-800/60">
                  {duplicates.length} duplicate groups
                </span>
              </div>

              <div className="space-y-3">
                {duplicates.map((grp, idx) => (
                  <div key={idx} className="p-3 rounded-lg bg-[#181818] border border-neutral-800 space-y-2">
                    <div className="flex items-center justify-between text-xs">
                      <span className="font-semibold text-neutral-200">{grp.evidence || "Duplicate"}</span>
                      <span className="text-neutral-400 font-mono text-[10px]">{(grp.file_hash || "").slice(0, 16)}...</span>
                    </div>
                    <div className="space-y-1">
                      {grp.files.map((f, fIdx) => (
                        <div
                          key={fIdx}
                          className="flex items-center justify-between p-1.5 rounded bg-neutral-900 text-[11px]"
                        >
                          <span className="text-neutral-300 font-mono truncate pr-2">{f.path}</span>
                          <span className="text-[10px] text-neutral-400 shrink-0">
                            {f.is_original ? "Original" : "Duplicate Copy"}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Authorized Folders Configuration */}
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-3">
            <div>
              <h3 className="text-sm font-semibold text-neutral-200">Authorized Folders</h3>
              <p className="text-xs text-neutral-400">
                Kyro only inspects files inside folders you explicitly approve. System roots are never touched.
              </p>
            </div>

            <div className="flex flex-col sm:flex-row items-center gap-2">
              <input
                type="text"
                value={newFolderPath}
                onChange={(e) => setNewFolderPath(e.target.value)}
                placeholder="Folder path (e.g. /home/user/Documents or C:\Users\user\Downloads)"
                className="flex-1 px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
              />
              <input
                type="text"
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                placeholder="Optional Label (e.g. My Documents)"
                className="w-full sm:w-44 px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
              />
              <button
                onClick={handleAddFolder}
                className="w-full sm:w-auto px-3.5 py-1.5 bg-neutral-800 hover:bg-neutral-700 text-xs font-semibold text-neutral-200 rounded-lg flex items-center justify-center gap-1.5 transition"
              >
                <Plus className="w-3.5 h-3.5" />
                Authorize
              </button>
            </div>

            <div className="space-y-1.5 pt-1">
              {authorizedFolders.length > 0 ? (
                authorizedFolders.map((f) => (
                  <div
                    key={f.id}
                    className="p-2.5 rounded-lg bg-[#181818] border border-neutral-800/80 flex items-center justify-between text-xs"
                  >
                    <div>
                      <span className="font-semibold text-neutral-200 mr-2">{f.display_name}</span>
                      <span className="font-mono text-neutral-400 text-[11px]">{f.path}</span>
                    </div>
                    <button
                      onClick={() => handleRemoveFolder(f.id)}
                      className="text-neutral-400 hover:text-rose-400 p-1 transition"
                      title="Revoke folder authorization"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))
              ) : (
                <p className="text-xs text-neutral-400 py-2">
                  No explicit authorized folders configured yet. Kyro safely checks standard user Documents/Downloads.
                </p>
              )}
            </div>
          </div>
        </div>
      )}

      {/* CATEGORY 2: STUDY & REVISION NOTES */}
      {activeCategory === "study" && (
        <div className="space-y-6">
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-4">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Study Notes & Revision Flashcards</h3>
                <p className="text-xs text-neutral-400">
                  Turn lengthy PDF documents, syllabus files, and articles into clear explanations, key dates, and question/answer study cards.
                </p>
              </div>
              <button
                onClick={onOpenDocAssistant}
                className="px-3 py-1.5 rounded-lg bg-neutral-900 hover:bg-neutral-800 text-neutral-300 border border-neutral-700 text-xs flex items-center gap-1.5 transition shrink-0"
              >
                <FileText className="w-3.5 h-3.5" /> Full Assistant
              </button>
            </div>

            <div className="space-y-2">
              <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
                <input
                  type="text"
                  value={studyDocPath}
                  onChange={(e) => setStudyDocPath(e.target.value)}
                  placeholder="Enter path to document or PDF file (e.g. /home/user/Documents/syllabus.pdf)..."
                  className="flex-1 px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
                />
                <input
                  type="text"
                  value={studyTopic}
                  onChange={(e) => setStudyTopic(e.target.value)}
                  placeholder="Focus topic (optional, e.g. 'Chapter 3' or 'Final Exam')..."
                  className="w-full sm:w-64 px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
                />
                <button
                  onClick={handleGenerateStudyNotes}
                  disabled={isGeneratingNotes}
                  className="px-4 py-2 bg-purple-950/50 hover:bg-purple-900/50 border border-purple-800/60 rounded-lg text-xs font-semibold text-purple-200 flex items-center justify-center gap-1.5 transition shrink-0"
                >
                  {isGeneratingNotes ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Sparkles className="w-3.5 h-3.5 text-purple-300" />}
                  Generate Notes
                </button>
              </div>
            </div>

            {/* Study Notes Outcome */}
            {studyNotes && (
              <div className="space-y-4 pt-3 border-t border-neutral-800">
                <div className="p-3 rounded-lg bg-[#181818] border border-neutral-800">
                  <h4 className="text-xs font-bold text-neutral-100 uppercase tracking-wider mb-1">
                    Overview: {studyNotes.doc_name}
                  </h4>
                  <p className="text-xs text-neutral-300 leading-relaxed">{studyNotes.overview}</p>
                </div>

                {/* Key Dates & Milestones */}
                {(studyNotes.key_dates_and_milestones || []).length > 0 && (
                  <div className="space-y-1.5">
                    <h5 className="text-xs font-semibold text-neutral-300">Important Dates & Deadlines</h5>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {(studyNotes.key_dates_and_milestones || []).map((d, dIdx) => (
                        <div key={dIdx} className="p-2 rounded bg-neutral-900 border border-neutral-800 text-xs text-neutral-200">
                          {d}
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Revision Flashcards */}
                {(studyNotes.revision_flashcards || []).length > 0 && (
                  <div className="space-y-2">
                    <h5 className="text-xs font-semibold text-neutral-300">
                      Revision Flashcards ({(studyNotes.revision_flashcards || []).length})
                    </h5>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                      {(studyNotes.revision_flashcards || []).map((fc, idx) => (
                        <div
                          key={idx}
                          className="p-3 rounded-xl bg-[#161616] border border-neutral-800 flex flex-col justify-between space-y-2"
                        >
                          <span className="text-xs font-medium text-neutral-200">{fc.question}</span>
                          {revealedCardIndex === idx ? (
                            <div className="p-2 rounded bg-neutral-900 border border-neutral-700/60 text-xs text-purple-200">
                              {fc.answer}
                            </div>
                          ) : (
                            <button
                              onClick={() => setRevealedCardIndex(idx)}
                              className="py-1 px-2.5 rounded bg-neutral-800 hover:bg-neutral-700 text-[11px] font-medium text-neutral-300 text-center transition"
                            >
                              Reveal Answer
                            </button>
                          )}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Simple Passage Explainer */}
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-3">
            <div>
              <h3 className="text-sm font-semibold text-neutral-200">Explain Difficult Passage</h3>
              <p className="text-xs text-neutral-400">
                Paste any complex legal clause, medical term, or technical sentence to understand it in plain English.
              </p>
            </div>
            <textarea
              value={passageToExplain}
              onChange={(e) => setPassageToExplain(e.target.value)}
              placeholder="Paste the confusing sentence or paragraph here..."
              rows={3}
              className="w-full p-2.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
            />
            <button
              onClick={handleExplainPassage}
              className="px-3.5 py-1.5 bg-neutral-800 hover:bg-neutral-700 rounded-lg text-xs font-medium text-neutral-200 transition"
            >
              Explain Simply
            </button>
            {passageExplanation && (
              <div className="p-3 rounded-lg bg-[#181818] border border-neutral-700/60 text-xs text-neutral-200 whitespace-pre-line leading-relaxed">
                {passageExplanation}
              </div>
            )}
          </div>
        </div>
      )}

      {/* CATEGORY 3: SCREEN & ERROR HELP */}
      {activeCategory === "screen" && (
        <div className="space-y-6">
          <div className="p-6 rounded-xl bg-[#141414] border border-sky-800/40 text-center space-y-4 max-w-xl mx-auto">
            <div className="p-3 rounded-full bg-sky-950/60 text-sky-400 w-12 h-12 mx-auto flex items-center justify-center border border-sky-800/60">
              <Camera className="w-6 h-6" />
            </div>
            <div>
              <h3 className="text-base font-bold text-neutral-100">Screen Understanding & Error Diagnostics</h3>
              <p className="text-xs text-neutral-400 mt-1 max-w-md mx-auto">
                Capture your current display snapshot, locate buttons, diagnose error dialogs, and get clear step-by-step guidance.
              </p>
            </div>
            <div className="flex items-center justify-center gap-3 pt-2">
              <button
                onClick={onOpenScreenExplainer}
                className="px-4 py-2 bg-sky-950/60 hover:bg-sky-900/60 border border-sky-700/60 rounded-lg text-xs font-semibold text-sky-200 flex items-center gap-2 transition shadow-sm"
              >
                <Camera className="w-4 h-4" />
                Launch Screen Explainer
              </button>
              <button
                onClick={() => onAskPrompt("Explain the error on my screen and help me fix it")}
                className="px-4 py-2 bg-neutral-800 hover:bg-neutral-700 rounded-lg text-xs font-medium text-neutral-200 transition"
              >
                "Help me fix this error"
              </button>
            </div>
          </div>
        </div>
      )}

      {/* CATEGORY 4: WEB RESEARCH & COMPARE */}
      {activeCategory === "web" && (
        <div className="space-y-6">
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-4">
            <div>
              <h3 className="text-sm font-semibold text-neutral-200">Guided Web Research</h3>
              <p className="text-xs text-neutral-400">
                Research questions across multiple trusted sources and get verified citations distinguishing actual page content from model analysis.
              </p>
            </div>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={webQuery}
                onChange={(e) => setWebQuery(e.target.value)}
                placeholder="What topic or question would you like to research? (e.g. 'Latest NASA Artemis mission updates')"
                className="flex-1 px-3 py-2 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
              />
              <button
                onClick={() => {
                  if (webQuery.trim()) {
                    onAskPrompt(`Research this question across multiple verified web sources and provide citations: ${webQuery.trim()}`);
                  }
                }}
                className="px-4 py-2 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-xs font-semibold text-neutral-200 flex items-center gap-1.5 transition"
              >
                <Globe className="w-3.5 h-3.5 text-neutral-300" />
                Research
              </button>
            </div>
          </div>

          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-3">
            <div>
              <h3 className="text-sm font-semibold text-neutral-200">Product & Price Comparison</h3>
              <p className="text-xs text-neutral-400">
                Compare products or services based on visible, user-defined criteria without invented ratings or prices.
              </p>
            </div>
            <div className="space-y-2">
              <input
                type="text"
                value={compareItems}
                onChange={(e) => setCompareItems(e.target.value)}
                placeholder="Products to compare (e.g. 'Sony WH-1000XM5 vs Bose QuietComfort Ultra')"
                className="w-full px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
              />
              <input
                type="text"
                value={compareCriteria}
                onChange={(e) => setCompareCriteria(e.target.value)}
                placeholder="Comparison criteria (comma-separated, e.g. price, battery life, noise cancellation)"
                className="w-full px-3 py-1.5 bg-neutral-900 border border-neutral-800 rounded-lg text-xs text-neutral-200 placeholder-neutral-400 focus:outline-none"
              />
              <button
                onClick={() => {
                  if (compareItems.trim()) {
                    onAskPrompt(`Compare these items based on ${compareCriteria}: ${compareItems.trim()}`);
                  }
                }}
                className="px-4 py-2 bg-neutral-800 hover:bg-neutral-700 rounded-lg text-xs font-medium text-neutral-200 flex items-center gap-1.5 transition"
              >
                <GitCompare className="w-3.5 h-3.5" />
                Build Comparison Table
              </button>
            </div>
          </div>
        </div>
      )}

      {/* CATEGORY 5: CONNECTED SERVICES */}
      {activeCategory === "connected" && (
        <div className="space-y-6">
          <div className="p-4 rounded-xl bg-[#121212] border border-neutral-800 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-semibold text-neutral-200">Account Integrations</h3>
                <p className="text-xs text-neutral-400">
                  Connect third-party accounts safely with scoped OAuth permissions and encrypted vault token storage.
                </p>
              </div>
              <button
                onClick={onOpenConnections}
                className="px-3 py-1.5 bg-neutral-800 hover:bg-neutral-700 rounded-lg text-xs font-medium text-neutral-200 flex items-center gap-1.5 transition"
              >
                <Plug className="w-3.5 h-3.5" />
                Manage Connections
              </button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
              <div className="p-3.5 rounded-xl bg-[#161616] border border-neutral-800 flex flex-col justify-between space-y-2">
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs font-bold text-neutral-200">Google Workspace</span>
                    <span className="text-[10px] text-neutral-400 font-mono">Gmail & Calendar</span>
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    Read unread emails, safely draft replies, view upcoming events, and propose meeting times.
                  </p>
                </div>
                <button
                  onClick={onOpenConnections}
                  className="w-full py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-xs text-neutral-200 font-medium transition"
                >
                  Configure / Connect
                </button>
              </div>

              <div className="p-3.5 rounded-xl bg-[#161616] border border-neutral-800 flex flex-col justify-between space-y-2">
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs font-bold text-neutral-200">GitHub</span>
                    <span className="text-[10px] text-neutral-400 font-mono">Issues & Actions</span>
                  </div>
                  <p className="text-[11px] text-neutral-400">
                    List open issues, pull requests, and CI workflow runs with read-only defaults.
                  </p>
                </div>
                <button
                  onClick={onOpenConnections}
                  className="w-full py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-xs text-neutral-200 font-medium transition"
                >
                  Configure / Connect
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
