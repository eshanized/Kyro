import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TimelineStep } from "../types/agent";
import { History, X, ChevronLeft, ChevronRight, Percent, Target } from "lucide-react";

interface TimelineDrawerProps {
  isOpen: boolean;
  onClose: () => void;
}

export const TimelineDrawer: React.FC<TimelineDrawerProps> = ({ isOpen, onClose }) => {
  const [steps, setSteps] = useState<TimelineStep[]>([]);
  const [selectedIdx, setSelectedIdx] = useState<number>(0);

  useEffect(() => {
    if (isOpen) {
      loadTimeline();
    }
  }, [isOpen]);

  const loadTimeline = async () => {
    try {
      const list = await invoke<TimelineStep[]>("get_timeline_steps");
      setSteps(list);
      if (list.length > 0) {
        setSelectedIdx(list.length - 1);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleClear = async () => {
    try {
      await invoke("clear_timeline_steps");
      setSteps([]);
    } catch (e) {
      console.error(e);
    }
  };

  if (!isOpen) return null;

  const currentStep = steps[selectedIdx];

  return (
    <div className="fixed inset-y-0 right-0 z-50 w-full max-w-3xl bg-[#0a0a0a] border-l border-[#262626] shadow-2xl flex flex-col overflow-hidden text-neutral-100">
      {/* Header */}
      <div className="px-6 py-4 border-b border-[#262626] bg-[#121212] flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <History className="w-5 h-5 text-neutral-300" />
          <h2 className="text-base font-semibold">Visual Step Timeline & Diff Inspector</h2>
          <span className="text-xs px-2 py-0.5 rounded bg-neutral-800 text-neutral-300 border border-neutral-700 font-mono">
            {steps.length} Steps Logged
          </span>
        </div>
        <div className="flex items-center gap-2">
          {steps.length > 0 && (
            <button
              onClick={handleClear}
              className="px-2.5 py-1 text-xs text-neutral-400 hover:text-red-400 hover:bg-neutral-800 rounded transition"
            >
              Clear
            </button>
          )}
          <button
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-white rounded-lg hover:bg-neutral-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
      </div>

      {steps.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center p-8 text-center text-neutral-500 text-xs">
          <Target className="w-10 h-10 mb-2 opacity-40 text-neutral-400" />
          No visual steps recorded in this session yet. Run an autonomous computer action to generate visual diff timelines.
        </div>
      ) : (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Step Inspector View */}
          {currentStep && (
            <div className="flex-1 p-6 overflow-y-auto space-y-4">
              <div className="flex items-center justify-between bg-[#141414] p-3 rounded-xl border border-[#262626]">
                <div className="flex items-center gap-3">
                  <span className="px-2 py-0.5 rounded bg-neutral-800 text-neutral-200 border border-neutral-700 font-mono text-xs font-semibold">
                    Step {currentStep.step}
                  </span>
                  <span className="text-sm font-semibold text-neutral-200">{currentStep.action_name}</span>
                </div>
                <div className="flex items-center gap-3 text-xs text-neutral-400 font-mono">
                  {currentStep.click_x !== undefined && currentStep.click_x !== null && (
                    <span className="flex items-center gap-1 text-red-400">
                      <Target className="w-3.5 h-3.5" /> ({currentStep.click_x}, {currentStep.click_y})
                    </span>
                  )}
                  <span className="flex items-center gap-1 text-neutral-300">
                    <Percent className="w-3 h-3" /> {currentStep.diff_percent.toFixed(1)}% delta
                  </span>
                </div>
              </div>

              {/* Before / After Screenshots */}
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-neutral-400 flex items-center justify-between">
                    <span>Targeting (Before Action)</span>
                    <span className="text-[10px] text-red-400 font-mono flex items-center gap-1">
                      <span className="w-2 h-2 rounded-full bg-red-500 animate-pulse"></span> Red Target Ring
                    </span>
                  </div>
                  <div className="rounded-xl overflow-hidden border border-[#262626] bg-black aspect-video flex items-center justify-center">
                    <img
                      src={`data:image/png;base64,${currentStep.before_base64}`}
                      alt="Before"
                      className="w-full h-full object-contain"
                    />
                  </div>
                </div>

                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-neutral-400">Result (After Action)</div>
                  <div className="rounded-xl overflow-hidden border border-[#262626] bg-black aspect-video flex items-center justify-center">
                    <img
                      src={`data:image/png;base64,${currentStep.after_base64}`}
                      alt="After"
                      className="w-full h-full object-contain"
                    />
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Scrubber Bottom Bar */}
          <div className="p-4 border-t border-[#262626] bg-[#0a0a0a] flex items-center gap-3">
            <button
              onClick={() => setSelectedIdx((prev) => Math.max(0, prev - 1))}
              disabled={selectedIdx === 0}
              className="p-2 text-neutral-400 hover:text-white disabled:opacity-30 rounded-lg hover:bg-neutral-800 transition"
            >
              <ChevronLeft className="w-5 h-5" />
            </button>

            <div className="flex-1 overflow-x-auto flex items-center gap-2 py-1">
              {steps.map((st, i) => (
                <button
                  key={i}
                  onClick={() => setSelectedIdx(i)}
                  className={`px-3 py-1.5 rounded-lg text-xs font-mono font-medium transition shrink-0 flex items-center gap-1.5 ${
                    selectedIdx === i
                      ? "bg-[#ededed] text-[#0a0a0a] font-semibold shadow"
                      : "bg-[#171717] border border-[#262626] text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800"
                  }`}
                >
                  Step {st.step}
                  <span className="text-[10px] opacity-70">({st.diff_percent.toFixed(0)}%)</span>
                </button>
              ))}
            </div>

            <button
              onClick={() => setSelectedIdx((prev) => Math.min(steps.length - 1, prev + 1))}
              disabled={selectedIdx === steps.length - 1}
              className="p-2 text-neutral-400 hover:text-white disabled:opacity-30 rounded-lg hover:bg-neutral-800 transition"
            >
              <ChevronRight className="w-5 h-5" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
