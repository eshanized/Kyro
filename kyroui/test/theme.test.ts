import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Theme System & Token Consistency", () => {
  const cssPath = path.resolve(__dirname, "../index.css");
  const cssContent = fs.readFileSync(cssPath, "utf-8");

  it("defines official Next.js dark base theme tokens (#0a0a0a and #ededed)", () => {
    expect(cssContent).toMatch(/--background:\s*#0a0a0a;/);
    expect(cssContent).toMatch(/--foreground:\s*#ededed;/);
  });

  it("defines official Next.js light base theme tokens (#ffffff and #171717)", () => {
    expect(cssContent).toMatch(/--background:\s*#ffffff;/);
    expect(cssContent).toMatch(/--foreground:\s*#171717;/);
  });

  it("defines consistent neutral application surface and message tokens", () => {
    // Understated neutral borders
    expect(cssContent).toMatch(/--border:\s*#262626;/);

    // Elevated message surfaces
    expect(cssContent).toMatch(/--message-user-bg:\s*#1f1f1f;/);
    expect(cssContent).toMatch(/--message-assistant-bg:\s*#121212;/);

    // Primary action high contrast
    expect(cssContent).toMatch(/--primary:\s*#ededed;/);
    expect(cssContent).toMatch(/--primary-foreground:\s*#0a0a0a;/);

    // Muted text
    expect(cssContent).toMatch(/--text-secondary:\s*#a1a1a1;/);

    // Distinct code and inputs
    expect(cssContent).toMatch(/--input-bg:\s*#000000;/);
    expect(cssContent).toMatch(/--code-bg:\s*#000000;/);
  });

  it("loads and uses Geist Sans as primary typography", () => {
    expect(cssContent).toContain('@import "@fontsource/geist-sans";');
    expect(cssContent).toContain('@import "@fontsource/geist-mono";');
    expect(cssContent).toMatch(/font-family:.*Geist Sans/);
  });

  it("configures Geist Sans and Geist Mono in tailwind configuration", () => {
    const tailwindPath = path.resolve(__dirname, "../../tailwind.config.js");
    const tailwindContent = fs.readFileSync(tailwindPath, "utf-8");
    expect(tailwindContent).toContain('"Geist Sans"');
    expect(tailwindContent).toContain('"Geist Mono"');
    expect(tailwindContent).toContain("var(--background)");
    expect(tailwindContent).toContain("var(--foreground)");
  });

  it("verifies no legacy teal/emerald color utilities remain in Kyro components", () => {
    const componentsDir = path.resolve(__dirname, "../components");
    const files = fs.readdirSync(componentsDir).filter((f: string) => f.endsWith(".tsx"));
    for (const file of files) {
      const content = fs.readFileSync(path.join(componentsDir, file), "utf-8");
      // Except terminal ANSI color definitions in TerminalView
      if (file !== "TerminalView.tsx") {
        expect(content).not.toMatch(/\b(bg-emerald-|text-emerald-|border-emerald-|bg-teal-|text-teal-)\b/);
      }
    }
  });
});
