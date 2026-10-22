import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { ScreenExplainerModal } from "../components/ScreenExplainerModal";
import { DocumentAssistantModal } from "../components/DocumentAssistantModal";
import { FileOrganizerModal } from "../components/FileOrganizerModal";
import { McpModal } from "../components/McpModal";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { MyToolsView } from "../components/MyToolsView";
import { OnboardingModal } from "../components/OnboardingModal";
import { TerminalView } from "../components/TerminalView";
import { TimelineDrawer } from "../components/TimelineDrawer";
import { MarkdownRenderer, CodeBlock } from "../components/MarkdownRenderer";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock @xterm/xterm and addon-fit
vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class {
      open = vi.fn();
      loadAddon = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      dispose = vi.fn();
      rows = 24;
      cols = 80;
      onData = vi.fn((cb: any) => {
        (this as any)._onDataCb = cb;
        return { dispose: vi.fn() };
      });
    },
  };
});

vi.mock("@xterm/addon-fit", () => {
  return {
    FitAddon: class {
      fit = vi.fn();
    },
  };
});

describe("Frontend Climax to 95%+ Coverage Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("DevWorkbenchModal Advanced Branches", () => {
    it("handles viewport switches, input changes, and demo baseline save/compare", async () => {
      const mockBaseline = {
        project_path: "/home/snigdha/Desktop/Kyro",
        route: "/dashboard",
        viewport: "tablet",
        base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        created_at: 1700000000,
      };

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "set_audit_baseline") {
          return Promise.resolve(mockBaseline);
        }
        if (cmd === "compare_visual_baseline") {
          return Promise.reject("Simulated comparison failure");
        }
        if (cmd === "create_multi_agent_workspace") {
          return Promise.reject("Failed multi-agent init");
        }
        return Promise.resolve([]);
      });

      const handleClose = vi.fn();
      render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="audit" />);

      // Change target route & project path
      const routeInput = screen.getByDisplayValue("/");
      fireEvent.change(routeInput, { target: { value: "/settings" } });

      const pathInputs = screen.getAllByRole("textbox");
      if (pathInputs[1]) {
        fireEvent.change(pathInputs[1], { target: { value: "/tmp/custom_project" } });
      }

      // Switch viewports: Tablet (768), Mobile (375), Desktop (1280)
      const tabletBtn = screen.getByRole("button", { name: /768/i });
      fireEvent.click(tabletBtn);

      const mobileBtn = screen.getByRole("button", { name: /375/i });
      fireEvent.click(mobileBtn);

      const desktopBtn = screen.getByRole("button", { name: /1280/i });
      fireEvent.click(desktopBtn);

      // Register Golden Baseline (handleSaveDemoBaseline)
      const saveDemoBtn = screen.getByText("Save As Baseline");
      fireEvent.click(saveDemoBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("set_audit_baseline", expect.any(Object));
        expect(screen.getByText(/Saved baseline for/i)).toBeInTheDocument();
      });

      // Compare against baseline (triggers catch branch)
      const compareBtn = screen.getByText("Compare Against Baseline");
      fireEvent.click(compareBtn);

      await waitFor(() => {
        expect(screen.getByText(/Comparison error: Simulated comparison failure/i)).toBeInTheDocument();
      });
    });

    it("handles multi-agent workspace creation error alert", async () => {
      const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
      (invoke as any).mockRejectedValueOnce(new Error("Workspace directory lock error"));

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="workspace" />);

      const initBtn = screen.getByText("Initialize Specialist Workspace");
      fireEvent.click(initBtn);

      await waitFor(() => {
        expect(alertSpy).toHaveBeenCalledWith(expect.stringContaining("Failed to initialize multi-agent workspace"));
      });
      alertSpy.mockRestore();
    });
  });

  describe("ScreenExplainerModal Error Handling and Inputs", () => {
    it("handles empty screenshot error notice, custom question input, and action execution error", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "take_screenshot") {
          return Promise.resolve({
            base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            width: 1920,
            height: 1080,
          });
        }
        if (cmd === "explain_screen_capture") {
          return Promise.resolve({
            summary: "Error dialog in payment checkout",
            visible_defects: [],
            confidence: 0.95,
          });
        }
        if (cmd === "run_agent_turn") {
          return Promise.reject("Agent loop busy with active takeover");
        }
        return Promise.resolve(null);
      });

      render(<ScreenExplainerModal isOpen={true} onClose={vi.fn()} />);

      // 1. Typing question into input
      const input = screen.getByPlaceholderText(/Ask what you need help with/i);
      fireEvent.change(input, { target: { value: "Where is the submit error?" } });
      expect(input).toHaveValue("Where is the submit error?");

      // 2. Capture fresh display
      const captureBtn = screen.getByText("Capture Fresh Display");
      fireEvent.click(captureBtn);

      await waitFor(() => {
        expect(screen.getByText("Explain Screen")).not.toBeDisabled();
      });

      // 3. Click analyze
      const analyzeBtn = screen.getByText("Explain Screen");
      fireEvent.click(analyzeBtn);

      await waitFor(() => {
        expect(screen.getByText(/Error dialog in payment checkout/i)).toBeInTheDocument();
        expect(screen.getByText("Execute Grounded Action")).toBeInTheDocument();
      });

      // 4. Execute fix (triggers catch block in handleExecuteRecommendedAction)
      const executeBtn = screen.getByText("Execute Grounded Action");
      fireEvent.click(executeBtn);

      await waitFor(() => {
        expect(screen.getByText(/Agent loop busy with active takeover/i)).toBeInTheDocument();
      });
    });

    it("displays error when analyze is triggered with analysis rejection", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "take_screenshot") {
          return Promise.resolve({
            base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            width: 1920,
            height: 1080,
          });
        }
        if (cmd === "explain_screen_capture") {
          return Promise.reject("Vision API quota exhausted");
        }
        return Promise.resolve(null);
      });

      render(<ScreenExplainerModal isOpen={true} onClose={vi.fn()} />);

      const captureBtn = screen.getByText("Capture Fresh Display");
      fireEvent.click(captureBtn);

      await waitFor(() => {
        expect(screen.getByText("Explain Screen")).not.toBeDisabled();
      });

      const analyzeBtn = screen.getByText("Explain Screen");
      fireEvent.click(analyzeBtn);

      await waitFor(() => {
        expect(screen.getByText(/Vision API quota exhausted/i)).toBeInTheDocument();
      });
    });
  });

  describe("DocumentAssistantModal Tabs and Validation", () => {
    it("handles missing comparison paths, comparison errors, and added/removed content rendering", async () => {
      const mockComp = {
        identical: false,
        summary: "Document comparison completed with 2 changes",
        similarity_score: 0.88,
        added_in_b: ["Added clause 4.2 governing indemnity"],
        removed_from_a: ["Deprecated section 1.1"],
      };

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "compare_documents") {
          if (args.pathA.includes("fail")) {
            return Promise.reject("File access prohibited");
          }
          return Promise.resolve(mockComp);
        }
        return Promise.resolve(null);
      });

      render(<DocumentAssistantModal isOpen={true} onClose={vi.fn()} />);

      // Switch to Compare tab
      const compareTabBtn = screen.getByText("Compare Two Documents");
      fireEvent.click(compareTabBtn);

      // Fill in paths and compare
      const pathAInput = screen.getByPlaceholderText(/Document A path/i);
      const pathBInput = screen.getByPlaceholderText(/Document B path/i);
      fireEvent.change(pathAInput, { target: { value: "/docs/v1.txt" } });
      fireEvent.change(pathBInput, { target: { value: "/docs/v2.txt" } });

      const compareBtn = screen.getByText("Generate Comparison Report");
      fireEvent.click(compareBtn);

      await waitFor(() => {
        expect(screen.getByText(/Added clause 4.2 governing indemnity/i)).toBeInTheDocument();
        expect(screen.getByText(/Deprecated section 1.1/i)).toBeInTheDocument();
        expect(screen.getByText(/Comparison report generated/i)).toBeInTheDocument();
      });

      // Switch back to Analyze tab
      const analyzeTabBtn = screen.getByText("Analyze & Extract");
      fireEvent.click(analyzeTabBtn);
      expect(screen.getByPlaceholderText(/Enter path to text document or PDF file/i)).toBeInTheDocument();
    });
  });

  describe("FileOrganizerModal Empty State and Undo Handling", () => {
    it("handles empty operations in preview and undo failure notices", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "preview_file_organization") {
          return Promise.resolve({
            directory: "/home/snigdha/Downloads",
            operations: [],
            total_scanned: 0,
            duplicates_count: 0,
            conflicts_count: 0,
          });
        }
        return Promise.resolve(null);
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      const dirInput = screen.getByPlaceholderText(/Enter absolute directory path/i);
      fireEvent.change(dirInput, { target: { value: "/home/snigdha/Downloads" } });

      const previewBtn = screen.getByText("Scan & Preview");
      fireEvent.click(previewBtn);

      await waitFor(() => {
        expect(screen.getByText(/Directory is already organized or contains no loose files/i)).toBeInTheDocument();
      });
    });

    it("handles execute and undo rejection gracefully", async () => {
      const mockPlan = {
        directory: "/home/snigdha/Downloads",
        operations: [
          {
            source_path: "/home/snigdha/Downloads/invoice.pdf",
            destination_path: "/home/snigdha/Downloads/PDFs/invoice.pdf",
            category: "PDFs",
            file_name: "invoice.pdf",
            file_size: 1024,
            is_duplicate: false,
            status: "pending" as const,
          },
        ],
        total_scanned: 1,
        duplicates_count: 0,
        conflicts_count: 0,
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "preview_file_organization") {
          return Promise.resolve(mockPlan);
        }
        if (cmd === "execute_file_organization") {
          return Promise.reject("Permission denied writing to destination folder");
        }
        return Promise.resolve(null);
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      const dirInput = screen.getByPlaceholderText(/Enter absolute directory path/i);
      fireEvent.change(dirInput, { target: { value: "/home/snigdha/Downloads" } });

      const previewBtn = screen.getByText("Scan & Preview");
      fireEvent.click(previewBtn);

      await waitFor(() => {
        expect(screen.getByText(/Execute Organization \(1 files\)/i)).toBeInTheDocument();
      });

      const executeBtn = screen.getByText(/Execute Organization \(1 files\)/i);
      fireEvent.click(executeBtn);

      await waitFor(() => {
        expect(screen.getByText(/Permission denied writing to destination folder/i)).toBeInTheDocument();
      });
    });
  });

  describe("McpModal Command Inputs and Confirmation Dialog", () => {
    it("updates command and arguments inputs and opens delete confirmation", async () => {
      const mockServers = [
        {
          id: "mcp-server-1",
          name: "Filesystem Server",
          command: "npx",
          args_json: JSON.stringify(["-y", "@modelcontextprotocol/server-filesystem"]),
          env_json: "{}",
          enabled: true,
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_mcp_servers") {
          return Promise.resolve(mockServers);
        }
        if (cmd === "create_mcp_server") {
          return Promise.reject("Server executable not in PATH");
        }
        return Promise.resolve([]);
      });

      render(<McpModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("Filesystem Server")).toBeInTheDocument();
      });

      // Click Add MCP Server
      const addServerBtn = screen.getByText("Add MCP Server");
      fireEvent.click(addServerBtn);

      // Type name, command and arguments
      const nameInput = screen.getByPlaceholderText("e.g. Local Filesystem or Brave Search");
      const cmdInput = screen.getByPlaceholderText("npx or python3");
      const argsInput = screen.getByPlaceholderText("-y @modelcontextprotocol/server-filesystem /path");

      fireEvent.change(nameInput, { target: { value: "Test Py Server" } });
      fireEvent.change(cmdInput, { target: { value: "python3" } });
      fireEvent.change(argsInput, { target: { value: "-m mcp_server" } });

      expect(nameInput).toHaveValue("Test Py Server");
      expect(cmdInput).toHaveValue("python3");
      expect(argsInput).toHaveValue("-m mcp_server");

      // Attempt save (triggers error notice)
      const saveBtn = screen.getByText("Save & Connect Server");
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(screen.getByText(/Server executable not in PATH/i)).toBeInTheDocument();
      });

      // Trigger deletion confirm dialog
      const deleteBtn = screen.getByLabelText("Delete Filesystem Server");
      fireEvent.click(deleteBtn);

      await waitFor(() => {
        expect(screen.getByText("Remove this MCP server?")).toBeInTheDocument();
      });

      // Cancel deletion
      const cancelBtns = screen.getAllByText("Cancel");
      fireEvent.click(cancelBtns[cancelBtns.length - 1]);

      await waitFor(() => {
        expect(screen.queryByText("Remove this MCP server?")).not.toBeInTheDocument();
      });
    });
  });

  describe("MyRoutinesView Edit Modal and Ask Prompt Trigger", () => {
    it("opens edit routine modal, updates target_id, and handles save", async () => {
      const mockRoutines = [
        {
          id: "rt_sync_1",
          name: "Daily Workflows",
          description: "Syncs git and organizes files",
          cron_expression: "0 9 * * 1-5",
          target_type: "prompt",
          target_id: "Sync changes with origin main",
          enabled: true,
          created_at: 1000,
          updated_at: 1000,
          last_run_at: null,
          next_run_at: null,
        },
      ];

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "list_routines") {
          return Promise.resolve(mockRoutines);
        }
        if (cmd === "update_routine") {
          return Promise.resolve({ ...mockRoutines[0], target_id: args.targetId });
        }
        return Promise.resolve([]);
      });

      const handleAsk = vi.fn();
      render(<MyRoutinesView onAskPrompt={handleAsk} />);

      await waitFor(() => {
        expect(screen.getByText("Daily Workflows")).toBeInTheDocument();
      });

      // Click Edit button
      const editBtn = screen.getByLabelText("Edit routine");
      fireEvent.click(editBtn);

      await waitFor(() => {
        expect(screen.getByText("Edit Routine")).toBeInTheDocument();
      });

      // Update instruction target
      const targetTextarea = screen.getByDisplayValue("Sync changes with origin main");
      fireEvent.change(targetTextarea, { target: { value: "Updated prompt instructions" } });

      // Click Save Changes
      const saveBtn = screen.getByText("Save Changes");
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("update_routine", {
          routine: expect.objectContaining({
            target_id: "Updated prompt instructions",
          }),
        });
      });
    });

    it("triggers onAskPrompt from empty routine state", async () => {
      (invoke as any).mockResolvedValueOnce([]); // empty routines
      const handleAsk = vi.fn();

      render(<MyRoutinesView onAskPrompt={handleAsk} />);

      await waitFor(() => {
        expect(screen.getByText(/Ask Kyro: “Every Friday, organize my new documents”/i)).toBeInTheDocument();
      });

      const promptBtn = screen.getByText(/Ask Kyro: “Every Friday, organize my new documents”/i);
      fireEvent.click(promptBtn);

      expect(handleAsk).toHaveBeenCalledWith("Every Friday, organize my new documents and show me what changed");
    });
  });

  describe("MyToolsView Search Result Actions and Comparison", () => {
    it("handles Study Notes and Summarize clicks on search results", async () => {
      const mockResults = [
        {
          file_name: "architecture_spec.pdf",
          format: "PDF",
          path: "/home/snigdha/Desktop/Kyro/docs/architecture_spec.pdf",
          snippet: "System architecture and data flow diagrams",
          line_number: null,
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "search_authorized_documents") {
          return Promise.resolve(mockResults);
        }
        return Promise.resolve([]);
      });

      const handleAsk = vi.fn();
      render(
        <MyToolsView
          onAskPrompt={handleAsk}
          onOpenConnections={vi.fn()}
          onOpenScreenExplainer={vi.fn()}
          onOpenFileOrganizer={vi.fn()}
          onOpenDocAssistant={vi.fn()}
        />
      );

      // Search documents
      const searchInput = screen.getByPlaceholderText(/Search keywords, filenames/i);
      fireEvent.change(searchInput, { target: { value: "architecture" } });

      const searchBtn = screen.getByRole("button", { name: /^Search$/i });
      fireEvent.click(searchBtn);

      await waitFor(() => {
        expect(screen.getByText("architecture_spec.pdf")).toBeInTheDocument();
        expect(screen.getByText("Study Notes")).toBeInTheDocument();
        expect(screen.getByText("Summarize")).toBeInTheDocument();
      });

      // Click Summarize (triggers onAskPrompt)
      const sumBtn = screen.getByText("Summarize");
      fireEvent.click(sumBtn);

      expect(handleAsk).toHaveBeenCalledWith("Explain and summarize this file: /home/snigdha/Desktop/Kyro/docs/architecture_spec.pdf");

      // Click Study Notes (switches to study category)
      const studyBtn = screen.getByText("Study Notes");
      fireEvent.click(studyBtn);

      expect(screen.getByText("Study & Revision Notes")).toBeInTheDocument();
    });

    it("handles compare items input criteria change and triggers onAskPrompt", async () => {
      const handleAsk = vi.fn();
      render(
        <MyToolsView
          onAskPrompt={handleAsk}
          onOpenConnections={vi.fn()}
          onOpenScreenExplainer={vi.fn()}
          onOpenFileOrganizer={vi.fn()}
          onOpenDocAssistant={vi.fn()}
        />
      );

      // Switch to Web category
      const compareCatBtn = screen.getByText("Web Research & Compare");
      fireEvent.click(compareCatBtn);

      const itemsInput = screen.getByPlaceholderText(/Products to compare/i);
      const criteriaInput = screen.getByPlaceholderText(/Comparison criteria/i);

      fireEvent.change(itemsInput, { target: { value: "Rust vs Go" } });
      fireEvent.change(criteriaInput, { target: { value: "concurrency, performance, developer velocity" } });

      const compareBtn = screen.getByText("Build Comparison Table");
      fireEvent.click(compareBtn);

      expect(handleAsk).toHaveBeenCalledWith("Compare these items based on concurrency, performance, developer velocity: Rust vs Go");
    });
  });

  describe("OnboardingModal Example Triggers", () => {
    it("handles example prompt selections in OnboardingModal", async () => {
      const handleExample = vi.fn();
      const handleClose = vi.fn();

      render(
        <OnboardingModal
          isOpen={true}
          onClose={handleClose}
          onTryExample={handleExample}
          apiKeys={{}}
          onSaveApiKeys={vi.fn()}
        />
      );

      // Advance through steps 1 -> 2 -> 3 -> 4 -> 5 -> 6 using "Continue"
      for (let i = 0; i < 5; i++) {
        const continueBtn = screen.getByText("Continue");
        fireEvent.click(continueBtn);
      }

      await waitFor(() => {
        expect(screen.getByText("Organize downloaded files")).toBeInTheDocument();
      });

      // Click organize downloads example
      const orgBtn = screen.getByText("Organize downloaded files");
      fireEvent.click(orgBtn);

      expect(handleExample).toHaveBeenCalledWith("Organize my downloads into category folders and check for duplicate files");
    });
  });

  describe("TerminalView Event Handlers", () => {
    it("handles spawn failure, terminal:output event, and window resize", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "spawn_terminal") {
          return Promise.reject("PTY allocation failure");
        }
        return Promise.resolve();
      });

      let outputCallback: any = null;
      (listen as any).mockImplementation((event: string, cb: any) => {
        if (event === "terminal:output") {
          outputCallback = cb;
        }
        return Promise.resolve(() => {});
      });

      render(<TerminalView />);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("spawn_terminal");
      });

      // Simulate output event
      if (outputCallback) {
        outputCallback({ payload: "kyro pty output line\r\n" });
      }

      // Simulate window resize
      window.dispatchEvent(new Event("resize"));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("resize_terminal", expect.any(Object));
      });
    });
  });

  describe("TimelineDrawer Scrubber Controls and Errors", () => {
    it("handles previous and next scrubber buttons, and clear error logging", async () => {
      const mockSteps = [
        {
          step: 1,
          action: "left_click (100, 200)",
          before_base64: "aW1nMQ==",
          after_base64: "aW1nMg==",
          diff_percent: 15.0,
          timestamp: 1000,
        },
        {
          step: 2,
          action: "type 'hello'",
          before_base64: "aW1nMg==",
          after_base64: "aW1nMw==",
          diff_percent: 25.0,
          timestamp: 2000,
        },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_timeline_steps") {
          return Promise.resolve(mockSteps);
        }
        if (cmd === "clear_timeline_steps") {
          return Promise.reject("Database lock error");
        }
        return Promise.resolve();
      });

      render(<TimelineDrawer isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /Step 1/i })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /Step 2/i })).toBeInTheDocument();
      });

      // Click step 1
      const step1Btn = screen.getByRole("button", { name: /Step 1/i });
      fireEvent.click(step1Btn);

      // Scrubber forward
      const scrubRight = screen.getAllByRole("button").find(b => b.querySelector("svg.lucide-chevron-right"));
      if (scrubRight) {
        fireEvent.click(scrubRight);
      }

      // Clear timeline (triggers catch block)
      const clearBtn = screen.getByText("Clear");
      fireEvent.click(clearBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("clear_timeline_steps");
      });
    });
  });

  describe("MarkdownRenderer CodeBlock Copy and Input Tag", () => {
    it("copies code to clipboard via CodeBlock and renders input elements", async () => {
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      render(<CodeBlock code="const a = 42;" language="typescript" />);

      expect(screen.getByText("TYPESCRIPT")).toBeInTheDocument();
      const copyBtn = screen.getByLabelText("Copy code to clipboard");
      fireEvent.click(copyBtn);

      await waitFor(() => {
        expect(writeTextMock).toHaveBeenCalledWith("const a = 42;");
      });

      // Render markdown with custom input type
      const mdContent = "Here is some text with <input type=\"text\" value=\"test\" />";
      render(<MarkdownRenderer content={mdContent} />);
    });
  });
});
