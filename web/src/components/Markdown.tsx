// Assistant messages are Markdown. Code blocks get a copy button; nothing
// else is special. `react-markdown` renders to React elements, never HTML
// strings, so model output cannot inject markup into the shell.

import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";

export function Markdown({ text }: { text: string }) {
  return (
    <div className="prose-chat">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer" className="text-accent underline">
              {children}
            </a>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

function CodeBlock({ children }: { children: React.ReactNode }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    const text = textOf(children);
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <div className="relative my-3 rounded-lg bg-page">
      <button
        onClick={copy}
        className="absolute right-2 top-2 rounded-md p-1 text-faint hover:bg-white hover:text-ink"
        title="Copy"
      >
        {copied ? <Check size={14} /> : <Copy size={14} />}
      </button>
      <pre className="overflow-x-auto p-4 font-mono text-[12.5px] leading-relaxed">{children}</pre>
    </div>
  );
}

function textOf(node: React.ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (typeof node === "object" && "props" in node) {
    const props = (node as { props: { children?: React.ReactNode } }).props;
    return textOf(props.children);
  }
  return "";
}
