// Badges, notices and progress: how a screen says what state something is
// in (docs/design/0.2.1-THEMES-AND-TRANSLATION.md §3). A notice is one
// sentence where the problem is, with at most one action and the technical
// text folded under "Details"; toasts stay for confirmations.

import type { ReactNode } from "react";
import { cx, type Tone } from "./controls.tsx";

export type BadgeTone = Tone | "accent" | "highlight";

const badgeTone: Record<BadgeTone, string> = {
  neutral: "",
  accent: "ls-accent-pill",
  highlight: "ls-highlight-pill",
  ok: "ls-ok",
  warn: "ls-warn-pill",
  danger: "ls-danger-pill",
};

/** A short label for a state: "In use", "Installed", "Recommended". */
export function Badge({ tone = "neutral", children }: { tone?: BadgeTone; children: ReactNode }) {
  return <span className={cx("ls-pill", badgeTone[tone])}>{children}</span>;
}

/** One sentence about a problem or a state, where it applies. */
export function Notice({ tone = "warn", children, action, details }: { tone?: "info" | "warn" | "danger"; children: ReactNode; action?: ReactNode; details?: string }) {
  return (
    <div className={cx("ls-notice", tone === "warn" && "ls-warn-notice", tone === "danger" && "ls-danger-notice")} role={tone === "danger" ? "alert" : "status"}>
      <div className="ls-notice-body">
        <div>{children}</div>
        {details && (
          <details className="ls-notice-details">
            <summary>Details</summary>
            <pre>{details}</pre>
          </details>
        )}
      </div>
      {action}
    </div>
  );
}

/** A bar for work under way, always with words saying how far it is. */
export function Progress({ value, label }: { value: number; label: string }) {
  const percent = Math.max(0, Math.min(100, value * 100));
  return (
    <div className="ls-progress">
      <div className="ls-progress-track" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)} aria-label={label}>
        <div style={{ width: `${percent}%` }} />
      </div>
      <div className="ls-progress-label">{label}</div>
    </div>
  );
}
