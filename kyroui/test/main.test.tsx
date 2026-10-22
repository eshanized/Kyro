import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { RootComponent } from "../main";
import * as webviewWindow from "@tauri-apps/api/webviewWindow";

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(),
}));

vi.mock("../App", () => ({
  App: () => <div data-testid="app-component">Main App View</div>,
}));

vi.mock("../hud/HudPanel", () => ({
  HudPanel: () => <div data-testid="hud-component">HUD Floating Widget</div>,
}));

vi.mock("../hud/Snipper", () => ({
  Snipper: () => <div data-testid="snipper-component">Screen Snipper Tool</div>,
}));

describe("RootComponent and window entry routing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders main App when window label is 'main'", () => {
    vi.mocked(webviewWindow.getCurrentWebviewWindow).mockReturnValue({
      label: "main",
    } as any);

    render(<RootComponent />);
    expect(screen.getByTestId("app-component")).toBeInTheDocument();
  });

  it("renders HudPanel when window label is 'hud'", () => {
    vi.mocked(webviewWindow.getCurrentWebviewWindow).mockReturnValue({
      label: "hud",
    } as any);

    render(<RootComponent />);
    expect(screen.getByTestId("hud-component")).toBeInTheDocument();
  });

  it("renders Snipper when window label is 'overlay'", () => {
    vi.mocked(webviewWindow.getCurrentWebviewWindow).mockReturnValue({
      label: "overlay",
    } as any);

    render(<RootComponent />);
    expect(screen.getByTestId("snipper-component")).toBeInTheDocument();
  });

  it("falls back to URL query parameter when getCurrentWebviewWindow throws", () => {
    vi.mocked(webviewWindow.getCurrentWebviewWindow).mockImplementation(() => {
      throw new Error("Not running in Tauri");
    });

    // Set search param to ?window=hud
    delete (window as any).location;
    (window as any).location = new URL("http://localhost:5173/?window=hud");

    render(<RootComponent />);
    expect(screen.getByTestId("hud-component")).toBeInTheDocument();
  });

  it("falls back to default main App when query parameter is missing", () => {
    vi.mocked(webviewWindow.getCurrentWebviewWindow).mockImplementation(() => {
      throw new Error("Not running in Tauri");
    });

    delete (window as any).location;
    (window as any).location = new URL("http://localhost:5173/");

    render(<RootComponent />);
    expect(screen.getByTestId("app-component")).toBeInTheDocument();
  });
});
