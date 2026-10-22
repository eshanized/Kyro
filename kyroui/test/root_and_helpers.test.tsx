import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { AgentAvatar, getAgentIconComponent } from "../components/AgentAvatar";
import {
  GithubIcon,
  GitlabIcon,
  SlackIcon,
  LinearIcon,
  GoogleDriveIcon,
  GoogleCalendarIcon,
  GmailIcon,
  TelegramIcon,
  NvidiaIcon,
  ServiceIcon,
} from "../components/BrandIcons";
import { ToolCallCard } from "../components/ToolCallCard";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Bot, Zap, Wrench, Globe, Terminal, Code, Cpu, Sparkles, Shield, User } from "lucide-react";

describe("AgentAvatar and Icon Helpers", () => {
  it("resolves correct icon component for each avatar / name pattern", () => {
    expect(getAgentIconComponent("\u26A1")).toBe(Zap);
    expect(getAgentIconComponent("zap")).toBe(Zap);
    expect(getAgentIconComponent(undefined, "Apex Orchestrator")).toBe(Zap);

    expect(getAgentIconComponent("\uD83D\uDEE0")).toBe(Wrench);
    expect(getAgentIconComponent("\uD83D\uDEE0\uFE0F")).toBe(Wrench);
    expect(getAgentIconComponent("wrench")).toBe(Wrench);
    expect(getAgentIconComponent(undefined, "Forge Engineer")).toBe(Wrench);

    expect(getAgentIconComponent("\uD83C\uDF10")).toBe(Globe);
    expect(getAgentIconComponent("globe")).toBe(Globe);
    expect(getAgentIconComponent(undefined, "Scout Navigator")).toBe(Globe);

    expect(getAgentIconComponent("terminal")).toBe(Terminal);
    expect(getAgentIconComponent("pty")).toBe(Terminal);

    expect(getAgentIconComponent("code")).toBe(Code);
    expect(getAgentIconComponent("cpu")).toBe(Cpu);
    expect(getAgentIconComponent("sparkles")).toBe(Sparkles);
    expect(getAgentIconComponent("shield")).toBe(Shield);
    expect(getAgentIconComponent("user")).toBe(User);

    expect(getAgentIconComponent("unknown_robot", "random")).toBe(Bot);
    expect(getAgentIconComponent()).toBe(Bot);
  });

  it("renders AgentAvatar with various styles and props", () => {
    const { container, rerender } = render(<AgentAvatar avatar="zap" name="Apex" />);
    expect(container.firstChild).toBeInTheDocument();

    rerender(<AgentAvatar avatar="shield" name="Security Officer" className="custom-class" iconClassName="custom-icon" />);
    expect(container.firstChild).toHaveClass("custom-class");
  });
});

describe("BrandIcons and ServiceIcon", () => {
  it("renders all individual SVG brand icons", () => {
    const icons = [
      <GithubIcon key="gh" data-testid="gh-icon" size={20} />,
      <GitlabIcon key="gl" data-testid="gl-icon" size={20} />,
      <SlackIcon key="sl" data-testid="sl-icon" size={20} />,
      <LinearIcon key="ln" data-testid="ln-icon" size={20} />,
      <GoogleDriveIcon key="gd" data-testid="gd-icon" size={20} />,
      <GoogleCalendarIcon key="gc" data-testid="gc-icon" size={20} />,
      <GmailIcon key="gm" data-testid="gm-icon" size={20} />,
      <TelegramIcon key="tg" data-testid="tg-icon" size={20} />,
      <NvidiaIcon key="nv" data-testid="nv-icon" size={20} />,
    ];

    icons.forEach((icon) => {
      const { unmount } = render(icon);
      unmount();
    });
  });

  it("resolves matching ServiceIcon for diverse service identifiers", () => {
    const testCases = [
      { id: "github", label: "GitHub" },
      { id: "gitlab", label: "GitLab" },
      { id: "slack", label: "Slack" },
      { id: "linear", label: "Linear" },
      { id: "google_drive", label: "Google Drive" },
      { id: "gcalendar", label: "Google Calendar" },
      { id: "gmail", label: "Gmail" },
      { id: "telegram", label: "Telegram" },
      { id: "nvidia_nemotron", label: "NVIDIA" },
      { id: "remote_mcp_http", label: "Remote MCP" },
      { id: "local_mcp_stdio", label: "Local MCP" },
      { id: "mcp_tool_test", label: "MCP Tool" },
      { id: "custom_service_xyz", label: "Custom Service" },
    ];

    testCases.forEach((tc) => {
      const { container, unmount } = render(<ServiceIcon id={tc.id} name={tc.label} size={16} />);
      expect(container.firstChild).toBeInTheDocument();
      unmount();
    });
  });
});

