import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  FolderOpen,
  Play,
  RotateCcw,
  AlertTriangle,
  CheckCircle2,
  Copy,
  FileText,
  X,
  Layers,
} from "lucide-react";
import {
  OrganizationPlan,
  OrganizationTransaction,
  OrganizationRule,
} from "../types/agent";

interface FileOrganizerModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const FileOrganizerModal: React.FC<FileOrganizerModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [directory, setDirectory] = useState("");
  const [recursive, setRecursive] = useState(false);
  const [groupDuplicates, setGroupDuplicates] = useState(true);
  const [loading, setLoading] = useState(false);
  const [plan, setPlan] = useState<OrganizationPlan | null>(null);
  const [lastTransaction, setLastTransaction] =
    useState<OrganizationTransaction | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);

  if (!isOpen) return null;

  const handlePreview = async () => {
    if (!directory.trim()) {
      setError("Please specify a directory path to organize.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccessMessage(null);

    const rule: OrganizationRule = {
      name: "Standard Organization",
      custom_extensions: {},
      group_duplicates: groupDuplicates,
      duplicates_folder_name: "Duplicates",
      recursive,
    };

    try {
      const res = await invoke<OrganizationPlan>("preview_file_organization", {
        directory: directory.trim(),
        rule,
      });
      setPlan(res);
      if (res.operations.length === 0) {
        setSuccessMessage("Directory is already organized or contains no loose files.");
      }
    } catch (e: any) {
      setError(e?.toString() || "Failed to generate preview plan.");
    } finally {
      setLoading(false);
    }
  };

  const handleExecute = async () => {
    if (!plan || plan.operations.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const trx = await invoke<OrganizationTransaction>(
        "execute_file_organization",
        { plan }
      );
      setLastTransaction(trx);
      const completed = trx.operations.filter((o) => o.status === "completed").length;
      setSuccessMessage(
        `Successfully organized ${completed} files into category folders. Transaction manifest saved.`
      );
      setPlan(null);
    } catch (e: any) {
      setError(e?.toString() || "Failed to execute organization.");
    } finally {
      setLoading(false);
    }
  };

  const handleUndo = async () => {
    if (!lastTransaction) return;
    setLoading(true);
    setError(null);
    try {
      const undoneCount = await invoke<number>("undo_file_organization", {
        transaction: lastTransaction,
      });
      setSuccessMessage(`Successfully reversed ${undoneCount} file moves back to original locations.`);
      setLastTransaction(null);
    } catch (e: any) {
      setError(e?.toString() || "Failed to undo organization transaction.");
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
            <div className="p-2 rounded-lg bg-green-500/10 text-green-400">
              <FolderOpen className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold">Safe File Organizer</h2>
              <p className="text-xs text-zinc-400">
                Preview, deduplicate, categorize, and execute reversible file moves
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

        {/* Configuration Bar */}
        <div className="p-6 border-b border-zinc-800/80 bg-zinc-900/50 space-y-4">
          <div className="flex gap-3">
            <div className="relative flex-1">
              <input
                type="text"
                placeholder="Enter absolute directory path (e.g. /home/user/Downloads or ~/Desktop)"
                value={directory}
                onChange={(e) => setDirectory(e.target.value)}
                className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-green-500 transition"
              />
            </div>
            <button
              onClick={handlePreview}
              disabled={loading || !directory.trim()}
              className="flex items-center space-x-2 bg-green-600 hover:bg-green-500 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-sm font-medium transition cursor-pointer"
            >
              <Layers className="w-4 h-4" />
              <span>{loading ? "Scanning..." : "Scan & Preview"}</span>
            </button>
            {lastTransaction && (
              <button
                onClick={handleUndo}
                disabled={loading}
                className="flex items-center space-x-2 bg-amber-600/20 text-amber-300 hover:bg-amber-600/30 border border-amber-500/30 px-3.5 py-2 rounded-lg text-sm font-medium transition cursor-pointer"
              >
                <RotateCcw className="w-4 h-4" />
                <span>Undo Last Move</span>
              </button>
            )}
          </div>

          <div className="flex items-center space-x-6 text-xs text-zinc-300">
            <label className="flex items-center space-x-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={groupDuplicates}
                onChange={(e) => setGroupDuplicates(e.target.checked)}
                className="rounded border-zinc-700 bg-zinc-950 text-green-500 focus:ring-0"
              />
              <span>Detect & group duplicate files via SHA-256 hash</span>
            </label>
            <label className="flex items-center space-x-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={recursive}
                onChange={(e) => setRecursive(e.target.checked)}
                className="rounded border-zinc-700 bg-zinc-950 text-green-500 focus:ring-0"
              />
              <span>Scan subfolders recursively</span>
            </label>
          </div>
        </div>

        {/* Notices */}
        {error && (
          <div className="mx-6 mt-4 p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-300 text-xs flex items-center space-x-2">
            <AlertTriangle className="w-4 h-4 shrink-0 text-red-400" />
            <span>{error}</span>
          </div>
        )}
        {successMessage && (
          <div className="mx-6 mt-4 p-3 rounded-lg bg-green-500/10 border border-green-500/30 text-green-300 text-xs flex items-center space-x-2">
            <CheckCircle2 className="w-4 h-4 shrink-0 text-green-400" />
            <span>{successMessage}</span>
          </div>
        )}

        {/* Content Area */}
        <div className="flex-1 overflow-y-auto p-6">
          {plan ? (
            <div className="space-y-4">
              {/* Stat Cards */}
              <div className="grid grid-cols-3 gap-3">
                <div className="p-3 rounded-lg bg-zinc-950 border border-zinc-800">
                  <div className="text-xs text-zinc-400">Files to Organize</div>
                  <div className="text-xl font-bold text-zinc-100 mt-1">
                    {plan.total_scanned}
                  </div>
                </div>
                <div className="p-3 rounded-lg bg-zinc-950 border border-zinc-800">
                  <div className="text-xs text-zinc-400">Content Duplicates</div>
                  <div className="text-xl font-bold text-amber-400 mt-1">
                    {plan.duplicates_count}
                  </div>
                </div>
                <div className="p-3 rounded-lg bg-zinc-950 border border-zinc-800">
                  <div className="text-xs text-zinc-400">Potential Conflicts</div>
                  <div className="text-xl font-bold text-zinc-100 mt-1">
                    {plan.conflicts_count}
                  </div>
                </div>
              </div>

              {/* Operations Table */}
              <div className="rounded-lg border border-zinc-800 overflow-hidden">
                <table className="w-full text-left text-xs">
                  <thead className="bg-zinc-950 text-zinc-400 border-b border-zinc-800 font-medium">
                    <tr>
                      <th className="py-2.5 px-3">File Name</th>
                      <th className="py-2.5 px-3">Category Folder</th>
                      <th className="py-2.5 px-3">Status / Reason</th>
                      <th className="py-2.5 px-3">Size</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-800/60">
                    {plan.operations.map((op, idx) => {
                      const basename =
                        op.source_path.split("/").pop() || op.source_path;
                      return (
                        <tr
                          key={idx}
                          className="hover:bg-zinc-800/30 transition"
                        >
                          <td className="py-2 px-3 font-mono text-zinc-200 truncate max-w-xs">
                            {basename}
                          </td>
                          <td className="py-2 px-3">
                            <span className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium bg-zinc-800 text-zinc-300 border border-zinc-700">
                              {op.category}
                            </span>
                          </td>
                          <td className="py-2 px-3 text-zinc-400">
                            {op.conflict ? (
                              <span className="text-red-400 flex items-center space-x-1">
                                <AlertTriangle className="w-3.5 h-3.5" />
                                <span>{op.conflict}</span>
                              </span>
                            ) : op.is_duplicate ? (
                              <span className="text-amber-400 flex items-center space-x-1">
                                <Copy className="w-3.5 h-3.5" />
                                <span>Identical content duplicate</span>
                              </span>
                            ) : (
                              <span>{op.reason}</span>
                            )}
                          </td>
                          <td className="py-2 px-3 text-zinc-400">
                            {(op.file_size / 1024).toFixed(1)} KB
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          ) : (
            <div className="h-64 flex flex-col items-center justify-center text-center text-zinc-500">
              <FileText className="w-10 h-10 mb-3 opacity-40" />
              <p className="text-sm">Enter a directory above to preview proposed organization.</p>
              <p className="text-xs text-zinc-600 mt-1 max-w-sm">
                Files are grouped safely by extension and duplicates into subfolders. Operations can be reversed at any time.
              </p>
            </div>
          )}
        </div>

        {/* Footer */}
        {plan && plan.operations.length > 0 && (
          <div className="border-t border-zinc-800 px-6 py-4 flex items-center justify-between bg-zinc-950">
            <span className="text-xs text-zinc-400">
              {plan.conflicts_count > 0
                ? `${plan.conflicts_count} conflicted files will be skipped automatically.`
                : "All operations validated and path-confined."}
            </span>
            <div className="flex space-x-3">
              <button
                onClick={() => setPlan(null)}
                className="px-3.5 py-1.5 rounded-lg border border-zinc-700 text-zinc-300 hover:bg-zinc-800 text-xs font-medium transition cursor-pointer"
              >
                Cancel
              </button>
              <button
                onClick={handleExecute}
                disabled={loading}
                className="flex items-center space-x-2 bg-green-600 hover:bg-green-500 text-white px-4 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer"
              >
                <Play className="w-3.5 h-3.5" />
                <span>Execute Organization ({plan.operations.length} files)</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
