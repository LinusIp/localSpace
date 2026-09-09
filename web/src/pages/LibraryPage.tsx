// Library: what is installed, and the catalog it came from. Installing is
// Core's decision — capabilities that widen are shown and asked about.

import { useEffect } from "react";
import { Download, Trash2 } from "lucide-react";
import { useSession } from "../store";
import { Button, Card, Pill, SectionTitle } from "../components/ui";
import { Switch } from "../components/Switch";

export function LibraryPage() {
  const { environment, catalog, refreshCatalog, install, uninstall, setEnabled } = useSession();

  useEffect(() => {
    void refreshCatalog();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const installed = environment?.harnesses ?? [];
  const available = catalog.filter((e) => !e.installed);

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <Card className="p-5">
        <SectionTitle>Installed</SectionTitle>
        {installed.length === 0 ? (
          <p className="text-sm text-muted">Only chat ships in the box. Everything else comes from the catalog.</p>
        ) : (
          <ul className="divide-y divide-line">
            {installed.map((h) => (
              <li key={h.id} className="py-3">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{h.title}</span>
                      <Pill tone="neutral">{h.tier === "wasm" ? "Tier A · wasm" : "Tier B · native"}</Pill>
                      {h.degraded && <Pill tone="warn">{h.degraded}</Pill>}
                    </div>
                    <div className="text-xs text-muted">
                      {h.id} · {h.version} · {h.publisher}
                    </div>
                    <div className="text-xs text-muted">
                      {h.tool_count} tools · {h.front_door.length} front door
                      {h.has_context_provider ? " · context provider" : ""} · logic {h.resources.logic_mb} MB, surface{" "}
                      {h.resources.surface_mb} MB
                      {h.accepts.length > 0 ? ` · accepts ${h.accepts.join(", ")}` : ""}
                      {h.produces.length > 0 ? ` · produces ${h.produces.join(", ")}` : ""}
                    </div>
                  </div>
                  <Switch checked={h.enabled} label={`${h.title} enabled`} onChange={(on) => void setEnabled(h.id, on)} />
                  <Button kind="danger" onClick={() => void uninstall(h.id)} title="Uninstall">
                    <Trash2 size={14} />
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card className="p-5">
        <SectionTitle action="Refresh" onAction={() => void refreshCatalog()}>
          Catalog
        </SectionTitle>
        {available.length === 0 ? (
          <p className="text-sm text-muted">
            {catalog.length === 0
              ? "No catalog is configured. Start the server with --registry <dir>."
              : "Everything in the catalog is installed."}
          </p>
        ) : (
          <ul className="divide-y divide-line">
            {available.map((e) => (
              <li key={e.id} className="py-3">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{e.title}</span>
                      <Pill tone="neutral">{e.kind}</Pill>
                      <Pill tone="neutral">{e.tier === "wasm" ? "Tier A" : "Tier B"}</Pill>
                    </div>
                    <div className="text-xs text-muted">
                      {e.id} · {e.version} · {e.publisher} · {e.source}
                    </div>
                    <p className="mt-1 text-sm">{e.description}</p>
                    <ul className="mt-1 text-xs text-muted">
                      {e.capability_lines.map((line) => (
                        <li key={line}>{line}</li>
                      ))}
                    </ul>
                    {e.dependencies.length > 0 && (
                      <div className="mt-1 text-xs text-muted">depends on {e.dependencies.join(", ")}</div>
                    )}
                    {e.native_reason && <div className="mt-1 text-xs text-warn">native: {e.native_reason}</div>}
                    {e.blocked && <div className="mt-1 text-xs text-danger">{e.blocked}</div>}
                    {e.widens.length > 0 && (
                      <div className="mt-1 text-xs text-warn">would widen: {e.widens.join(", ")}</div>
                    )}
                  </div>
                  <Button kind="primary" disabled={!!e.blocked} onClick={() => void install(e.path)}>
                    <span className="flex items-center gap-1">
                      <Download size={14} /> Install
                    </span>
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