describe("ToolCallCard Component", () => {
  it("renders shell execution with command preview and success result", () => {
    const toolCall = {
      id: "call_1",
      name: "execute_shell",
      arguments: { command: "cargo test --all" },
    };
    const result = {
      tool_call_id: "call_1",
      output: "test result: ok. 14 passed",
      is_error: false,
    };

    render(<ToolCallCard toolCall={toolCall} result={result} />);

    expect(screen.getByText("execute_shell")).toBeInTheDocument();
    expect(screen.getByText("cargo test --all")).toBeInTheDocument();
    expect(screen.getByText(/Done/i)).toBeInTheDocument();
    expect(screen.getByText("test result: ok. 14 passed")).toBeInTheDocument();

    // Toggle collapse
    fireEvent.click(screen.getByText("execute_shell"));
    expect(screen.queryByText("test result: ok. 14 passed")).not.toBeInTheDocument();

    // Toggle expand
    fireEvent.click(screen.getByText("execute_shell"));
    expect(screen.getByText("test result: ok. 14 passed")).toBeInTheDocument();
  });

  it("renders computer action with coordinates and error result", () => {
    const toolCall = {
      id: "call_2",
      name: "computer_action",
      arguments: { action: "click", x: 150, y: 300 },
    };
    const result = {
      tool_call_id: "call_2",
      output: "Failed to locate target coordinates",
      is_error: true,
    };

    render(<ToolCallCard toolCall={toolCall} result={result} />);

    expect(screen.getByText("computer_action")).toBeInTheDocument();
    expect(screen.getByText("click (150, 300)")).toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("renders file reading and capture screen tools without coordinates", () => {
    const readFileCall = {
      id: "call_3",
      name: "read_file",
      arguments: { path: "/workspace/config.json" },
    };
    const { unmount } = render(<ToolCallCard toolCall={readFileCall} />);
    expect(screen.getByText("/workspace/config.json")).toBeInTheDocument();
    expect(screen.getByText(/Running/i)).toBeInTheDocument();
    unmount();

    const captureScreenCall = {
      id: "call_4",
      name: "capture_screen",
      arguments: {},
    };
    render(<ToolCallCard toolCall={captureScreenCall} />);
    expect(screen.getByText("capture_screen")).toBeInTheDocument();
  });
});

describe("ConfirmDialog Component", () => {
  it("renders nothing when isOpen is false", () => {
    const { container } = render(
      <ConfirmDialog
        isOpen={false}
        title="Delete Item"
        message="Are you sure?"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it("calls onConfirm and onCancel on button clicks", () => {
    const handleConfirm = vi.fn();
    const handleCancel = vi.fn();

    render(
      <ConfirmDialog
        isOpen={true}
        title="Delete Project"
        message="This action cannot be undone."
        confirmLabel="Destroy"
        cancelLabel="Keep"
        destructive={true}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    );

    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    expect(screen.getByText("Delete Project")).toBeInTheDocument();
    expect(screen.getByText("This action cannot be undone.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Destroy" }));
    expect(handleConfirm).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(handleCancel).toHaveBeenCalledTimes(1);
  });

  it("handles Escape key to cancel", () => {
    const handleCancel = vi.fn();

    render(
      <ConfirmDialog
        isOpen={true}
        title="Discard Draft"
        message="Draft will be lost."
        onConfirm={vi.fn()}
        onCancel={handleCancel}
      />
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(handleCancel).toHaveBeenCalledTimes(1);
  });

  it("handles Tab navigation to trap focus inside the dialog", () => {
    render(
      <ConfirmDialog
        isOpen={true}
        title="Confirmation Required"
        message="Please verify."
        confirmLabel="Proceed"
        cancelLabel="Abort"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    const abortBtn = screen.getByRole("button", { name: "Abort" });
    const proceedBtn = screen.getByRole("button", { name: "Proceed" });

    // Focus first button and Tab with Shift
    proceedBtn.focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });

    // Focus last button and Tab
    abortBtn.focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: false });
  });

  it("handles backdrop clicks to cancel when not busy", () => {
    const handleCancel = vi.fn();

    const { container } = render(
      <ConfirmDialog
        isOpen={true}
        title="Backdrop Test"
        message="Click outside to dismiss."
        isBusy={false}
        onConfirm={vi.fn()}
        onCancel={handleCancel}
      />
    );

    const backdrop = container.firstChild as HTMLElement;
    fireEvent.mouseDown(backdrop);
    expect(handleCancel).toHaveBeenCalledTimes(1);
  });

  it("disables buttons and prevents cancellation when isBusy is true", () => {
    const handleCancel = vi.fn();

    render(
      <ConfirmDialog
        isOpen={true}
        title="Processing Transaction"
        message="Please wait..."
        isBusy={true}
        onConfirm={vi.fn()}
        onCancel={handleCancel}
      />
    );

    const confirmBtn = screen.getByRole("button", { name: /Working/i });
    const cancelBtn = screen.getByRole("button", { name: "Cancel" });

    expect(confirmBtn).toBeDisabled();
    expect(cancelBtn).toBeDisabled();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(handleCancel).not.toHaveBeenCalled();
  });
});
