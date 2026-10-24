import React, { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { vscDarkPlus } from "react-syntax-highlighter/dist/esm/styles/prism";

export interface MarkdownRendererProps {
  content: string;
  className?: string;
}

export const CodeBlock: React.FC<{ code: string; language?: string }> = ({ code, language }) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(code);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }
    } catch {
      // ignore
    }
  };

  const displayLang = (language || "text").toUpperCase();

  return (
    <div
      data-testid="fenced-code-block"
      className="my-3 rounded-lg border border-neutral-800 bg-[#000000] overflow-hidden text-xs font-mono shadow-sm"
    >
      <div className="flex items-center justify-between px-3.5 py-1.5 bg-[#141414] border-b border-neutral-800 text-[11px] text-neutral-400 select-none">
        <span className="font-semibold text-neutral-300" data-testid="code-block-language">
          {displayLang}
        </span>
        <button
          onClick={handleCopy}
          aria-label="Copy code to clipboard"
          className="flex items-center gap-1 text-[11px] text-neutral-400 hover:text-neutral-100 transition-colors focus:outline-none focus:ring-1 focus:ring-neutral-500 rounded px-1.5 py-0.5"
          title="Copy code"
        >
          {copied ? (
            <>
              <Check className="w-3.5 h-3.5 text-neutral-200" />
              <span>Copied!</span>
            </>
          ) : (
            <>
              <Copy className="w-3.5 h-3.5" />
              <span>Copy</span>
            </>
          )}
        </button>
      </div>
      <div className="overflow-x-auto p-3 text-xs leading-relaxed">
        <SyntaxHighlighter
          language={language || "text"}
          style={vscDarkPlus}
          customStyle={{
            margin: 0,
            padding: 0,
            background: "transparent",
            fontSize: "12px",
            lineHeight: "1.6",
          }}
          codeTagProps={{
            style: {
              fontFamily: 'var(--font-code, "Fira Code", monospace)',
              fontSize: "12px",
            },
          }}
        >
          {code}
        </SyntaxHighlighter>
      </div>
    </div>
  );
};

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({ content, className }) => {
  const safeContent = typeof content === "string" ? content : (content ? JSON.stringify(content) : "");
  return (
    <div className={`markdown-content text-sm leading-relaxed max-w-full min-w-0 ${className || ""}`}>
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          pre({ children }) {
            let code = "";
            let language = "text";

            if (React.isValidElement(children)) {
              const childProps = children.props as any;
              const className = childProps?.className || "";
              const match = /language-(\w+)/.exec(className);
              if (match) {
                language = match[1];
              }
              const childContent = childProps?.children;
              if (Array.isArray(childContent)) {
                code = childContent.join("");
              } else {
                code = String(childContent || "");
              }
              code = code.replace(/\n$/, "");
            } else {
              code = String(children || "").replace(/\n$/, "");
            }

            return <CodeBlock code={code} language={language} />;
          },
          code({ className, children, ...props }) {
            return (
              <code
                data-testid="inline-code"
                className="px-1.5 py-0.5 rounded text-[12px] font-mono bg-[#1c1c1c] text-neutral-200 border border-neutral-800 break-words"
                {...props}
              >
                {children}
              </code>
            );
          },
          h1({ children, ...props }) {
            return (
              <h1 className="text-xl font-bold tracking-tight text-neutral-100 mt-4 mb-2 first:mt-0" {...props}>
                {children}
              </h1>
            );
          },
          h2({ children, ...props }) {
            return (
              <h2 className="text-lg font-semibold tracking-tight text-neutral-100 mt-3.5 mb-2 first:mt-0" {...props}>
                {children}
              </h2>
            );
          },
          h3({ children, ...props }) {
            return (
              <h3 className="text-base font-semibold text-neutral-200 mt-3 mb-1.5 first:mt-0" {...props}>
                {children}
              </h3>
            );
          },
          h4({ children, ...props }) {
            return (
              <h4 className="text-sm font-semibold text-neutral-200 mt-2.5 mb-1 first:mt-0" {...props}>
                {children}
              </h4>
            );
          },
          h5({ children, ...props }) {
            return (
              <h5 className="text-xs font-semibold text-neutral-300 mt-2 mb-1 first:mt-0" {...props}>
                {children}
              </h5>
            );
          },
          h6({ children, ...props }) {
            return (
              <h6 className="text-xs font-medium text-neutral-400 mt-1.5 mb-1 first:mt-0" {...props}>
                {children}
              </h6>
            );
          },
          strong({ children, ...props }) {
            return (
              <strong className="font-semibold text-neutral-100" {...props}>
                {children}
              </strong>
            );
          },
          em({ children, ...props }) {
            return (
              <em className="italic text-neutral-200" {...props}>
                {children}
              </em>
            );
          },
          del({ children, ...props }) {
            return (
              <del className="line-through text-neutral-400" {...props}>
                {children}
              </del>
            );
          },
          p({ children, ...props }) {
            return (
              <p
                className="leading-relaxed text-sm text-neutral-200 my-2 first:mt-0 last:mb-0 break-words [overflow-wrap:anywhere]"
                {...props}
              >
                {children}
              </p>
            );
          },
          blockquote({ children, ...props }) {
            return (
              <blockquote className="border-l-2 border-neutral-700 pl-3.5 my-2.5 text-neutral-400 italic" {...props}>
                {children}
              </blockquote>
            );
          },
          hr({ ...props }) {
            return <hr className="my-4 border-neutral-800" {...props} />;
          },
          ul({ children, ...props }) {
            return (
              <ul className="list-disc pl-5 my-2 space-y-1 text-sm text-neutral-200 [&_ul]:my-1 [&_ol]:my-1" {...props}>
                {children}
              </ul>
            );
          },
          ol({ children, ...props }) {
            return (
              <ol className="list-decimal pl-5 my-2 space-y-1 text-sm text-neutral-200 [&_ul]:my-1 [&_ol]:my-1" {...props}>
                {children}
              </ol>
            );
          },
          li({ children, ...props }) {
            return (
              <li className="leading-relaxed" {...props}>
                {children}
              </li>
            );
          },
          table({ children, ...props }) {
            return (
              <div className="my-3 overflow-x-auto rounded-lg border border-neutral-800 max-w-full" data-testid="table-container">
                <table className="min-w-full divide-y divide-neutral-800 text-left text-xs font-mono" {...props}>
                  {children}
                </table>
              </div>
            );
          },
          thead({ children, ...props }) {
            return (
              <thead className="bg-[#141414] text-neutral-200 uppercase tracking-wider text-[11px]" {...props}>
                {children}
              </thead>
            );
          },
          th({ children, ...props }) {
            return (
              <th className="px-3.5 py-2.5 font-semibold text-neutral-200 border-b border-neutral-800 whitespace-nowrap" {...props}>
                {children}
              </th>
            );
          },
          tbody({ children, ...props }) {
            return (
              <tbody className="divide-y divide-neutral-800/60 bg-[#0a0a0a]" {...props}>
                {children}
              </tbody>
            );
          },
          tr({ children, ...props }) {
            return (
              <tr className="hover:bg-neutral-800/30 transition-colors" {...props}>
                {children}
              </tr>
            );
          },
          td({ children, ...props }) {
            return (
              <td className="px-3.5 py-2 text-neutral-300" {...props}>
                {children}
              </td>
            );
          },
          a({ href, children, ...props }) {
            const isSafe = href && !href.trim().toLowerCase().startsWith("javascript:");
            return (
              <a
                href={isSafe ? href : "#"}
                target="_blank"
                rel="noopener noreferrer"
                className="text-neutral-200 underline underline-offset-4 decoration-neutral-500 hover:decoration-neutral-200 hover:text-white transition-colors break-all"
                {...props}
              >
                {children}
              </a>
            );
          },
          input({ type, checked, disabled, ...props }) {
            if (type === "checkbox") {
              return (
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={true}
                  className="mr-2 h-3.5 w-3.5 rounded border-neutral-700 bg-neutral-900 text-neutral-200 accent-neutral-300 align-middle"
                  {...props}
                />
              );
            }
            return <input type={type} disabled={disabled} {...props} />;
          },
        }}
      >
        {safeContent}
      </Markdown>
    </div>
  );
};
