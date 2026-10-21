import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Camera,
  Sparkles,
  HelpCircle,
  Eye,
  CheckCircle,
  AlertCircle,
  X,
  Play,
  RotateCw,
} from "lucide-react";
import { VisualAnalysisResult } from "../types/agent";

interface ScreenExplainerModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialScreenshotBase64?: string | null;
}

export const ScreenExplainerModal: React.FC<ScreenExplainerModalProps> = ({
  isOpen,
  onClose,
  initialScreenshotBase64,
}) => {
  const [screenshot, setScreenshot] = useState<string | null>(
    initialScreenshotBase64 || null
  );
  const [question, setQuestion] = useState("Explain what is happening on this screen and what I should do next.");
  const [loading, setLoading] = useState(false);
  const [analysis, setAnalysis] = useState<VisualAnalysisResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionOutput, setActionOutput] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleCaptureScreen = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await invoke<{ base64_png: string }>("take_screenshot", {
        monitorIndex: 0,
      });
      setScreenshot(res.base64_png);
    } catch (e: any) {
      setError(e?.toString() || "Failed to capture screen.");
    } finally {
      setLoading(false);
    }
  };

  const handleAnalyze = async () => {
    if (!screenshot) {
      setError("Please capture or attach a screenshot first.");
      return;
    }
    setLoading(true);
    setError(null);
    setActionOutput(null);

    try {
      const res = await invoke<VisualAnalysisResult>("explain_screen_capture", {
        base64Png: screenshot,
        question: question.trim(),
        model: null,
      });
      setAnalysis(res);
    } catch (e: any) {
      setError(e?.toString() || "Visual analysis failed.");
    } finally {
      setLoading(false);
    }
  };

  const handleExecuteRecommendedAction = async () => {
    if (!analysis) return;
    setLoading(true);
    setError(null);
    try {
      // Prompt agent turn with grounded screenshot & recommended action
      const prompt = `Based on the visual analysis of the screen, execute the recommended step: ${analysis.summary}`;
      await invoke("run_agent_turn", {
        prompt,
        visualData: screenshot,
      });
      setActionOutput("Action initiated through Kyro's governed execution engine. Check Timeline for progress.");
    } catch (e: any) {
      setError(e?.toString() || "Failed to execute recommended action.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-4xl max-h-[88vh] flex flex-col rounded-xl border border-zinc-700 bg-zinc-900 shadow-2xl text-zinc-100 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-purple-500/10 text-purple-400">
              <Camera className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold">Explain My Screen</h2>
              <p className="text-xs text-zinc-400">
                Visual assistance: understand errors, layouts, forms, and get grounded action guidance
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

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-5">
          {/* Controls Bar */}
          <div className="flex items-center space-x-3">
            <button
              onClick={handleCaptureScreen}
              disabled={loading}
              className="flex items-center space-x-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 px-4 py-2 rounded-lg text-xs font-medium border border-zinc-700 transition cursor-pointer"
            >
              <RotateCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
              <span>Capture Fresh Display</span>
            </button>
            <div className="flex-1">
              <input
                type="text"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                placeholder="Ask what you need help with (e.g. 'What is this error?', 'Find the submit button')"
                className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-xs text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-purple-500"
              />
            </div>
            <button
              onClick={handleAnalyze}
              disabled={loading || !screenshot}
              className="flex items-center space-x-2 bg-purple-600 hover:bg-purple-500 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-xs font-medium transition cursor-pointer"
            >
              <Sparkles className="w-3.5 h-3.5" />
              <span>{loading ? "Analyzing..." : "Explain Screen"}</span>
            </button>
          </div>

          {error && (
            <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-300 text-xs">
              {error}
            </div>
          )}
          {actionOutput && (
            <div className="p-3 rounded-lg bg-green-500/10 border border-green-500/30 text-green-300 text-xs">
              {actionOutput}
            </div>
          )}

          {/* Screenshot Preview & Visual Results */}
          <div className="grid grid-cols-2 gap-5">
            {/* Left: Image Canvas */}
            <div className="flex flex-col rounded-xl border border-zinc-800 bg-zinc-950 overflow-hidden">
              <div className="p-2.5 border-b border-zinc-800 text-[11px] font-medium text-zinc-400 flex items-center justify-between">
                <span>Observed Screen Preview</span>
                {screenshot && (
                  <span className="text-green-400 flex items-center space-x-1">
                    <CheckCircle className="w-3 h-3" />
                    <span>Captured</span>
                  </span>
                )}
              </div>
              <div className="p-3 flex items-center justify-center min-h-[260px] max-h-[360px] overflow-hidden bg-zinc-900/40">
                {screenshot ? (
                  <img
                    src={`data:image/png;base64,${screenshot}`}
                    alt="Captured screen"
                    className="max-h-full max-w-full rounded object-contain border border-zinc-800"
                  />
                ) : (
                  <div className="text-center text-zinc-500 text-xs">
                    <Camera className="w-8 h-8 mx-auto mb-2 opacity-30" />
                    Click "Capture Fresh Display" to grab your screen
                  </div>
                )}
              </div>
            </div>

            {/* Right: Structured Visual Intelligence */}
            <div className="flex flex-col rounded-xl border border-zinc-800 bg-zinc-950 p-4 space-y-4">
              <div className="text-xs font-medium text-zinc-300 flex items-center space-x-1.5">
                <Eye className="w-4 h-4 text-purple-400" />
                <span>Visual Analysis & Actionable Next Steps</span>
              </div>

              {analysis ? (
                <div className="space-y-3.5 text-xs">
                  {/* Summary */}
                  <div className="p-3 rounded-lg bg-zinc-900/80 border border-zinc-800/80">
                    <div className="text-[11px] text-zinc-400 font-medium mb-1">Diagnostic Summary</div>
                    <div className="text-zinc-200 leading-relaxed">{analysis.summary}</div>
                  </div>

                  {/* Observed Facts vs Inferred Causes */}
                  {analysis.supporting_evidence && (
                    <div className="p-3 rounded-lg bg-zinc-900/60 border border-zinc-800/60">
                      <div className="text-[11px] text-zinc-400 font-medium mb-1">Observed Visual Facts</div>
                      <div className="text-zinc-300">{analysis.supporting_evidence}</div>
                    </div>
                  )}

                  {/* Defects / Identified Issues */}
                  {analysis.visible_defects.length > 0 && (
                    <div className="space-y-1.5">
                      <div className="text-[11px] text-zinc-400 font-medium">Identified Elements & Defects</div>
                      {analysis.visible_defects.map((def, i) => (
                        <div
                          key={i}
                          className="p-2 rounded bg-red-500/10 border border-red-500/20 text-red-300 flex items-start space-x-2"
                        >
                          <AlertCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                          <div>
                            <span className="font-semibold">{def.defect_type}: </span>
                            <span>{def.description}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}

                  {/* Action Execution Button */}
                  <div className="pt-2">
                    <button
                      onClick={handleExecuteRecommendedAction}
                      disabled={loading}
                      className="w-full flex items-center justify-center space-x-2 bg-green-600 hover:bg-green-500 text-white py-2 rounded-lg font-medium transition cursor-pointer"
                    >
                      <Play className="w-3.5 h-3.5" />
                      <span>Execute Grounded Action</span>
                    </button>
                  </div>
                </div>
              ) : (
                <div className="h-full flex flex-col items-center justify-center text-center text-zinc-500 text-xs">
                  <HelpCircle className="w-8 h-8 mb-2 opacity-30" />
                  <p>Ask a question and click "Explain Screen" to see structured facts and next steps.</p>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
