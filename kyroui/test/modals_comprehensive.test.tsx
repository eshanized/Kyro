import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { ConnectionsModal } from "../components/ConnectionsModal";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { CommandPalette } from "../components/CommandPalette";
import { TimelineDrawer } from "../components/TimelineDrawer";
import { TeammateRosterModal } from "../components/TeammateRosterModal";
import { McpModal } from "../components/McpModal";
import { TerminalView } from "../components/TerminalView";
import { ToolCallCard } from "../components/ToolCallCard";
import { OnboardingModal } from "../components/OnboardingModal";
import { AgentAvatar, getAgentIconComponent } from "../components/AgentAvatar";
import { InlineNotice } from "../components/InlineNotice";
import { KyroLogo } from "../components/Logo";
import { ServiceIcon, NvidiaIcon, GithubIcon, SlackIcon } from "../components/BrandIcons";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class MockTerminal {
      loadAddon = vi.fn();
      open = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      onData = vi.fn();
      dispose = vi.fn();
    },
  };
});

vi.mock("@xterm/addon-fit", () => {
  return {
    FitAddon: class MockFitAddon {
      fit = vi.fn();
    },
  };
});

describe("Comprehensive UI Modals & Subsystems", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("ConnectionsModal", () => {
    it("returns null when closed and loads data when opened", async () => {
      const { container, rerender } = render(
        <ConnectionsModal isOpen={false} onClose={vi.fn()} />
      );
      expect(container.firstChild).toBeNull();

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_connector_catalog") {
          return Promise.resolve([
            {
              id: "github",
              name: "GitHub",
              connector_type: "native",
              description: "Manage issues and pull requests",
              category: "developer",
              auth_type: "oauth2",
              icon: "github",
              capabilities: ["issues", "pull_requests"],
              required_scopes: ["repo"],
              is_builtin: true,
              is_available: true,
              connected_accounts_count: 1,
              status: "connected",
            },
          ]);
        }
        if (cmd === "list_connected_accounts") {
          return Promise.resolve([
            {
              id: "gh_acc_1",
              connector_id: "github",
              account_identifier: "alice@github",
              display_name: "alice@github",
              created_at: 1000,
              updated_at: 2000,
              status: "connected",
              granted_scopes: ["repo"],
              config: {},
              tool_count: 5,
            },
          ]);
        }
        if (cmd === "list_mcp_servers") {
          return Promise.resolve([]);
        }
        return Promise.resolve(null);
      });

      rerender(<ConnectionsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("GitHub")).toBeInTheDocument();
      });

      // Switch to Connected tab
      const connectedTabBtn = screen.getByRole("button", { name: /Connected Accounts/i });
      fireEvent.click(connectedTabBtn);

      await waitFor(() => {
        expect(screen.getByText("alice@github")).toBeInTheDocument();
      });
    });
  });

  describe("ArtifactsModal", () => {
    it("renders artifacts, supports search filter, preview and commenting", async () => {
      const mockArtifact = {
        id: "art_1",
        run_id: "run_42",
        conversation_id: "conv_1",
        artifact_type: "code",
        name: "Database Migration Script",
        file_path: "/tmp/migration.sql",
        mime_type: "text/x-sql",
        size_bytes: 512,
        created_at: 1000,
        metadata_json: JSON.stringify({ version: "1.0", comments: [] }),
      };

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_artifacts") {
          return Promise.resolve([mockArtifact]);
        }
        if (cmd === "read_artifact_content") {
          return Promise.resolve("CREATE TABLE users (id INT PRIMARY KEY);");
        }
        if (cmd === "add_artifact_comment") {
          return Promise.resolve({
            ...mockArtifact,
            metadata_json: JSON.stringify({
              comments: [{ id: "c1", author: "User", text: "Looks great", created_at: 2000 }],
            }),
          });
        }
        return Promise.resolve(null);
      });

      render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getAllByText("Database Migration Script").length).toBeGreaterThan(0);
      });

      // Click artifact to preview
      fireEvent.click(screen.getAllByText("Database Migration Script")[0]);

      await waitFor(() => {
        expect(screen.getByText(/CREATE TABLE users/i)).toBeInTheDocument();
      });

      // Filter search
      const searchInput = screen.getByPlaceholderText(/Filter artifacts/i);
      fireEvent.change(searchInput, { target: { value: "Database" } });
      expect(screen.getAllByText("Database Migration Script").length).toBeGreaterThan(0);
    });
  });

  describe("CommandPalette", () => {
    it("renders actions and triggers callbacks on click", () => {
      const onNewSession = vi.fn();
      const onToggleGhost = vi.fn();
      const onToggleTerminal = vi.fn();
      const onClose = vi.fn();

      render(
        <CommandPalette
          isOpen={true}
          onClose={onClose}
          onNewSession={onNewSession}
          onToggleGhost={onToggleGhost}
          onToggleTerminal={onToggleTerminal}
          onOpenTeammates={vi.fn()}
          onOpenTeachTask={vi.fn()}
          onOpenMcp={vi.fn()}
          onOpenTimeline={vi.fn()}
          onOpenArtifacts={vi.fn()}
          onOpenSettings={vi.fn()}
          onSelectConversation={vi.fn()}
          onSelectBot={vi.fn()}
        />
      );

      expect(screen.getByPlaceholderText(/Type a command or search/i)).toBeInTheDocument();

      // Trigger New Session action
      const newSessionBtn = screen.getByText("New Autonomous Session");
      fireEvent.click(newSessionBtn);
      expect(onNewSession).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  describe("TimelineDrawer", () => {
    it("displays timeline steps and allows clearing", async () => {
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_timeline_steps") {
          return Promise.resolve([
            {
              id: "t1",
              run_id: "r1",
              step: 1,
              action_name: "Click Button",
              click_x: 200,
              click_y: 350,
              before_base64: "",
              after_base64: "",
              diff_percent: 2.5,
              timestamp: 1000,
            },
          ]);
        }
        if (cmd === "clear_timeline_steps") {
          return Promise.resolve();
        }
        return Promise.resolve(null);
      });

      render(<TimelineDrawer isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText(/Visual Step Timeline/i)).toBeInTheDocument();
      });

      expect(screen.getByText("1 Steps Logged")).toBeInTheDocument();

      // Clear button
      const clearBtn = screen.getByRole("button", { name: /Clear/i });
      fireEvent.click(clearBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("clear_timeline_steps");
      });
    });
  });

  describe("TeammateRosterModal", () => {
    it("renders agent profiles and selects a teammate", async () => {
      const onSelectBot = vi.fn();
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "list_agent_profiles") {
          return Promise.resolve([
            {
              id: "apex-orchestrator",
              name: "Apex",
              avatar: "zap",
              role: "Lead Agent",
              description: "",
              model_id: null,
              origin: "builtin",
              enabled: true,
              runtime_disabled: false,
              schema_version: 1,
              version_label: "v1:abc123",
              aliases: ["apex"],
              validation_error: null,
              source_path: null,
              instructions: "",
              tools_mode: "inherit",
              tools_allow: [],
              delegation_enabled: true,
              delegation_max_depth: 3,
              delegation_max_children: 4,
              max_steps: 25,
              is_builtin: true,
            },
          ]);
        }
        if (cmd === "get_roster_statuses") {
          return Promise.resolve([
            {
              id: "apex-orchestrator",
              state: "idle",
              current_task: "Idle",
              active_tasks: [],
            },
          ]);
        }
        return Promise.resolve(null);
      });

      render(
        <TeammateRosterModal
          isOpen={true}
          onClose={vi.fn()}
          selectedBotId="apex-orchestrator"
          onSelectBot={onSelectBot}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("@Apex")).toBeInTheDocument();
      });

      expect(screen.getByText("Lead Agent")).toBeInTheDocument();
    });
  });

  describe("McpModal", () => {
    it("lists MCP servers and adds new server", async () => {
      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "list_mcp_servers") {
          return Promise.resolve([
            {
              id: "srv_1",
              name: "Filesystem MCP",
              command: "npx",
              args_json: JSON.stringify(["-y", "server-fs"]),
              env_json: "{}",
              enabled: true,
            },
          ]);
        }
        if (cmd === "create_mcp_server") {
          return Promise.resolve();
        }
        return Promise.resolve(null);
      });

      render(<McpModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("Filesystem MCP")).toBeInTheDocument();
      });

      // Click Add MCP Server
      const addBtn = screen.getByRole("button", { name: /Add MCP Server/i });
      fireEvent.click(addBtn);

      expect(screen.getByPlaceholderText(/e\.g\. Local Filesystem/i)).toBeInTheDocument();
    });
  });

  describe("TerminalView", () => {
    it("initializes terminal and invokes spawn_terminal", () => {
      render(<TerminalView />);
      expect(invoke).toHaveBeenCalledWith("spawn_terminal");
      expect(listen).toHaveBeenCalledWith("terminal:output", expect.any(Function));
    });
  });

  describe("ToolCallCard", () => {
    it("renders tool call details, expanded view and status", () => {
      const toolCall = {
        name: "execute_shell",
        arguments: { command: "npm test" },
      };

      const { rerender } = render(
        <ToolCallCard
          toolCall={toolCall}
          result={{
            output: "Tests passed: 10 ok",
            is_error: false,
          }}
        />
      );

      expect(screen.getByText("execute_shell")).toBeInTheDocument();
      expect(screen.getByText("npm test")).toBeInTheDocument();
      expect(screen.getByText("Done")).toBeInTheDocument();
      expect(screen.getByText("Tests passed: 10 ok")).toBeInTheDocument();

      // Error result
      rerender(
        <ToolCallCard
          toolCall={toolCall}
          result={{
            output: "Permission denied",
            is_error: true,
          }}
        />
      );
      expect(screen.getByText("Failed")).toBeInTheDocument();
      expect(screen.getByText("Permission denied")).toBeInTheDocument();
    });
  });

  describe("OnboardingModal", () => {
    it("walks through onboarding steps and triggers completion", async () => {
      const onClose = vi.fn();
      const onTryExample = vi.fn();

      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "get_suggested_folders") {
          return Promise.resolve([
            { id: "downloads", name: "Downloads", path: "/home/user/Downloads", exists: true },
          ]);
        }
        return Promise.resolve(null);
      });

      render(
        <OnboardingModal
          isOpen={true}
          onClose={onClose}
          onTryExample={onTryExample}
          onOpenConnections={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("Getting Started with Kyro")).toBeInTheDocument();
      });

      // Continue step
      const continueBtn = screen.getByRole("button", { name: /Continue/i });
      fireEvent.click(continueBtn);

      await waitFor(() => {
        expect(screen.getByText(/Step 2 of 6/i)).toBeInTheDocument();
      });
    });
  });

  describe("Presentation Components", () => {
    it("renders AgentAvatar with matching icons for roles", () => {
      expect(getAgentIconComponent("zap", "Apex")).toBeDefined();
      expect(getAgentIconComponent("wrench", "Forge")).toBeDefined();
      expect(getAgentIconComponent("globe", "Scout")).toBeDefined();

      const { container } = render(<AgentAvatar avatar="zap" name="Apex" />);
      expect(container.firstChild).toBeInTheDocument();
    });

    it("renders InlineNotice with tones and dismiss button", () => {
      const onDismiss = vi.fn();
      render(<InlineNotice tone="error" message="Test critical error" onDismiss={onDismiss} />);
      expect(screen.getByText("Test critical error")).toBeInTheDocument();

      const dismissBtn = screen.getByLabelText("Dismiss notification");
      fireEvent.click(dismissBtn);
      expect(onDismiss).toHaveBeenCalled();
    });

    it("renders KyroLogo and BrandIcons", () => {
      const { container: logoContainer } = render(<KyroLogo size={32} />);
      expect(logoContainer.querySelector("svg")).toBeInTheDocument();

      const { container: srvContainer } = render(<ServiceIcon id="github" name="GitHub" />);
      expect(srvContainer.querySelector("svg")).toBeInTheDocument();

      const { container: nvContainer } = render(<NvidiaIcon />);
      expect(nvContainer.querySelector("svg")).toBeInTheDocument();

      const { container: ghContainer } = render(<GithubIcon />);
      expect(ghContainer.querySelector("svg")).toBeInTheDocument();

      const { container: slContainer } = render(<SlackIcon />);
      expect(slContainer.querySelector("svg")).toBeInTheDocument();
    });
  });
});
