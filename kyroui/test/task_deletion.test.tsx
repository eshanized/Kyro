import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { invoke } from "@tauri-apps/api/core";

// Mock Tauri invoke API
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("TeachTaskModal - Task Deletion & Lifecycle", () => {
  const mockSkills = [
    {
      id: "skill-repo-review",
      name: "Daily Repository Review",
      description: "Review open PRs, check test status, and audit licenses",
      steps_json: JSON.stringify([
        { action_type: "click", x: 100, y: 200, delay_ms: 100 },
        { action_type: "type", text: "git status", delay_ms: 200 },
      ]),
      created_at: 1700000000000,
    },
    {
      id: "skill-export-metrics",
      name: "Export Weekly Metrics",
      description: "Export dashboard charts to CSV",
      steps_json: JSON.stringify([
        { action_type: "click", x: 400, y: 500, delay_ms: 150 },
      ]),
      created_at: 1700001000000,
    },
  ];

  const mockRoutines = [
    {
      id: "routine-morning-review",
      name: "Daily Morning Review",
      cron_expression: "0 9 * * *",
      action_type: "skill",
      target_id: "skill-repo-review",
      enabled: true,
      last_run: 1700050000000,
      next_run: 1700136400000,
    },
    {
      id: "routine-hourly-ping",
      name: "Hourly Service Ping",
      cron_expression: "0 * * * *",
      action_type: "prompt",
      target_id: "Check cluster health",
      enabled: true,
      last_run: null,
      next_run: 1700053600000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation(async (cmd: string, _args?: any) => {
      if (cmd === "list_skills") {
        return [...mockSkills];
      }
      if (cmd === "list_routines") {
        return [...mockRoutines];
      }
      if (cmd === "delete_skill" || cmd === "delete_routine") {
        return true;
      }
      return null;
    });
  });

  // 1. Delete action appears for eligible tasks with tooltip and accessible label
  it("renders a clearly discoverable delete action with tooltip and accessible label for each task", async () => {
    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    expect(deleteButtons.length).toBe(mockSkills.length);
    expect(deleteButtons[0]).toHaveAttribute("title", "Delete task");
    expect(deleteButtons[0]).toHaveAttribute("aria-label", "Delete task");
  });

  // 2. Explicit confirmation dialog identifying the task by name and consequences
  it("shows explicit confirmation dialog naming the selected task and consequences", async () => {
    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    // Dialog title & text
    expect(screen.getByRole("heading", { name: "Delete task?" })).toBeInTheDocument();
    expect(
      screen.getByText(/Are you sure you want to delete “Daily Repository Review”\? This action cannot be undone\./i)
    ).toBeInTheDocument();

    // Associated linked routine consequence
    expect(
      screen.getByText(/1 associated recurring routine targeting this skill definition will also be cleaned up/i)
    ).toBeInTheDocument();

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Delete task" })).toBeInTheDocument();
  });

  // 3. Canceling confirmation preserves the task
  it("canceling confirmation closes dialog and preserves the task without invoking backend deletion", async () => {
    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    expect(screen.getByRole("heading", { name: "Delete task?" })).toBeInTheDocument();

    // Click Cancel in dialog
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    // Confirmation dialog should be closed
    expect(screen.queryByRole("heading", { name: "Delete task?" })).not.toBeInTheDocument();

    // Backend deletion was never called
    expect(invoke).not.toHaveBeenCalledWith("delete_skill", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("delete_routine", expect.anything());

    // Task is preserved in view
    expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
  });

  // 4. Confirming deletion removes correct task using stable ID and refreshes view
  it("confirming deletion calls backend with stable identifier and removes task after success", async () => {
    let currentSkills = [...mockSkills];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_skills") {
        return currentSkills;
      }
      if (cmd === "list_routines") {
        return mockRoutines;
      }
      if (cmd === "delete_skill") {
        currentSkills = currentSkills.filter((s) => s.id !== args.id);
        return true;
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    // Confirm deletion inside modal
    const dialog = screen.getByRole("dialog");
    const confirmDeleteBtn = within(dialog).getByRole("button", { name: "Delete task" });
    fireEvent.click(confirmDeleteBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_skill", { id: "skill-repo-review" });
    });

    // Dialog closes
    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Delete task?" })).not.toBeInTheDocument();
    });

    // Task is removed from interface
    await waitFor(() => {
      expect(screen.queryByText("Daily Repository Review")).not.toBeInTheDocument();
    });

    // Other task is preserved
    expect(screen.getByText("Export Weekly Metrics")).toBeInTheDocument();
  });

  // 5. Deletion failures preserve or correctly restore UI state and display error
  it("deletion failure preserves UI state and displays error without optimistic removal", async () => {
    (invoke as any).mockImplementation(async (cmd: string, _args?: any) => {
      if (cmd === "list_skills") return mockSkills;
      if (cmd === "list_routines") return mockRoutines;
      if (cmd === "delete_skill") {
        throw "Cannot delete task 'skill-repo-review': task is currently executing. Please wait for execution to complete.";
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    const dialog = screen.getByRole("dialog");
    const confirmDeleteBtn = within(dialog).getByRole("button", { name: "Delete task" });
    fireEvent.click(confirmDeleteBtn);

    // Error alert is displayed inside the dialog
    await waitFor(() => {
      expect(
        screen.getByText(/Cannot delete task 'skill-repo-review': task is currently executing/i)
      ).toBeInTheDocument();
    });

    // Dialog remains open
    expect(screen.getByRole("heading", { name: "Delete task?" })).toBeInTheDocument();

    // Task is still preserved in the interface
    expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
  });

  // 6. Duplicate submissions prevented while deletion is in progress
  it("disables deletion buttons during in-progress deletion to prevent duplicate submissions", async () => {
    let resolveDelete: (value: any) => void;
    const pendingPromise = new Promise((resolve) => {
      resolveDelete = resolve;
    });

    (invoke as any).mockImplementation(async (cmd: string) => {
      if (cmd === "list_skills") return mockSkills;
      if (cmd === "list_routines") return mockRoutines;
      if (cmd === "delete_skill") {
        return pendingPromise;
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    const dialog = screen.getByRole("dialog");
    const confirmDeleteBtn = within(dialog).getByRole("button", { name: "Delete task" });
    const cancelBtn = within(dialog).getByRole("button", { name: "Cancel" });

    fireEvent.click(confirmDeleteBtn);

    // During in-flight request:
    expect(screen.getByText("Deleting...")).toBeInTheDocument();
    expect(confirmDeleteBtn).toBeDisabled();
    expect(cancelBtn).toBeDisabled();

    // Resolve deletion
    resolveDelete!(true);

    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Delete task?" })).not.toBeInTheDocument();
    });
  });

  // 7. Search/filter results update immediately upon deletion
  it("filtering tasks by search query and deleting updates filtered list consistently", async () => {
    let currentSkills = [...mockSkills];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_skills") return currentSkills;
      if (cmd === "list_routines") return mockRoutines;
      if (cmd === "delete_skill") {
        currentSkills = currentSkills.filter((s) => s.id !== args.id);
        return true;
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    // Search filter
    const searchInput = screen.getByPlaceholderText(/Search saved skills by name/i);
    fireEvent.change(searchInput, { target: { value: "Review" } });

    expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    expect(screen.queryByText("Export Weekly Metrics")).not.toBeInTheDocument();

    // Delete filtered task
    const deleteBtn = screen.getByRole("button", { name: "Delete task" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete task" }));

    await waitFor(() => {
      expect(screen.queryByText("Daily Repository Review")).not.toBeInTheDocument();
    });

    // Empty search state
    expect(screen.getByText(/No saved skills match “Review”/i)).toBeInTheDocument();
  });

  // 8. Recurring routine deletion and confirmation in routines view
  it("supports routine deletion with explicit confirmation dialog and removes routine", async () => {
    let currentRoutines = [...mockRoutines];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_skills") return mockSkills;
      if (cmd === "list_routines") return currentRoutines;
      if (cmd === "delete_routine") {
        currentRoutines = currentRoutines.filter((r) => r.id !== args.id);
        return true;
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} initialTab="routines" />);

    await waitFor(() => {
      expect(screen.getByText("Daily Morning Review")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete task" });
    fireEvent.click(deleteButtons[0]);

    // Confirmation dialog
    expect(screen.getByRole("heading", { name: "Delete task?" })).toBeInTheDocument();
    expect(
      screen.getByText(/Are you sure you want to delete “Daily Morning Review”\? This action cannot be undone\./i)
    ).toBeInTheDocument();

    // Confirm deletion
    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete task" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_routine", { id: "routine-morning-review" });
    });

    await waitFor(() => {
      expect(screen.queryByText("Daily Morning Review")).not.toBeInTheDocument();
    });

    // Other routine is preserved
    expect(screen.getByText("Hourly Service Ping")).toBeInTheDocument();
  });

  // 9. Task detail view provides delete task action with confirmation
  it("allows deleting a task directly from the task details view", async () => {
    let currentSkills = [...mockSkills];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_skills") return currentSkills;
      if (cmd === "list_routines") return mockRoutines;
      if (cmd === "delete_skill") {
        currentSkills = currentSkills.filter((s) => s.id !== args.id);
        return true;
      }
      return null;
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Daily Repository Review")).toBeInTheDocument();
    });

    // Click row to open details
    fireEvent.click(screen.getByText("Daily Repository Review"));

    // Verify detail view is opened showing steps
    expect(screen.getByText("Recorded Steps (2)")).toBeInTheDocument();

    // Click "Delete task" inside the detail view (first delete task button in the document)
    const detailDeleteBtn = screen.getAllByRole("button", { name: "Delete task" })[0];
    fireEvent.click(detailDeleteBtn);

    expect(screen.getByRole("heading", { name: "Delete task?" })).toBeInTheDocument();

    // Confirm deletion inside dialog
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete task" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_skill", { id: "skill-repo-review" });
    });

    await waitFor(() => {
      expect(screen.queryByText("Daily Repository Review")).not.toBeInTheDocument();
    });
  });
});
