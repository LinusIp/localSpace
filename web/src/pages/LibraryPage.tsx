// Library: what is installed, and the catalog it came from. Installing is
// Core's decision — capabilities that widen are shown and asked about.

import { useEffect } from "react";
import { Button, Card, DownloadIcon, IconButton, Pill, SectionTitle, Switch, TrashIcon } from "@localspace/ui";
import { useSession } from "../store";

export function LibraryPage() {
  const { environment, catalog, refreshCatalog, install, uninstall, setEnabled } = useSession();

  useEffect(() => {
    void refreshCatalog();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const installed = environment?.harnesses ?? [];
  const available = catalog.filter((e) => !e.installed);

  return (
    <div className="page page-grid-2">
      <Card className="ls-pad-4">
        <SectionTitle>Installed</SectionTitle>
        {installed.length === 0 ? (
          <p className="ls-muted">Only chat ships in the box. Everything else comes from the catalog.</p>
        ) : (
          <ul className="ls-list ls-divided">
            {installed.map((h) => (
              <li key={h.id} style={{ padding: "12px 0" }}>
                <div className="ls-row ls-gap-3">
                  <div className="ls-grow">
                    <div className="ls-row ls-gap-2 ls-wrap">
                      <span className="ls-medium">{h.title}</span>
                      <Pill tone="neutral">{h.tier === "wasm" ? "Tier A · wasm" : "Tier B · native"}</Pill>
                      {h.degraded && <Pill tone="warn">{h.degraded}</Pill>}
                    </div>
                    <div className="ls-small ls-muted">
                      {h.id} · {h.version} · {h.publisher}
                    </div>
                    <div className="ls-small ls-muted">
                      {h.tool_count} tools · {h.front_door.length} front door
                      {h.has_context_provider ? " · context provider" : ""} · logic {h.resources.logic_mb} MB, surface {h.resources.surface_mb} MB
                      {h.accepts.length > 0 ? ` · accepts ${h.accepts.join(", ")}` : ""}
                      {h.produces.length > 0 ? ` · produces ${h.produces.join(", ")}` : ""}
                    </div>
                  </div>
                  <Switch checked={h.enabled} label={`${h.title} enabled`} onChange={(on) => void setEnabled(h.id, on)} />
                  <IconButton label="Uninstall" onClick={() => void uninstall(h.id)} className="ls-danger">
                    <TrashIcon size={14} />
                  </IconButton>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card className="ls-pad-4">
        <SectionTitle action="Refresh" onAction={() => void refreshCatalog()}>
          Catalog
        </SectionTitle>
        {available.length === 0 ? (
          <p className="ls-muted">
            {catalog.length === 0 ? "No catalog is configured. Start the server with --registry <dir>." : "Everything in the catalog is installed."}
          </p>
        ) : (
          <ul className="ls-list ls-divided">
            {available.map((e) => (
              <li key={e.id} style={{ padding: "12px 0" }}>
                <div className="ls-row ls-start ls-gap-3">
                  <div className="ls-grow">
                    <div className="ls-row ls-gap-2 ls-wrap">
                      <span className="ls-medium">{e.title}</span>
                      <Pill tone="neutral">{e.kind}</Pill>
                      <Pill tone="neutral">{e.tier === "wasm" ? "Tier A" : "Tier B"}</Pill>
                    </div>
                    <div className="ls-small ls-muted">
                      {e.id} · {e.version} · {e.publisher} · {e.source}
                    </div>
                    <p className="ls-mt-1" style={{ marginBottom: 0 }}>
                      {e.description}
                    </p>
                    <ul className="ls-list ls-mt-1 ls-small ls-muted">
                      {e.capability_lines.map((line) => (
                        <li key={line}>{line}</li>
                      ))}
                    </ul>
                    {e.dependencies.length > 0 && <div className="ls-mt-1 ls-small ls-muted">depends on {e.dependencies.join(", ")}</div>}
                    {e.native_reason && <div className="ls-mt-1 ls-small ls-warn">native: {e.native_reason}</div>}
                    {e.blocked && <div className="ls-mt-1 ls-small ls-danger">{e.blocked}</div>}
                    {e.widens.length > 0 && <div className="ls-mt-1 ls-small ls-warn">would widen: {e.widens.join(", ")}</div>}
                  </div>
                  <Button kind="primary" disabled={!!e.blocked} onClick={() => void install(e.path)}>
                    <DownloadIcon size={14} /> Install
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
