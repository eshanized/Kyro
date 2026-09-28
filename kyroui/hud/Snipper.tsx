import React, { useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

export const Snipper: React.FC = () => {
  const [startPoint, setStartPoint] = useState<{ x: number; y: number } | null>(null);
  const [currentPoint, setCurrentPoint] = useState<{ x: number; y: number } | null>(null);
  const isDragging = useRef(false);

  const handleMouseDown = (e: React.MouseEvent) => {
    isDragging.current = true;
    setStartPoint({ x: e.clientX, y: e.clientY });
    setCurrentPoint({ x: e.clientX, y: e.clientY });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging.current) return;
    setCurrentPoint({ x: e.clientX, y: e.clientY });
  };

  const handleMouseUp = async () => {
    if (!startPoint || !currentPoint) return;
    isDragging.current = false;

    const x = Math.min(startPoint.x, currentPoint.x);
    const y = Math.min(startPoint.y, currentPoint.y);
    const width = Math.abs(currentPoint.x - startPoint.x);
    const height = Math.abs(currentPoint.y - startPoint.y);

    if (width > 5 && height > 5) {
      try {
        await invoke("take_region_screenshot", { x, y, width, height });
      } catch (err) {
        console.error("Region capture error:", err);
      }
    }

    // Hide overlay window
    await invoke("toggle_overlay_window", { show: false });
    setStartPoint(null);
    setCurrentPoint(null);
  };

  const handleKeyDown = async (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      await invoke("toggle_overlay_window", { show: false });
      setStartPoint(null);
      setCurrentPoint(null);
    }
  };

  const getRectStyle = () => {
    if (!startPoint || !currentPoint) return { display: "none" };
    const x = Math.min(startPoint.x, currentPoint.x);
    const y = Math.min(startPoint.y, currentPoint.y);
    const width = Math.abs(currentPoint.x - startPoint.x);
    const height = Math.abs(currentPoint.y - startPoint.y);

    return {
      left: `${x}px`,
      top: `${y}px`,
      width: `${width}px`,
      height: `${height}px`,
    };
  };

  return (
    <div
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      className="fixed inset-0 cursor-crosshair bg-black/40 backdrop-blur-[1px] select-none z-50 overflow-hidden"
    >
      <div className="absolute top-6 left-1/2 -translate-x-1/2 px-4 py-2 bg-[#121212]/90 border border-[#262626] rounded-full text-xs text-neutral-200 pointer-events-none shadow-xl flex items-center gap-2">
        <span>Click and drag to snip a region</span>
        <span className="text-neutral-500">•</span>
        <kbd className="px-1.5 py-0.5 bg-neutral-800 border border-neutral-700 rounded text-[10px]">ESC</kbd>
        <span>to cancel</span>
      </div>

      {startPoint && currentPoint && (
        <div
          style={getRectStyle()}
          className="absolute border-2 border-neutral-100 bg-white/10 pointer-events-none shadow-[0_0_0_9999px_rgba(0,0,0,0.5)]"
        >
          <div className="absolute bottom-2 right-2 px-1.5 py-0.5 bg-[#ededed] text-[#0a0a0a] text-[10px] rounded font-mono font-semibold">
            {Math.abs(currentPoint.x - startPoint.x)} x {Math.abs(currentPoint.y - startPoint.y)}
          </div>
        </div>
      )}
    </div>
  );
};
