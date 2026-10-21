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
