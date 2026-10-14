import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { InlineNotice } from "../components/InlineNotice";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("ConfirmDialog", () => {
  it("is not rendered when it is closed", () => {
    render(
      <ConfirmDialog
        isOpen={false}
        title="Delete?"
        message="This cannot be undone."
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("is an accessible modal dialog when it is open", () => {
    render(
      <ConfirmDialog
        isOpen
        title="Delete artifact?"
        message="This cannot be undone."
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleName("Delete artifact?");
    expect(dialog).toHaveAccessibleDescription("This cannot be undone.");
  });

  it("moves the focus to the confirm button when it opens", () => {
    render(
      <ConfirmDialog
        isOpen
        title="Delete?"
        message="Cannot be undone."
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    const confirm = screen.getByRole("button", { name: "Confirm" });
    expect(document.activeElement).toBe(confirm);
  });

  it("calls onConfirm when the confirm button is pressed", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        isOpen
        title="Delete?"
        message="Cannot be undone."
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("calls onCancel when the cancel button is pressed", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        isOpen
        title="Delete?"
        message="Cannot be undone."
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("cancels on Escape", () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        isOpen
        title="Delete?"
        message="Cannot be undone."
        onConfirm={() => {}}
        onCancel={onCancel}
      />
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("does not cancel while the operation is busy", () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        isOpen
        isBusy
        title="Delete?"
        message="Cannot be undone."
        onConfirm={() => {}}
        onCancel={onCancel}
      />
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Working…" })).toBeDisabled();
  });
});

describe("InlineNotice", () => {
  it("announces an error with the alert role", () => {
    render(<InlineNotice tone="error" message="Something failed" />);
    const notice = screen.getByRole("alert");
    expect(notice).toHaveTextContent("Something failed");
  });

  it("announces a success with the status role", () => {
    render(<InlineNotice tone="success" message="It worked" />);
    expect(screen.getByRole("status")).toHaveTextContent("It worked");
  });

  it("can be dismissed", () => {
    const onDismiss = vi.fn();
    render(<InlineNotice tone="info" message="Note" onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss notification" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
