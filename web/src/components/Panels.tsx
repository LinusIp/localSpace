// The panels area: every open view as a tab, the active one shown, the rest
// kept mounted so an iframe keeps its state (v2 §6.1: at most six).

import { Card, IconButton, SidebarIcon, Tabs } from "@localspace/ui";
import { useSession } from "../store";
import { HarnessFrame } from "./HarnessFrame";
import { WidgetView } from "./WidgetView";

export function Panels() {
  const { panels, activePanel, activatePanel, closePanel, details, toggleDetails, environment } = useSession();
  const titleOf = (harness: string) => environment?.harnesses.find((h) => h.id === harness)?.title ?? harness;

  return (
    <Card className="ls-col ls-grow ls-hidden-scroll">
      <Tabs
        tabs={panels.map((p) => ({ id: p.key, label: p.title, title: `${titleOf(p.harness)} · ${p.view}`, closable: true }))}
        active={activePanel}
        onSelect={activatePanel}
        onClose={closePanel}
        trailing={
          <IconButton label={details ? "Hide the details column" : "Show the details column"} quiet on={details} onClick={toggleDetails}>
            <SidebarIcon size={16} />
          </IconButton>
        }
      />
      <div className="ls-panel-body">
        {panels.map((p) => {
          const active = p.key === activePanel;
          return (
            <div key={p.key} className="ls-panel-layer" hidden={!active}>
              {p.kind === "web" && <HarnessFrame panel={p} active={active} />}
              {p.kind === "widgets" && <WidgetView harness={p.harness} view={p.view} />}
              {(p.kind === "egui" || p.kind === "stream" || p.kind === "native") && (
                <div className="ls-empty">
                  <p className="ls-muted">
                    {p.title} is{" "}
                    {p.kind === "egui" ? "an egui surface" : p.kind === "stream" ? "a streamed surface" : "a native surface, reserved for the Tier B runtime"}.
                  </p>
                  <p className="ls-mt-1 ls-small ls-faint" style={{ maxWidth: "28rem" }}>
                    The web shell runs <code>web</code> and <code>widgets</code> views. This one still runs in the egui client until its harness ships a
                    web view.
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
