// Assistant messages are Markdown. The parser is remark (infrastructure,
// behind this one module); the rendering is ours: the syntax tree becomes
// React elements, never HTML strings, so model output cannot inject markup
// into the shell. Code blocks get a copy button; nothing else is special.

import { useMemo, useState, type ReactNode } from "react";
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import type { Root, RootContent, PhrasingContent } from "mdast";
import { CheckIcon, CopyIcon } from "@localspace/ui";

const parser = unified().use(remarkParse).use(remarkGfm);

export function parseMarkdown(text: string): Root {
  return parser.parse(text) as Root;
}

export function Markdown({ text }: { text: string }) {
  const tree = useMemo(() => parseMarkdown(text), [text]);
  return <div className="prose-chat">{tree.children.map((node, i) => render(node, i))}</div>;
}

type Node = RootContent | PhrasingContent;

function children(nodes: readonly Node[] | undefined): ReactNode[] {
  return (nodes ?? []).map((n, i) => render(n, i));
}

function render(node: Node, key: number): ReactNode {
  switch (node.type) {
    case "paragraph":
      return <p key={key}>{children(node.children)}</p>;
    case "heading": {
      const inner = children(node.children);
      switch (node.depth) {
        case 1:
          return <h1 key={key}>{inner}</h1>;
        case 2:
          return <h2 key={key}>{inner}</h2>;
        case 3:
          return <h3 key={key}>{inner}</h3>;
        default:
          return <h4 key={key}>{inner}</h4>;
      }
    }
    case "text":
      return node.value;
    case "emphasis":
      return <em key={key}>{children(node.children)}</em>;
    case "strong":
      return <strong key={key}>{children(node.children)}</strong>;
    case "delete":
      return <del key={key}>{children(node.children)}</del>;
    case "inlineCode":
      return <code key={key}>{node.value}</code>;
    case "code":
      return <CodeBlock key={key} code={node.value} language={node.lang ?? undefined} />;
    case "link":
      return (
        <a key={key} href={safeHref(node.url)} target="_blank" rel="noreferrer">
          {children(node.children)}
        </a>
      );
    case "image":
      // No remote images: the text says what it was.
      return <span key={key} className="ls-muted">[image: {node.alt || node.url}]</span>;
    case "list":
      return node.ordered ? (
        <ol key={key} start={node.start ?? undefined}>{children(node.children)}</ol>
      ) : (
        <ul key={key}>{children(node.children)}</ul>
      );
    case "listItem":
      return (
        <li key={key}>
          {node.checked !== null && node.checked !== undefined && <input type="checkbox" checked={node.checked} readOnly />}{" "}
          {children(node.children)}
        </li>
      );
    case "blockquote":
      return <blockquote key={key}>{children(node.children)}</blockquote>;
    case "thematicBreak":
      return <hr key={key} />;
    case "break":
      return <br key={key} />;
    case "table": {
      const [head, ...rows] = node.children;
      return (
        <table key={key}>
          {head && (
            <thead>
              <tr>
                {head.children.map((cell, i) => (
                  <th key={i}>{children(cell.children)}</th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {rows.map((row, r) => (
              <tr key={r}>
                {row.children.map((cell, c) => (
                  <td key={c}>{children(cell.children)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
    case "html":
      // Raw HTML is shown as the text it is, never interpreted.
      return <span key={key}>{node.value}</span>;
    case "footnoteReference":
      return <sup key={key}>[{node.identifier}]</sup>;
    case "footnoteDefinition":
      return (
        <div key={key} className="ls-small ls-muted">
          [{node.identifier}] {children(node.children)}
        </div>
      );
    default:
      return null;
  }
}

/** Only web and mail links open; anything else is shown but goes nowhere. */
function safeHref(url: string): string | undefined {
  return /^(https?:|mailto:)/i.test(url) ? url : undefined;
}

function CodeBlock({ code, language }: { code: string; language?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <div className="codeblock">
      <button type="button" className="copy" onClick={copy} title="Copy" aria-label="Copy the code">
        {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
      </button>
      <pre>
        <code data-language={language}>{code}</code>
      </pre>
    </div>
  );
}
