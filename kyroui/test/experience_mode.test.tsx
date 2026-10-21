import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("Kyro Dual Experience Modes (Everyday vs Dev)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();

    (invoke as any).mockImplementation((cmd: string) => {
      switch (cmd) {
        case "list_conversations":
          return Promise.resolve([]);
        case "get_default_bot":
          return Promise.resolve({
            id: "apex-orchestrator",
            name: "Apex",
            avatar: "zap",
            role: "General Workspace & Strategic Orchestrator",
            system_prompt: "",
            is_default: true,
          });
        case "get_takeover_status":
          return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
        case "list_pending_approvals":
          return Promise.resolve([]);
        case "list_dev_servers":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
  });

  it("renders Kyro Everyday mode by default with everyday quick action cards", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
    });

    expect(
      screen.getByText(
        "Understand screens, safely organize downloads, analyze documents, and automate repetitive tasks with zero developer jargon."
      )
    ).toBeInTheDocument();

    // Verify Everyday action buttons & cards exist across sidebar and hero
    expect(screen.getAllByText("Explain Screen").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Organize Files").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Document Assistant").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Automate Routine").length).toBeGreaterThanOrEqual(1);
  });

  it("switches to Kyro Dev mode when Dev Mode button is clicked", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
    });

    // Find and click Dev Mode toggle in sidebar
    const devModeButton = screen.getByTitle(
      "Kyro Dev: Autonomous developer agent, dev servers, diffs, checkpoints & visual regression"
    );
    fireEvent.click(devModeButton);

    // Verify Dev mode hero elements are displayed
    await waitFor(() => {
      expect(screen.getByText("Kyro Autonomous Workspace")).toBeInTheDocument();
      expect(screen.getByText("DEV")).toBeInTheDocument();
    });

    expect(screen.getByText("Dev Servers")).toBeInTheDocument();
    expect(screen.getByText("Code Checkpoints")).toBeInTheDocument();
    expect(screen.getByText("Build Doctor")).toBeInTheDocument();
    expect(screen.getByText("Visual Baselines")).toBeInTheDocument();

    // Check localStorage was updated
    expect(localStorage.getItem("kyro_experience_mode")).toBe("dev");
  });

  it("switches back to Everyday mode seamlessly", async () => {
    localStorage.setItem("kyro_experience_mode", "dev");
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Kyro Autonomous Workspace")).toBeInTheDocument();
    });

    const everydayButton = screen.getByTitle(
      "Kyro Everyday: Screen intelligence, file organizer, document review & automations"
    );
    fireEvent.click(everydayButton);

    await waitFor(() => {
      expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
    });
    expect(localStorage.getItem("kyro_experience_mode")).toBe("everyday");
  });
});
