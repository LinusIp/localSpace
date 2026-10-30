// The screens' states, one way with `themes` off and another with it on:
// off, the shell's own pills, error lines and bars, exactly as before; on,
// the package's Badge, Notice and Progress. One place to drop the old half
// when the switch goes.

import type { ReactNode } from "react";
import { Badge, Notice, Progress } from "@localspace/ui";
import { featureOn } from "../lib/features";

type StateTone = "neutral" | "ok" | "warn" | "danger";

const OLD_PILL: Record<StateTone, string> = { neutral: "pill", ok: "pill green", warn: "pill amber", danger: "pill red" };

/** A short state beside a name: "In use", "Installed", "Disabled". */
export function StatePill({ tone = "neutral", children }: { tone?: StateTone; children: ReactNode }) {
  if (featureOn("themes")) return <Badge tone={tone}>{children}</Badge>;
  return <span className={OLD_PILL[tone]}>{children}</span>;
}

/** Something that went wrong, said where it happened. */
export function ErrorLine({ children }: { children: ReactNode }) {
  if (featureOn("themes")) {
    return (
      <div style={{ marginTop: 12 }}>
        <Notice tone="danger">{children}</Notice>
      </div>
    );
  }
  return <div className="error">{children}</div>;
}

/** A download's bar and the words under it. */
export function DownloadBar({ percent, label, labelClass }: { percent: number; label: string; labelClass: string }) {
  if (featureOn("themes")) return <Progress value={percent / 100} label={label} />;
  return (
    <>
      <div className="progress">
        <div style={{ width: `${percent}%` }} />
      </div>
      <div className={labelClass}>{label}</div>
    </>
  );
}
