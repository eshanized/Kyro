import React from "react";
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ThinkingIndicator } from "../components/ThinkingIndicator";
import { ToolCallCard } from "../components/ToolCallCard";
import { MarkdownRenderer } from "../components/MarkdownRenderer";

describe("ThinkingIndicator Component", () => {
  it("renders honest waiting fallback when reasoning is absent or empty", () => {
    const { container } = render(
      <ThinkingIndicator agentName="Kyro" isStreaming={true} />
    );

    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.getByText("Waiting for model response…")).toBeInTheDocument();

    // Check animated thinking dots
    const dot1 = container.querySelector(".animate-thinking-dot-1");
    const dot2 = container.querySelector(".animate-thinking-dot-2");
    const dot3 = container.querySelector(".animate-thinking-dot-3");
    expect(dot1).toBeInTheDocument();
    expect(dot2).toBeInTheDocument();
    expect(dot3).toBeInTheDocument();
  });

  it("renders custom statusText fallback when provided", () => {
    render(
      <ThinkingIndicator
        agentName="Kyro"
        statusText="Contacting inference engine…"
        isStreaming={true}
      />
    );

    expect(screen.getByText("Contacting inference engine…")).toBeInTheDocument();
  });

  it("renders live model reasoning with agent attribution in header", () => {
    render(
      <ThinkingIndicator
        agentName="Kyro"
        reasoningText="Analyzing user inquiry and decomposing steps."
        isStreaming={true}
      />
    );

    expect(screen.getByText("Kyro · Model reasoning")).toBeInTheDocument();
    expect(
      screen.getByText("Analyzing user inquiry and decomposing steps.")
    ).toBeInTheDocument();
  });

  it("renders delegated teammate agent attribution (@Scout)", () => {
    render(
      <ThinkingIndicator
        agentName="@Scout"
        reasoningText="Inspecting DOM elements for login form..."
        isStreaming={true}
      />
    );

    expect(screen.getByText("@Scout · Model reasoning")).toBeInTheDocument();
    expect(
      screen.getByText("Inspecting DOM elements for login form...")
    ).toBeInTheDocument();
  });

  it("renders reasoning strictly as plain text without executing Markdown or HTML", () => {
    const dangerousPayload = "<img src='x' onerror='alert(1)'> **bold text** `inline_code`";
    const { container } = render(
      <ThinkingIndicator
        agentName="Kyro"
        reasoningText={dangerousPayload}
        isStreaming={true}
      />
    );

    // Should NOT create an img element in the DOM
    expect(container.querySelector("img")).toBeNull();
    // Should NOT create a <strong> element for markdown
    expect(container.querySelector("strong")).toBeNull();
    // Raw plain text should be preserved verbatim
    const pre = container.querySelector("pre");
    expect(pre).toBeInTheDocument();
    expect(pre?.textContent).toBe(dangerousPayload);
    expect(pre?.className).toContain("font-mono");
    expect(pre?.className).toContain("text-[12px]");
  });

  it("toggles collapse and expand when header is clicked or activated by keyboard", () => {
    const reasoningText = "Step 1: Check credentials.\nStep 2: Proceed.";
    const { container } = render(
      <ThinkingIndicator
        agentName="Kyro"
        reasoningText={reasoningText}
        isStreaming={true}
      />
    );

    let pre = container.querySelector("pre");
    expect(pre).toBeInTheDocument();
    expect(pre?.textContent).toBe(reasoningText);
    expect(screen.getByText("Hide")).toBeInTheDocument();

    const header = screen.getByRole("button");
    fireEvent.click(header);

    expect(container.querySelector("pre")).toBeNull();
    expect(screen.getByText("Show")).toBeInTheDocument();

    // Re-expand with Enter key
    fireEvent.keyDown(header, { key: "Enter" });
    pre = container.querySelector("pre");
    expect(pre).toBeInTheDocument();
    expect(pre?.textContent).toBe(reasoningText);
    expect(screen.getByText("Hide")).toBeInTheDocument();
  });

  it("stops active thinking animation when stream is completed", () => {
    const { container } = render(
      <ThinkingIndicator
        agentName="Kyro"
        reasoningText="Completed all thoughts."
        isStreaming={false}
      />
    );

    // Active wave dots should not be present in completed state
    expect(container.querySelector(".animate-thinking-dot-1")).toBeNull();
    // The panel glow should not be applied when isStreaming is false
    expect(container.querySelector(".thinking-panel-glow")).toBeNull();
  });
});

describe("Code Typography (Fira Code at 12px)", () => {
  it("uses Fira Code at 12px in ToolCallCard arguments and output", () => {
    const toolCall = {
      id: "tc_1",
      name: "execute_shell",
      arguments: { command: "ls -la /workspace" },
    };
    const toolResult = {
      tool_call_id: "tc_1",
      name: "execute_shell",
      output: "total 0\ndrwxr-xr-x 2 user group 64 Oct 24 12:00 src",
      is_error: false,
    };

    const { container } = render(
      <ToolCallCard toolCall={toolCall} result={toolResult} />
    );

    const pres = container.querySelectorAll("pre");
    expect(pres.length).toBeGreaterThanOrEqual(2);

    // Verify both Arguments and Output pre tags have font-mono and text-[12px]
    pres.forEach((pre) => {
      expect(pre.className).toContain("font-mono");
      expect(pre.className).toContain("text-[12px]");
    });
  });

  it("uses Fira Code at 12px in MarkdownRenderer code blocks", () => {
    const markdown = "```rust\nfn main() { println!(\"hello\"); }\n```";
    const { container } = render(<MarkdownRenderer content={markdown} />);

    const code = container.querySelector("pre code");
    expect(code).toBeInTheDocument();
    const style = code?.getAttribute("style") || "";
    expect(style).toContain("var(--font-code");
    expect(style).toContain("Fira Code");
    expect(style).toContain("12px");
  });
});
