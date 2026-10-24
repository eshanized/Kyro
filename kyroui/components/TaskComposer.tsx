import React, { useRef, useEffect } from "react";
import { Paperclip, Mic, MicOff, Send, StopCircle, X, FileCode, Sparkles } from "lucide-react";

export interface TaskComposerProps {
  prompt: string;
  onPromptChange: (value: string) => void;
  onSubmit: () => void;
  onAbort?: () => void;
  isAgentRunning: boolean;
  activeBotName?: string;
  attachedFile?: { name: string; base64?: string; mimeType?: string } | null;
  onSelectFile?: (file: File) => void;
  onRemoveFile?: () => void;
  isListening?: boolean;
  onToggleVoice?: () => void;
  suggestions?: string[];
  onSelectSuggestion?: (suggestion: string) => void;
  className?: string;
  placeholder?: string;
}

export const TaskComposer: React.FC<TaskComposerProps> = ({
  prompt,
  onPromptChange,
  onSubmit,
  onAbort,
  isAgentRunning,
  activeBotName = "Kyro",
  attachedFile,
  onSelectFile,
  onRemoveFile,
  isListening = false,
  onToggleVoice,
  suggestions = [],
  onSelectSuggestion,
  className = "",
  placeholder,
}) => {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [showAllSuggestions, setShowAllSuggestions] = React.useState(false);

  // Auto-resize textarea height up to 160px
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    const nextHeight = Math.min(Math.max(textarea.scrollHeight, 44), 160);
    textarea.style.height = `${nextHeight}px`;
  }, [prompt]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!isAgentRunning && (prompt.trim() || attachedFile)) {
        onSubmit();
      }
    }
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file && onSelectFile) {
      onSelectFile(file);
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  };

  const visibleSuggestions = showAllSuggestions ? suggestions : suggestions.slice(0, 3);

  const defaultPlaceholder = isListening
    ? "Listening to voice..."
    : `Ask @${activeBotName} to automate a desktop task, inspect project, or run workflows... (Shift+Enter for newline)`;

  return (
    <div className={`border-t border-neutral-800 bg-[#0a0a0a] ${className}`}>
      {/* Attached File Preview Chip */}
      {attachedFile && (
        <div className="px-4 py-2 bg-[#121212] border-b border-neutral-800 flex items-center justify-between text-xs text-neutral-200">
          <div className="flex items-center gap-2">
            <FileCode className="w-4 h-4 text-neutral-400 shrink-0" />
            <span className="truncate">
              Attached: <strong className="text-white">{attachedFile.name}</strong>
            </span>
          </div>
          {onRemoveFile && (
            <button
              type="button"
              onClick={onRemoveFile}
              className="text-neutral-400 hover:text-white p-1 rounded transition"
              aria-label="Remove attached file"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      )}

      {/* Responsive suggestion pills without horizontal overflow */}
      {suggestions.length > 0 && !isAgentRunning && (
        <div className="px-4 pt-2.5 pb-1 flex flex-wrap items-center gap-1.5 text-[11px]">
          <span className="text-neutral-400 font-medium flex items-center gap-1 mr-1">
            <Sparkles className="w-3 h-3 text-neutral-400" /> Suggestions:
          </span>
          {visibleSuggestions.map((s, idx) => (
            <button
              key={idx}
              type="button"
              onClick={() => onSelectSuggestion && onSelectSuggestion(s)}
              className="px-2.5 py-1 rounded-lg bg-[#141414] hover:bg-[#1f1f1f] text-neutral-300 hover:text-white border border-neutral-800/80 transition text-left"
            >
              {s}
            </button>
          ))}
          {suggestions.length > 3 && (
            <button
              type="button"
              onClick={() => setShowAllSuggestions(!showAllSuggestions)}
              className="px-2 py-1 text-[11px] text-neutral-400 hover:text-neutral-200 transition underline underline-offset-2"
            >
              {showAllSuggestions ? "Show less" : `+${suggestions.length - 3} more`}
            </button>
          )}
        </div>
      )}

      {/* Multi-line Composer */}
      <div className="p-3 sm:p-4">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (!isAgentRunning && (prompt.trim() || attachedFile)) {
              onSubmit();
            }
          }}
          className="flex items-end gap-2 bg-[#121212] border border-neutral-800 rounded-2xl p-2 focus-within:border-neutral-700 transition shadow-sm"
        >
          {/* Hidden File Picker */}
          <input
            type="file"
            ref={fileInputRef}
            onChange={handleFileChange}
            className="hidden"
            aria-hidden="true"
          />

          {/* Attachment Button */}
          {onSelectFile && (
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              className="p-2.5 rounded-xl text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800/60 transition shrink-0 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-neutral-400"
              title="Attach local file or screenshot"
              aria-label="Attach file"
            >
              <Paperclip className="w-4 h-4" />
            </button>
          )}

          {/* Voice Dictation Button */}
          {onToggleVoice && (
            <button
              type="button"
              onClick={onToggleVoice}
              className={`p-2.5 rounded-xl transition shrink-0 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-neutral-400 ${
                isListening
                  ? "bg-red-950/40 text-red-400 border border-red-800/60 animate-pulse"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800/60"
              }`}
              title={isListening ? "Listening... click to stop" : "Voice dictation"}
              aria-label={isListening ? "Stop voice dictation" : "Start voice dictation"}
            >
              {isListening ? (
                <Mic className="w-4 h-4 text-red-400 animate-bounce" />
              ) : (
                <MicOff className="w-4 h-4" />
              )}
            </button>
          )}

          {/* Multi-line Textarea */}
          <textarea
            ref={textareaRef}
            rows={1}
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={isAgentRunning}
            placeholder={placeholder || defaultPlaceholder}
            className="flex-1 bg-transparent px-2 py-2 text-xs sm:text-sm text-neutral-100 placeholder-neutral-500 focus:outline-none resize-none leading-relaxed min-h-[40px] max-h-[160px]"
          />

          {/* Action Button: Send or Stop */}
          {isAgentRunning ? (
            <button
              type="button"
              onClick={onAbort}
              className="px-4 py-2 rounded-xl bg-red-600 hover:bg-red-500 text-white text-xs font-semibold uppercase tracking-wider flex items-center gap-1.5 shadow-sm transition shrink-0 h-9"
              aria-label="Stop running task"
            >
              <StopCircle className="w-4 h-4 fill-current" />
              <span className="hidden sm:inline">Stop</span>
            </button>
          ) : (
            <button
              type="submit"
              disabled={!prompt.trim() && !attachedFile}
              className="px-4 py-2 rounded-xl bg-[#ededed] hover:bg-white text-[#0a0a0a] text-xs font-semibold flex items-center gap-1.5 shadow-sm transition shrink-0 disabled:opacity-40 disabled:cursor-not-allowed h-9"
              aria-label="Send task message"
            >
              <Send className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">Send</span>
            </button>
          )}
        </form>
      </div>
    </div>
  );
};
