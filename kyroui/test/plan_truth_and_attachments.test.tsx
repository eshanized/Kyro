/**
 * Task-plan truthfulness and attachment policy regression tests
 * (Issues #22, bugs 5, 6, 19).
 *
 * Drives the REAL App component: intent classification → suggested-workflow
 * card rendering, backend `agent:step` events → truthful plan evidence, and
 * the browser File input → backend `extract_attachment_text` document parse.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { classifyTaskIntent } from "../lib/taskIntent";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ listen: vi.fn().mockResolvedValue(() => {}) }),
}));

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

let stepHandler: ((event: { payload: any }) => void) | null = null;

const conversations = [
  { id: "conv-1", title: "Task Alpha", model: "m", created_at: 1700000000000 },
];

function baseInvokeImpl(turnFactory: () => any = () => new Promise<string>(() => {})) {
  return async (cmd: string, args: any) => {
    if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
    if (cmd === "list_conversations") return conversations;
    if (cmd === "get_conversation_messages") return [];
    if (cmd === "get_default_bot") {
      return {
        id: "apex",
        name: "Apex",
        role: "Strategic Orchestrator",
        avatar: "zap",
        system_prompt: "",
        is_default: true,
      };
    }
    if (cmd === "list_pending_approvals") return [];
    if (cmd === "list_authorized_folders") return [];
    if (cmd === "add_message") return {};
    if (cmd === "run_agent_turn") return turnFactory();
    return Promise.resolve(null);
  };
}

class MockFileReader {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  result: string | null = null;
  readAsDataURL(_file: any) {
    this.result = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
    setTimeout(() => {
      if (this.onload) this.onload();
    }, 5);
  }
  readAsText(file: any) {
    // Return the raw underlying bytes as text where tests populate them.
    try {
      this.result = new TextDecoder().decode(new Uint8Array(file._bytes ?? []));
    } catch {
      this.result = "";
    }
    setTimeout(() => {
      if (this.onload) this.onload();
    }, 5);
  }
  readAsArrayBuffer(file: any) {
    this.result = new Uint8Array(file._bytes ?? []).buffer;
    setTimeout(() => {
      if (this.onload) this.onload();
    }, 5);
  }
}

describe("Intent classification honesty (bug 5)", () => {
  it("never marks substring-lookalikes as consequential", () => {
    // "latest" contains "test" — the OLD /test/i substring regex fired on it.
    expect(classifyTaskIntent("What changed in the latest release?").isConsequential).toBe(false);
    expect(classifyTaskIntent("Why is the sky blue?").isConsequential).toBe(false);
    // Explicit task wording does classify.
    expect(classifyTaskIntent("Organize my files into folders").isConsequential).toBe(true);
    expect(classifyTaskIntent("Run pytest and report failures").isConsequential).toBe(true);
    expect(classifyTaskIntent("Clean download folder and verify integrity").isConsequential).toBe(true);
  });
});

describe("Suggested workflow rendering (bug 5)", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    stepHandler = null;
    mockListen.mockImplementation(async (event: string, handler?: any) => {
      if (event === "agent:step") stepHandler = handler;
      return vi.fn();
    });
    mockInvoke.mockImplementation(baseInvokeImpl());
    render(<App />);
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument()
    );
    await waitFor(() => expect(stepHandler).not.toBeNull());
  });

  it("everyday questions create NO plan card", async () => {
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "Why is the sky blue?" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("run_agent_turn", expect.anything()));
    // Let any late plan creation surface.
    await act(async () => new Promise((r) => setTimeout(r, 0)));
    expect(screen.queryByTestId("task-progress-card")).not.toBeInTheDocument();
  });

  it("a consequential prompt yields an honestly labeled SUGGESTED workflow, not an engine plan", async () => {
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));

    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());
    expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
    expect(screen.getByText("(suggested)")).toBeInTheDocument();
    // The old authoritative label must not appear for client-side plans.
    expect(screen.queryByText("Authoritative Execution Plan")).not.toBeInTheDocument();
    // Honest resource claims for an organize task.
    expect(
      screen.getByText(/Authorized folders \(paths outside them need your approval\)/i)
    ).toBeInTheDocument();
  });

  it("a new task replaces the previous suggested workflow (stale-plan clearing)", async () => {
    let resolveRun: (v: any) => void = () => {};
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      const base = await baseInvokeImpl();
      if (cmd === "run_agent_turn") {
        return new Promise<string>((resolve) => {
          resolveRun = resolve;
        });
      }
      return base(cmd, args);
    });

    // Run one creates a suggested workflow.
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));
    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());

    // Let the run terminate before starting a new task: a conversation's
    // submission lock is synchronous and CORRECTLY blocks a resubmit while
    // the run is in flight (bug 4 semantics), so the stale-plan guarantee
    // applies to the next task once the previous one resolves.
    resolveRun("First organize done.");
    await waitFor(() =>
      expect(screen.getByText(/First organize done\./i)).toBeInTheDocument()
    );

    // A plain question cannot keep the previous suggested workflow alive.
    await act(async () => {
      fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
        target: { value: "Why is the sky blue?" },
      });
      fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));
    });

    await waitFor(() =>
      expect(screen.queryByTestId("task-progress-card")).not.toBeInTheDocument()
    );
  });
});

describe("Step evidence truthfulness (bug 6)", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    stepHandler = null;
    mockListen.mockImplementation(async (event: string, handler?: any) => {
      if (event === "agent:step") stepHandler = handler;
      return vi.fn();
    });
    mockInvoke.mockImplementation(baseInvokeImpl());
    render(<App />);
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument()
    );
    await waitFor(() => expect(stepHandler).not.toBeNull());
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));
    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());
  });

  it("an inspection tool completes ONLY the inspection step, never the execution step", async () => {
    const runId = mockInvoke.mock.calls.find(([cmd]) => cmd === "run_agent_turn")![1].runId;
    act(() => {
      stepHandler?.({
        payload: {
          run_id: runId,
          conversation_id: "conv-1",
          step: 1,
          thought: "Looking around",
          tool_calls: [{ id: "tc1", name: "read_file", arguments: {} }],
          tool_results: [
            {
              tool_call_id: "tc1",
              name: "read_file",
              output: "file contents",
              is_error: false,
            },
          ],
          is_finished: false,
        },
      });
    });

    await waitFor(() =>
      expect(screen.getByText(/Inspection tool ran successfully/i)).toBeInTheDocument()
    );
    // The execution step MUST still be pending: inspection is not execution.
    expect(screen.queryByText("Actions executed")).not.toBeInTheDocument();
  });

  it("execute_shell running an actual test command counts as verification via its command argument", async () => {
    const runId = mockInvoke.mock.calls.find(([cmd]) => cmd === "run_agent_turn")![1].runId;
    act(() => {
      stepHandler?.({
        payload: {
          run_id: runId,
          conversation_id: "conv-1",
          step: 1,
          thought: "running tests",
          tool_calls: [
            { id: "tc1", name: "execute_shell", arguments: { command: "npm test" } },
          ],
          tool_results: [
            {
              tool_call_id: "tc1",
              name: "execute_shell",
              output: "All 24 assertions executed correctly and passed",
              is_error: false,
              verification: {
                status: "succeeded",
                condition_checked: "Process exit status and test suite passing",
                evidence: "Verification check passed via execute_shell",
                task_postcondition_verified: true,
                trusted_verifier: true,
              },
            },
          ],
          is_finished: true,
        },
      });
    });

    await waitFor(() =>
      expect(screen.getAllByText(/Verification check passed via execute_shell/i).length).toBeGreaterThanOrEqual(1)
    );
  });

  it("a recorded execution failure is immutable: later successes may not clear it", async () => {
    const runId = mockInvoke.mock.calls.find(([cmd]) => cmd === "run_agent_turn")![1].runId;
    act(() => {
      stepHandler?.({
        payload: {
          run_id: runId,
          conversation_id: "conv-1",
          step: 1,
          thought: "moving files",
          tool_calls: [{ id: "tc1", name: "move_file", arguments: {} }],
          tool_results: [
            { tool_call_id: "tc1", name: "move_file", output: "disk full", is_error: true },
          ],
          is_finished: false,
        },
      });
    });
    await waitFor(() =>
      expect(screen.getByText(/Execution failure observed/)).toBeInTheDocument()
    );

    // A later successful action must NOT flip the step back to completed.
    act(() => {
      stepHandler?.({
        payload: {
          run_id: runId,
          conversation_id: "conv-1",
          step: 2,
          thought: "retrying",
          tool_calls: [{ id: "tc2", name: "move_file", arguments: {} }],
          tool_results: [
            { tool_call_id: "tc2", name: "move_file", output: "ok", is_error: false },
          ],
          is_finished: false,
        },
      });
    });
    expect(screen.getByText(/Execution failure observed/)).toBeInTheDocument();
  });

  it("a plan can only be mutated by events of the run that owns it", async () => {
    act(() => {
      stepHandler?.({
        payload: {
          run_id: "run-someone-else",
          conversation_id: "conv-1",
          step: 1,
          thought: "foreign run",
          tool_calls: [{ id: "tcx", name: "read_file", arguments: {} }],
          tool_results: [
            { tool_call_id: "tcx", name: "read_file", output: "contents", is_error: true },
          ],
          is_finished: false,
        },
      });
    });
    // No failure evidence from a foreign run may touch this run's plan.
    expect(screen.queryByText("Execution failure observed")).not.toBeInTheDocument();
    expect(screen.queryByText(/Error in read_file/i)).not.toBeInTheDocument();
  });
});

describe("Attachment policy (bug 19)", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    stepHandler = null;
    mockListen.mockImplementation(async (event: string, handler?: any) => {
      if (event === "agent:step") stepHandler = handler;
      return vi.fn();
    });
    mockInvoke.mockImplementation(baseInvokeImpl());
    vi.stubGlobal("FileReader", MockFileReader);
    render(<App />);
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument()
    );
  });

  it("accepts a genuine image and never routes it through the document parser", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(fileInput, {
      target: { files: [new File(["png-bytes"], "chart.png", { type: "image/png" })] },
    });
    await waitFor(() => expect(screen.getByText("chart.png")).toBeInTheDocument());
    expect(
      mockInvoke.mock.calls.some(([cmd]) => cmd === "extract_attachment_text")
    ).toBe(false);
  });

  it("rejects a mismatched image extension/content-type pair", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    // Word document wearing a .png extension cannot be attached as an image.
    const fake = new File(["magicy"], "virus.png", { type: "application/msword" });
    Object.defineProperty(fake, "name", {
      value: "virus.png",
      writable: false,
    });
    fireEvent.change(fileInput, { target: { files: [fake] } });

    await waitFor(() =>
      expect(screen.getByText(/mismatched file cannot be attached/i)).toBeInTheDocument()
    );
    expect(screen.queryByText("virus.png")).not.toBeInTheDocument();
  });

  it("rejects oversized images before any read", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    const big = new File(["x"], "huge.png", { type: "image/png" });
    Object.defineProperty(big, "size", { value: 11 * 1024 * 1024 });
    fireEvent.change(fileInput, { target: { files: [big] } });

    await waitFor(() =>
      expect(screen.getByText(/exceeds the image size limit/i)).toBeInTheDocument()
    );
    expect(screen.queryByText("huge.png")).not.toBeInTheDocument();
  });

  it("rejects empty files", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(fileInput, {
      target: { files: [new File([], "empty.txt", { type: "text/plain" })] },
    });
    await waitFor(() =>
      expect(screen.getByText(/"empty.txt" is empty/i)).toBeInTheDocument()
    );
  });

  it("routes a searchable PDF through the backend document parser, not client text decoding", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      const base = await baseInvokeImpl()(cmd, args);
      if (cmd === "extract_attachment_text") {
        expect(args.fileName).toBe("doc.pdf");
        expect(typeof args.base64Bytes).toBe("string");
        expect(args.base64Bytes.length).toBeGreaterThan(0);
        return {
          text: "PDF page one extract",
          format: "pdf",
          page_count: 3,
          file_name: "doc.pdf",
          word_count: 4,
          character_count: 20,
          truncated: false,
        };
      }
      return base;
    });

    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(fileInput, {
      target: { files: [new File(["%PDF-1.5 fake"], "doc.pdf", { type: "application/pdf" })] },
    });

    await waitFor(() => expect(screen.getByText("doc.pdf")).toBeInTheDocument());
    expect(
      mockInvoke.mock.calls.some(([cmd]) => cmd === "extract_attachment_text")
    ).toBe(true);
  });

  it("rejects a binary payload in a text extension before submission", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    const binary = new File([new Uint8Array([0, 1, 2, 0])], "malware.rs", { type: "text/plain" });
    (binary as any)._bytes = [0, 1, 2, 0];
    fireEvent.change(fileInput, { target: { files: [binary] } });

    await waitFor(() =>
      expect(
        screen.getByText(/contains binary data and cannot be attached as text/i)
      ).toBeInTheDocument()
    );
    expect(screen.queryByText("malware.rs")).not.toBeInTheDocument();
  });

  it("rejects an invalid UTF-8 byte sequence in text file (Bug 19)", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    const invalidUtf8 = new File([new Uint8Array([0xff, 0xfe, 0xfd])], "invalid.txt", { type: "text/plain" });
    (invalidUtf8 as any)._bytes = [0xff, 0xfe, 0xfd];
    fireEvent.change(fileInput, { target: { files: [invalidUtf8] } });

    await waitFor(() =>
      expect(
        screen.getByText(/not valid UTF-8 text/i)
      ).toBeInTheDocument()
    );
    expect(screen.queryByText("invalid.txt")).not.toBeInTheDocument();
  });

  it("rejects ELF executable magic bytes even with .txt extension (Bug 19)", async () => {
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    const elfFile = new File([new Uint8Array([0x7f, 0x45, 0x4c, 0x46, 1, 1, 1, 0])], "binary.txt", { type: "text/plain" });
    (elfFile as any)._bytes = [0x7f, 0x45, 0x4c, 0x46, 1, 1, 1, 0];
    fireEvent.change(fileInput, { target: { files: [elfFile] } });

    await waitFor(() =>
      expect(
        screen.getByText(/contains binary data and cannot be attached as text/i)
      ).toBeInTheDocument()
    );
    expect(screen.queryByText("binary.txt")).not.toBeInTheDocument();
  });
});
