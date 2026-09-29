import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MarkdownRenderer } from "../components/MarkdownRenderer";

describe("MarkdownRenderer Component", () => {
  // 1. A heading and bold text render as semantic elements
  it("renders headings from H1 through H6 and bold text as semantic elements", () => {
    const markdown = `
# Heading 1
## Heading 2
### Open Application Windows
#### Heading 4
##### Heading 5
###### Heading 6
**Open Application Windows**
*Italic text*
~Strikethrough~
`;
    render(<MarkdownRenderer content={markdown} />);

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Heading 1");
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Heading 2");
    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent("Open Application Windows");
    expect(screen.getByRole("heading", { level: 4 })).toHaveTextContent("Heading 4");
    expect(screen.getByRole("heading", { level: 5 })).toHaveTextContent("Heading 5");
    expect(screen.getByRole("heading", { level: 6 })).toHaveTextContent("Heading 6");

    const boldEl = screen.getByText("Open Application Windows", { selector: "strong" });
    expect(boldEl).toBeInTheDocument();
    expect(boldEl.tagName).toBe("STRONG");

    const italicEl = screen.getByText("Italic text");
    expect(italicEl.tagName).toBe("EM");

    const strikeEl = screen.getByText("Strikethrough");
    expect(strikeEl.tagName).toBe("DEL");
  });

  // 2. A valid GFM table renders as a table with headers and rows
  it("renders a valid GFM table with headers and rows (reproduction screenshot case)", () => {
    const tableMarkdown = `
| Window ID | Title | Application | State |
| :--- | :--- | :--- | :--- |
| 1042 | Terminal - Kyro | Ghost Terminal | Focused |
| 2088 | Google Chrome | Web Browser | Active |
`;
    render(<MarkdownRenderer content={tableMarkdown} />);

    const table = screen.getByRole("table");
    expect(table).toBeInTheDocument();

    const columnHeaders = screen.getAllByRole("columnheader");
    expect(columnHeaders).toHaveLength(4);
    expect(columnHeaders[0]).toHaveTextContent("Window ID");
    expect(columnHeaders[1]).toHaveTextContent("Title");
    expect(columnHeaders[2]).toHaveTextContent("Application");
    expect(columnHeaders[3]).toHaveTextContent("State");

    const cells = screen.getAllByRole("cell");
    expect(cells.length).toBe(8);
    expect(cells[0]).toHaveTextContent("1042");
    expect(cells[1]).toHaveTextContent("Terminal - Kyro");
    expect(cells[2]).toHaveTextContent("Ghost Terminal");
    expect(cells[3]).toHaveTextContent("Focused");
  });

  // 3. Lists, blockquotes and horizontal rules render correctly
  it("renders ordered, unordered, task lists, blockquotes and horizontal rules correctly", () => {
    const listMarkdown = `
* First bullet
* Second bullet
  * Nested bullet

1. Numbered item 1
2. Numbered item 2

* [ ] Pending task item
* [x] Completed task item

> This is a notable quote block

---
`;
    const { container } = render(<MarkdownRenderer content={listMarkdown} />);

    expect(screen.getByText("First bullet")).toBeInTheDocument();
    expect(screen.getByText("Nested bullet")).toBeInTheDocument();
    expect(screen.getByText("Numbered item 1")).toBeInTheDocument();

    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes.length).toBe(2);
    expect(checkboxes[0]).not.toBeChecked();
    expect(checkboxes[1]).toBeChecked();

    const quote = container.querySelector("blockquote");
    expect(quote).toBeInTheDocument();
    expect(quote).toHaveTextContent("This is a notable quote block");

    const hr = container.querySelector("hr");
    expect(hr).toBeInTheDocument();
  });

  // 4. Inline code and fenced code blocks are distinguishable
  it("distinguishes inline code from fenced code blocks", () => {
    const codeMarkdown = `
Use \`cargo test\` to run checks.

\`\`\`bash
cargo run --manifest-path libkyro/Cargo.toml
\`\`\`
`;
    render(<MarkdownRenderer content={codeMarkdown} />);

    const inlineCode = screen.getByTestId("inline-code");
    expect(inlineCode).toHaveTextContent("cargo test");
    expect(inlineCode.tagName).toBe("CODE");

    const fencedBlock = screen.getByTestId("fenced-code-block");
    expect(fencedBlock).toBeInTheDocument();

    const langBadge = screen.getByTestId("code-block-language");
    expect(langBadge).toHaveTextContent("BASH");
    expect(fencedBlock).toHaveTextContent("cargo run --manifest-path libkyro/Cargo.toml");
  });

  // 5. Plain-text responses still look correct
  it("renders plain-text responses without code-style pre wrapper", () => {
    const plainText = "Hello! I am Kyro, your local-first autonomous desktop agent.";
    const { container } = render(<MarkdownRenderer content={plainText} />);

    const paragraph = container.querySelector("p");
    expect(paragraph).toBeInTheDocument();
    expect(paragraph).toHaveTextContent(plainText);
    expect(container.querySelector("pre")).toBeNull();
  });

  // 6. Streaming and completed messages render without exposing raw Markdown after completion
  it("handles incomplete streaming Markdown chunks gracefully without crashing and renders final Markdown", () => {
    // Incomplete stream chunk 1: partial heading
    const { rerender } = render(<MarkdownRenderer content="### Open App" />);
    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent("Open App");

    // Incomplete stream chunk 2: unclosed table
    rerender(<MarkdownRenderer content={"| App | PID |\n| --- | --- |\n| Kyro |"} />);
    expect(screen.getByRole("table")).toBeInTheDocument();

    // Incomplete stream chunk 3: unclosed code block
    rerender(<MarkdownRenderer content={"```rust\nfn main() {"} />);
    expect(screen.getByTestId("fenced-code-block")).toBeInTheDocument();

    // Final completed message
    const completedMarkdown = `
### Open Application Windows

| Window ID | Title |
| --- | --- |
| 1 | Main Kyro Window |

Task completed successfully.
`;
    rerender(<MarkdownRenderer content={completedMarkdown} />);

    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent("Open Application Windows");
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("Main Kyro Window")).toBeInTheDocument();
    expect(screen.getByText("Task completed successfully.")).toBeInTheDocument();
  });

  // 7. Potentially unsafe HTML is not executed
  it("sanitizes potentially unsafe raw HTML tags and javascript: links", () => {
    const malicious = `
<script>window.__xss_test = true;</script>

<img src="x" onerror="window.__xss_test = true;" />

[Unsafe Link](javascript:alert(1))
`;
    const { container } = render(<MarkdownRenderer content={malicious} />);

    expect(container.querySelector("script")).toBeNull();
    expect((window as any).__xss_test).toBeUndefined();

    const link = screen.getByRole("link", { name: "Unsafe Link" });
    expect(link).toHaveAttribute("href", "#");
  });

  // 8. Long tables and code blocks do not break the chat layout
  it("wraps tables and code blocks in horizontally scrollable containers", () => {
    const wideMarkdown = `
| Very Long Header 1 | Very Long Header 2 | Very Long Header 3 | Very Long Header 4 | Very Long Header 5 |
| --- | --- | --- | --- | --- |
| Long cell content that could overflow standard widths | Another very long cell content string | Data cell 3 | Data cell 4 | Data cell 5 |

\`\`\`typescript
const veryLongLineOfCodeThatExceedsNormalViewportWidthsAndNeedsHorizontalScrolling = "This is a long unbroken string of text that should scroll cleanly";
\`\`\`
`;
    const { container } = render(<MarkdownRenderer content={wideMarkdown} />);

    const tableContainer = screen.getByTestId("table-container");
    expect(tableContainer).toHaveClass("overflow-x-auto");

    const codeBlock = screen.getByTestId("fenced-code-block");
    const codeScroll = codeBlock.querySelector(".overflow-x-auto");
    expect(codeScroll).toBeInTheDocument();

    const rootWrapper = container.firstElementChild;
    expect(rootWrapper).toHaveClass("max-w-full");
    expect(rootWrapper).toHaveClass("min-w-0");
  });
});
