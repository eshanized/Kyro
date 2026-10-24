import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { FileOrganizerModal } from "../components/FileOrganizerModal";
import { DocumentAssistantModal } from "../components/DocumentAssistantModal";
import { ScreenExplainerModal } from "../components/ScreenExplainerModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("Kyro Everyday Workflows & Modals", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("FileOrganizerModal", () => {
    it("renders file organizer form, handles preview and execution with safe undo", async () => {
      const mockPlan = {
        plan_id: "plan-123",
        target_directory: "/home/user/Downloads",
        total_scanned: 5,
        duplicates_count: 1,
        conflicts_count: 0,
        operations: [
          {
            source_path: "/home/user/Downloads/report.pdf",
            destination_path: "/home/user/Downloads/Documents/report.pdf",
            category: "Documents",
            reason: "Extension .pdf",
            file_size: 1024,
            is_duplicate: false,
          },
          {
            source_path: "/home/user/Downloads/report_copy.pdf",
            destination_path: "/home/user/Downloads/Duplicates/report_copy.pdf",
            category: "Duplicates",
            reason: "Identical SHA-256 hash",
            file_size: 1024,
            is_duplicate: true,
            duplicate_of: "/home/user/Downloads/report.pdf",
          },
        ],
      };

      const mockTx = {
        transaction_id: "tx-456",
        target_directory: "/home/user/Downloads",
        created_at: 1700000000,
        status: "completed" as const,
        operations: [
          {
            source_path: "/home/user/Downloads/report.pdf",
            destination_path: "/home/user/Downloads/Documents/report.pdf",
            file_hash: "hash123",
            file_size: 1024,
            status: "completed" as const,
          },
        ],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "preview_file_organization") {
          return Promise.resolve(mockPlan);
        }
        if (cmd === "execute_file_organization") {
          return Promise.resolve(mockTx);
        }
        if (cmd === "undo_file_organization") {
          return Promise.resolve(true);
        }
        return Promise.resolve(null);
      });

      const handleClose = vi.fn();
      render(<FileOrganizerModal isOpen={true} onClose={handleClose} />);

      expect(screen.getByText("Safe File Organizer")).toBeInTheDocument();

      // Fill in path
      const pathInput = screen.getByPlaceholderText(/Enter absolute directory path/);
      fireEvent.change(pathInput, { target: { value: "/home/user/Downloads" } });

      // Trigger preview with Scan & Preview button
      const previewBtn = screen.getByText("Scan & Preview");
      fireEvent.click(previewBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("preview_file_organization", expect.objectContaining({
          directory: "/home/user/Downloads",
        }));
        expect(screen.getByText("Content Duplicates")).toBeInTheDocument();
      });

      // Apply changes
      const applyBtn = screen.getByRole("button", { name: /Execute Organization/i });
      fireEvent.click(applyBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("execute_file_organization", { plan: mockPlan });
        expect(screen.getByText("Undo Last Move")).toBeInTheDocument();
      });

      // Trigger Undo
      fireEvent.click(screen.getByText("Undo Last Move"));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("undo_file_organization", { transaction: mockTx });
      });
    });

    it("loads authorized folders and populates directory on chip click", async () => {
      const mockFolders = [
        { id: "f1", path: "/home/user/Documents", display_name: "My Documents", is_active: true, created_at: 1000 },
        { id: "f2", path: "/home/user/Downloads", display_name: "Downloads", is_active: true, created_at: 1001 },
      ];

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_authorized_folders") return Promise.resolve(mockFolders);
        if (cmd === "list_file_transactions") return Promise.resolve([]);
        return Promise.resolve(null);
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("My Documents")).toBeInTheDocument();
        expect(screen.getByText("Downloads")).toBeInTheDocument();
      });

      const docChip = screen.getByText("My Documents");
      fireEvent.click(docChip);

      const pathInput = screen.getByPlaceholderText(/Enter absolute directory path/) as HTMLInputElement;
      expect(pathInput.value).toBe("/home/user/Documents");
    });

    it("invalidates current plan when directory or settings change", async () => {
      const mockPlan = {
        plan_id: "plan-invalidate",
        target_directory: "/home/user/Downloads",
        total_scanned: 2,
        duplicates_count: 0,
        conflicts_count: 0,
        scan_warnings: ["Symlink skipped: symlink.txt"],
        operations: [
          {
            source_path: "/home/user/Downloads/note.txt",
            destination_path: "/home/user/Downloads/Documents/note.txt",
            category: "Documents",
            reason: "Extension .txt",
            file_size: 50,
            is_duplicate: false,
          },
        ],
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "preview_file_organization") return Promise.resolve(mockPlan);
        return Promise.resolve([]);
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      const pathInput = screen.getByPlaceholderText(/Enter absolute directory path/);
      fireEvent.change(pathInput, { target: { value: "/home/user/Downloads" } });
      fireEvent.click(screen.getByText("Scan & Preview"));

      await waitFor(() => {
        expect(screen.getByText("Scan Warnings (1)")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /Execute Organization/i })).toBeInTheDocument();
      });

      // Changing directory invalidates the plan
      fireEvent.change(pathInput, { target: { value: "/home/user/Music" } });
      expect(screen.queryByRole("button", { name: /Execute Organization/i })).not.toBeInTheDocument();
      expect(screen.queryByText("Scan Warnings (1)")).not.toBeInTheDocument();
    });

    it("displays partial failures and per-file errors with undo recovery", async () => {
      const mockPlan = {
        plan_id: "plan-partial",
        target_directory: "/home/user/Downloads",
        total_scanned: 2,
        duplicates_count: 0,
        conflicts_count: 1,
        operations: [
          {
            source_path: "/home/user/Downloads/fail.txt",
            destination_path: "/home/user/Downloads/Documents/fail.txt",
            category: "Documents",
            reason: "Extension .txt",
            file_size: 100,
            is_duplicate: false,
          },
        ],
      };

      const mockPartialTx = {
        transaction_id: "tx-partial-999",
        target_directory: "/home/user/Downloads",
        created_at: 1700000000,
        status: "partially_failed" as const,
        operations: [
          {
            source_path: "/home/user/Downloads/fail.txt",
            destination_path: "/home/user/Downloads/Documents/fail.txt",
            file_hash: "hash_fail",
            file_size: 100,
            status: "failed" as const,
            error: "Source file modified or stale hash",
          },
        ],
      };

      const mockUndoneTx = {
        ...mockPartialTx,
        status: "undone" as const,
        operations: [
          {
            ...mockPartialTx.operations[0],
            status: "undone" as const,
          },
        ],
      };

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "preview_file_organization") return Promise.resolve(mockPlan);
        if (cmd === "execute_file_organization") return Promise.resolve(mockPartialTx);
        if (cmd === "undo_file_organization") return Promise.resolve(1);
        if (cmd === "get_file_transaction") return Promise.resolve(mockUndoneTx);
        return Promise.resolve([]);
      });

      render(<FileOrganizerModal isOpen={true} onClose={vi.fn()} />);

      const pathInput = screen.getByPlaceholderText(/Enter absolute directory path/);
      fireEvent.change(pathInput, { target: { value: "/home/user/Downloads" } });
      fireEvent.click(screen.getByText("Scan & Preview"));

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /Execute Organization/i })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: /Execute Organization/i }));

      await waitFor(() => {
        expect(screen.getByText(/Partially organized/)).toBeInTheDocument();
        expect(screen.getByText("Source file modified or stale hash")).toBeInTheDocument();
        expect(screen.getByText("partially_failed")).toBeInTheDocument();
      });

      // Click Undo Transaction
      fireEvent.click(screen.getByRole("button", { name: /Undo Transaction/i }));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("undo_file_organization", { transaction: mockPartialTx });
        expect(invoke).toHaveBeenCalledWith("get_file_transaction", { transactionId: "tx-partial-999" });
        expect(screen.getByText(/Successfully reversed 1 file moves/)).toBeInTheDocument();
      });
    });
  });

  describe("DocumentAssistantModal", () => {
    it("renders document analysis and executes local extraction", async () => {
      const mockAnalysis = {
        metadata: {
          file_name: "contract.pdf",
          file_path: "/home/user/contract.pdf",
          format: "PDF (Local)",
          size_bytes: 4096,
          page_count: 2,
          word_count: 120,
          character_count: 750,
          is_local_only: true,
        },
        summary: "This contract establishes service parameters and SLA deadlines.",
        key_facts: ["Effective date: 2026-11-01", "Confidentiality binding for 3 years"],
        action_items: [
          { text: "Sign the NDA", priority: "high" as const, line_number: 12 },
          { text: "Review invoice schedule", priority: "medium" as const, line_number: 30 },
        ],
        extracted_dates: [
          { raw_date: "2026-11-01", context: "Effective start date" },
          { raw_date: "2026-12-15", context: "First deliverable deadline" },
        ],
        missing_or_ambiguous: [],
        sections: [{ heading: "Scope", content: "Agentic automation deliverables" }],
        privacy_disclosure: "Extraction processed 100% locally on device.",
        raw_text: "Contract full text sample",
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "analyze_document") {
          return Promise.resolve(mockAnalysis);
        }
        return Promise.resolve(null);
      });

      const handleClose = vi.fn();
      render(<DocumentAssistantModal isOpen={true} onClose={handleClose} />);

      expect(screen.getByText("Local Document Assistant")).toBeInTheDocument();

      // Enter document path
      const docInput = screen.getByPlaceholderText(/Enter path to text document or PDF file/);
      fireEvent.change(docInput, { target: { value: "/home/user/contract.pdf" } });

      // Trigger analysis
      const analyzeBtn = screen.getByText("Analyze");
      fireEvent.click(analyzeBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("analyze_document", {
          path: "/home/user/contract.pdf",
          model: null,
        });
        expect(screen.getByText("This contract establishes service parameters and SLA deadlines.")).toBeInTheDocument();
        expect(screen.getByText("Sign the NDA")).toBeInTheDocument();
        expect(screen.getByText("2026-11-01")).toBeInTheDocument();
      });
    });
  });

  describe("ScreenExplainerModal", () => {
    it("renders explain screen modal with visual breakdown", async () => {
      const mockResult = {
        summary: "A form validation error is visible in the password field.",
        key_regions: [],
        visible_defects: [
          { defect_type: "Validation Error", description: "Red outline around password box", severity: "high" },
        ],
        discrepancies: [],
        confidence: "high",
        supporting_evidence: "CSS class 'has-error' and red text observed in snapshot",
        uncertainty: "none",
        vision_supported: true,
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "explain_screen_capture") {
          return Promise.resolve(mockResult);
        }
        return Promise.resolve(null);
      });

      const handleClose = vi.fn();
      render(
        <ScreenExplainerModal
          isOpen={true}
          onClose={handleClose}
          initialScreenshotBase64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
        />
      );

      expect(screen.getByText("Explain My Screen")).toBeInTheDocument();

      // Trigger analysis
      const explainBtn = screen.getByRole("button", { name: /Explain Screen/i });
      fireEvent.click(explainBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("explain_screen_capture", {
          base64Png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
          question: "Explain what is happening on this screen and what I should do next.",
          model: null,
        });
        expect(screen.getByText("A form validation error is visible in the password field.")).toBeInTheDocument();
        expect(screen.getByText("Red outline around password box")).toBeInTheDocument();
      });
    });
  });
});
