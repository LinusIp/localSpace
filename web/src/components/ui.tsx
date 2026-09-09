// Small shared pieces of the shell: cards, section headings, pills, buttons.

import type { ReactNode } from "react";

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div className={`rounded-xl border border-line bg-white ${className}`}>{children}</div>
  );
}

export function SectionTitle({
  children,
  action,
  onAction,
}: {
  children: ReactNode;
  action?: string;
  onAction?: () => void;
}) {
  return (
    <div className="mb-2 flex items-baseline justify-between">
      <h2 className="text-[15px] font-semibold text-ink">{children}</h2>
      {action && (
        <button className="text-xs text-muted hover:text-ink" onClick={onAction}>
          {action}
        </button>
      )}
    </div>
  );
}

export function Pill({
  tone = "neutral",
  children,
}: {
  tone?: "neutral" | "ok" | "warn" | "danger";
  children: ReactNode;
}) {
  const tones = {
    neutral: "bg-page text-muted border-line",
    ok: "bg-accent-soft text-accent border-accent/20",
    warn: "bg-warn-soft text-warn border-warn/20",
    danger: "bg-danger-soft text-danger border-danger/20",
  } as const;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium ${tones[tone]}`}
    >
      {children}
    </span>
  );
}

export function Dot({ tone }: { tone: "ok" | "warn" | "danger" | "neutral" }) {
  const color = {
    ok: "bg-accent",
    warn: "bg-warn",
    danger: "bg-danger",
    neutral: "bg-faint",
  }[tone];
  return <span className={`inline-block h-2 w-2 rounded-full ${color}`} />;
}

export function Button({
  children,
  onClick,
  kind = "ghost",
  disabled,
  title,
  type = "button",
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  kind?: "ghost" | "primary" | "danger";
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
  className?: string;
}) {
  const kinds = {
    ghost: "border border-line bg-white text-ink hover:bg-page",
    primary: "bg-accent text-white hover:bg-accent-strong",
    danger: "border border-danger/30 bg-white text-danger hover:bg-danger-soft",
  } as const;
  return (
    <button
      type={type}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={`rounded-lg px-3 py-1.5 text-sm font-medium disabled:cursor-not-allowed disabled:opacity-50 ${kinds[kind]} ${className}`}
    >
      {children}
    </button>
  );
}

export function Empty({ title, body }: { title: string; body?: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center p-8 text-center">
      <p className="text-sm text-muted">{title}</p>
      {body && <p className="mt-1 max-w-md text-xs text-faint">{body}</p>}
    </div>
  );
}

export function KeyValue({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted">{k}</dt>
          <dd className="min-w-0 break-words">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} days ago`;
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
