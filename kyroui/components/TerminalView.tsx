import React, { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const TerminalView: React.FC = () => {
  const terminalRef = useRef<HTMLDivElement>(null);
  const termInstance = useRef<Terminal | null>(null);
  const fitAddon = useRef<FitAddon | null>(null);

  useEffect(() => {
    if (!terminalRef.current) return;

    const term = new Terminal({
      theme: {
        background: "#0a0a0a",
        foreground: "#ededed",
        cursor: "#ededed",
        selectionBackground: "rgba(255, 255, 255, 0.2)",
        black: "#171717",
        red: "#ef4444",
        green: "#a3e635",
        yellow: "#facc15",
        blue: "#60a5fa",
        magenta: "#c084fc",
        cyan: "#38bdf8",
        white: "#ededed",
      },
      fontFamily: 'var(--font-code, "Fira Code", monospace)',
      fontSize: 12,
      lineHeight: 1.2,
      cursorBlink: true,
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(terminalRef.current);
    fit.fit();

    termInstance.current = term;
    fitAddon.current = fit;

    // Initialize PTY session in Rust
    invoke("spawn_terminal").catch((err) => {
      term.writeln(`\r\n\x1b[31mFailed to spawn terminal: ${err}\x1b[0m\r\n`);
    });

    // Listen to terminal output events from Rust
    const unlistenPromise = listen<string>("terminal:output", (event) => {
      term.write(event.payload);
    });

    // Forward user keystrokes to Rust PTY
    term.onData((data) => {
      invoke("write_terminal_input", { input: data }).catch((err) => {
        term.writeln(`\r\n\x1b[31mFailed to write terminal input: ${err}\x1b[0m\r\n`);
      });
    });

    // Handle window resize
    const handleResize = () => {
      fit.fit();
      invoke("resize_terminal", { rows: term.rows, cols: term.cols }).catch(console.error);
    };
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      unlistenPromise.then((fn) => fn());
      term.dispose();
    };
  }, []);

  return (
    <div className="w-full h-full p-2 bg-[#0a0a0a] rounded-lg border border-neutral-800 overflow-hidden flex flex-col">
      <div className="flex items-center justify-between pb-2 mb-2 border-b border-neutral-800 text-xs font-mono text-neutral-400">
        <span className="flex items-center gap-1.5">
          <span className="w-2 h-2 rounded-full bg-neutral-400 animate-pulse"></span>
          Interactive Terminal (Local PTY)
        </span>
        <button
          onClick={() => fitAddon.current?.fit()}
          className="hover:text-neutral-200 transition-colors"
          title="Fit Window"
        >
          Fit
        </button>
      </div>
      <div ref={terminalRef} className="flex-1 overflow-hidden" />
    </div>
  );
};
