import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { SettingsModal } from "../components/SettingsModal";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}));

describe("SettingsModal Browser Automation", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_api_keys") {
        return Promise.resolve({ nvidia_configured: false, telegram_configured: false });
      }
      if (cmd === "get_governance_policy") {
        return Promise.resolve({
          shell_mode: "SafeAutoApprove",
          computer_use_mode: "FullAutonomous",
          require_takeover_for_credentials: true,
          blocked_domains: [],
        });
      }
      if (cmd === "get_sandbox_status") {
        return Promise.resolve({ container_available: false });
      }
      if (cmd === "get_system_health") {
        return Promise.resolve({
          db_connected: true,
          db_wal_mode: true,
          docker_available: false,
          docker_message: "",
          ghost_xvfb_available: false,
          os_keyring_available: true,
          display_server: "x11",
          tools: {},
        });
      }
      if (cmd === "get_starter_templates") {
        return Promise.resolve([]);
      }
      if (cmd === "get_browser_capabilities") {
        return Promise.resolve({
          browsers: [
            {
              id: "firefox",
              name: "firefox",
              display_name: "Mozilla Firefox",
              family: "firefox",
              path: "/usr/bin/firefox",
              version: "128.0",
              available: true,
              automatable: true,
              adapter: "webdriver-bidi",
            },
            {
              id: "chromium",
              name: "chromium",
              display_name: "Chromium",
              family: "chromium",
              path: "/usr/bin/chromium",
              version: "126.0",
              available: true,
              automatable: true,
              adapter: "cdp",
            },
            {
              id: "safari",
              name: "safari",
              display_name: "Apple Safari",
              family: "safari",
              path: "",
              available: false,
              automatable: false,
              adapter: "none",
              unavailable_reason: "Safari is not automatable on Linux",
            },
          ],
          default_browser_id: "firefox",
          driver: {
            name: "kyro-browser-engine",
            version: "0.2.0",
            status: "ready",
            bidi_supported: true,
            cdp_supported: true,
            external_driver_required: false,
          },
        });
      }
      if (cmd === "get_browser_settings") {
        return Promise.resolve({
          selected_browser: "auto",
          headless_default: true,
        });
      }
      if (cmd === "save_browser_settings") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });
  });

  it("renders the browser automation tab and loads discovered browsers", async () => {
    render(
      <SettingsModal
        isOpen={true}
        onClose={() => {}}
        selectedModel="meta/llama-3.3-70b-instruct"
        onSelectModel={() => {}}
      />
    );

    const browserTab = screen.getByText("Browser Automation");
    expect(browserTab).toBeInTheDocument();
    fireEvent.click(browserTab);

    expect(await screen.findByText("Browser Automation & Capability Matrix")).toBeInTheDocument();
    expect(screen.getByText("Automatic (System Default)")).toBeInTheDocument();
    expect(screen.getByText("Mozilla Firefox")).toBeInTheDocument();
    expect(screen.getByText("Chromium")).toBeInTheDocument();
    expect(screen.getByText("Apple Safari")).toBeInTheDocument();
    expect(screen.getByText("Safari is not automatable on Linux")).toBeInTheDocument();
  });

  it("saves browser settings when requested", async () => {
    render(
      <SettingsModal
        isOpen={true}
        onClose={() => {}}
        selectedModel="meta/llama-3.3-70b-instruct"
        onSelectModel={() => {}}
      />
    );

    const browserTab = screen.getByText("Browser Automation");
    fireEvent.click(browserTab);

    await screen.findByText("Chromium");
    const chromiumChoice = screen.getByText("Chromium");
    fireEvent.click(chromiumChoice);

    const saveButton = screen.getByText("Save Browser Preferences");
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("save_browser_settings", {
        settings: {
          selected_browser: "chromium",
          headless_default: true,
        },
      });
    });
  });
});
