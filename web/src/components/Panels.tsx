// The panels area: every open view as a tab, the active one shown, the rest
// kept mounted so an iframe keeps its state (v2 §6.1: at most six).

import { PanelRightOpen, X } from "lucide-react";
import { useSession } from "../store";
import { Card } from "./ui";
import { HarnessFrame } from "./HarnessFrame";
import { WidgetView } from "./WidgetView";

export function Panels() {
  const { panels, activePanel, activatePanel, closePanel, details, toggleDetails, environment } = useSession();
  const titleOf = (harness: string) => environment?.harnesses.find((h) => h.id === harness)?.title ?? harness;

  return (
    <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-1 border-b border-line px-2 py-1.5">
        {panels.map((p) => {
          const active = p.key === activePanel;
          return (
            <div
              key={p.key}
              className={`group flex items-center gap-1 rounded-lg px-2 py-1 text-sm ${
                active ? "bg-accent-soft text-accent" : "text-muted hover:bg-page"
              }`}
            >
              <button onClick={() => activatePanel(p.key)} title={`${titleOf(p.harness)} · ${p.view}`}>
                {p.title}
              </button>
              <button
                className="rounded p-0.5 opacity-60 hover:opacity-100"
                onClick={() => closePanel(p.key)}
                aria-label={`Close ${p.title}`}
              >
                <X size={13} />
              </button>
            </div>
          );
        })}
        <button
          className={`ml-auto rounded-lg p-1.5 ${details ? "text-accent" : "text-muted hover:text-ink"}`}
          onClick={toggleDetails}
          title={details ? "Hide the details column" : "Show the details column"}
          aria-label="Details"
        >
          <PanelRightOpen size={16} />
        </button>
      </div>
      <div className="relative min-h-0 flex-1">
        {panels.map((p) => {
          const active = p.key === activePanel;
          return (
            <div key={p.key} className="absolute inset-0" hidden={!active}>
              {p.kind === "web" && <HarnessFrame panel={p} active={active} />}
              {p.kind === "widgets" && <WidgetView harness={p.harness} view={p.view} />}
              {(p.kind === "egui" || p.kind === "stream" || p.kind === "native") && (
                <div className="flex h-full flex-col items-center justify-center p-8 text-center">
                  <p className="text-sm text-muted">
                    {p.title} is {p.kind === "egui" ? "an egui surface" : p.kind === "stream" ? "a streamed surface" : "a native surface, reserved for the Tier B runtime"}.
                  </p>
                  <p className="mt-1 max-w-md text-xs text-faint">
                    The web shell runs <code>web</code> and <code>widgets</code> views. This one still runs in the
                    egui client until its harness ships a web view.
                  </p>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </Card>
  );
}
