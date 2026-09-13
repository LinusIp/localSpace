// The Store (screen 5, Directive 3): what can be added to the workspace,
// each as an icon, a name, one sentence and one button. What it is made of
// sits behind Details. Everything listed is in the catalog this server was
// given; nothing here is a promise.

import { useEffect, useState } from "react";
import { BoardIcon, Dialog } from "@localspace/ui";
import type { CatalogEntry } from "../api/generated";
import { useSession } from "../store";
import type { InstallPrompt } from "../store";
import { TopBar } from "../components/TopBar";

/** The first sentence of a description, for the tile. */
function firstSentence(text: string): string {
  const trimmed = text.trim();
  const end = trimmed.search(/[.!?](\s|$)/);
  return end > 0 ? trimmed.slice(0, end + 1) : trimmed;
}

export function StorePage() {
  const { me, catalog, environment, refreshCatalog, install, approveInstall, uninstall, openBoard } = useSession();
  const [prompt, setPrompt] = useState<InstallPrompt | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    void refreshCatalog();
  }, [refreshCatalog]);

  // Libraries a tool depends on install with it; the Store shows the tools.
  const entries = catalog.filter((e) => e.kind === "harness");
  const where = me?.topology === "organisation" ? "your own server" : "this computer";

  const add = async (entry: CatalogEntry) => {
    setBusy(entry.id);
    const asked = await install(entry.path);
    setBusy(null);
    if (asked) setPrompt(asked);
  };

  return (
    <>
      <TopBar />
      <div className="page">
        <h1 className="page-title">Store</h1>
        <div className="page-sub">Add tools to your workspace. Everything installs onto {where}.</div>
        {entries.length === 0 ? (
          <p className="ls-muted" style={{ marginTop: 32 }}>
            {catalog.length === 0 ? "This server has no catalog of tools yet. Your administrator adds one." : "Nothing in the catalog is a tool to add."}
          </p>
        ) : (
          <div className="tiles">
            {entries.map((e) => (
              <Tile
                key={e.id}
                entry={e}
                installed={environment?.harnesses.some((h) => h.id === e.id) ?? e.installed}
                busy={busy === e.id}
                onInstall={() => void add(e)}
                onOpen={() => openBoard(e.id)}
                onRemove={() => void uninstall(e.id)}
              />
            ))}
          </div>
        )}
      </div>
      <Dialog
        open={prompt !== null}
        title={prompt ? `${entries.find((e) => e.id === prompt.harness)?.title ?? prompt.harness} asks for more` : ""}
        onClose={() => setPrompt(null)}
        actions={
          prompt && (
            <>
              <button type="button" className="btn" onClick={() => setPrompt(null)}>
                Don't install
              </button>
              <button
                type="button"
                className="btn solid"
                onClick={() => {
                  const p = prompt;
                  setPrompt(null);
                  void approveInstall(p.harness, p.token);
                }}
              >
                Allow and install
              </button>
            </>
          )
        }
      >
        {prompt && (
          <>
            <p style={{ marginTop: 0 }}>This version asks for more than the one before it:</p>
            <ul>
              {prompt.diff.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
            {prompt.native_reason && <p className="ls-muted">It runs as its own program on the server, because: {prompt.native_reason}</p>}
          </>
        )}
      </Dialog>
    </>
  );
}

function Tile({ entry, installed, busy, onInstall, onOpen, onRemove }: { entry: CatalogEntry; installed: boolean; busy: boolean; onInstall: () => void; onOpen: () => void; onRemove: () => void }) {
  const [open, setOpen] = useState(false);
  const blocked = entry.blocked !== null;
  const canOpen = installed && entry.doc_kind === "crdt";
  return (
    <div className={`tile${blocked ? " muted" : ""}`}>
      <div className="tile-top">
        <span className={`glyph${blocked ? " grey" : ""}`}>
          <BoardIcon size={21} />
        </span>
        {installed && <span className="pill green">Installed</span>}
      </div>
      <div className="tile-title" style={blocked ? { color: "var(--ls-muted)" } : undefined}>
        {entry.title}
      </div>
      <div className="tile-body">{firstSentence(entry.description) || "No description was given."}</div>
      <div className="tile-actions">
        {blocked ? (
          <span className="ls-small ls-faint" style={{ fontWeight: 600 }}>
            Not available here
          </span>
        ) : installed ? (
          canOpen ? (
            <button type="button" className="btn" onClick={onOpen}>
              Open
            </button>
          ) : (
            <span className="ls-small ls-faint">Ready in the chat</span>
          )
        ) : (
          <button type="button" className="btn solid" onClick={onInstall} disabled={busy}>
            {busy ? "Installing…" : "Install"}
          </button>
        )}
        <button type="button" className="link" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
          {open ? "Hide details" : "Details"}
        </button>
      </div>
      {open && (
        <dl className="tile-details" style={{ margin: 0 }}>
          {entry.description !== firstSentence(entry.description) && (
            <>
              <dt>About</dt>
              <dd>{entry.description}</dd>
            </>
          )}
          <dt>What it can do</dt>
          <dd>{entry.capability_lines.length > 0 ? entry.capability_lines.join(" ") : "It works only inside its own document."}</dd>
          <dt>Version</dt>
          <dd>
            {entry.version} · from {entry.publisher}
            {installed && entry.installed_version && entry.installed_version !== entry.version ? ` · ${entry.installed_version} is installed` : ""}
          </dd>
          <dt>Where it runs</dt>
          <dd>{entry.tier === "wasm" ? "Inside the app, in its own sandbox." : "As its own program on the server."}</dd>
          {entry.dependencies.length > 0 && (
            <>
              <dt>Needs</dt>
              <dd>{entry.dependencies.join(", ")}</dd>
            </>
          )}
          {entry.widens.length > 0 && (
            <>
              <dt>Asks for more than the version installed</dt>
              <dd>{entry.widens.join("; ")}</dd>
            </>
          )}
          {entry.native_reason && (
            <>
              <dt>Why it runs as its own program</dt>
              <dd>{entry.native_reason}</dd>
            </>
          )}
          {entry.blocked && (
            <>
              <dt>Why it is not available here</dt>
              <dd>{entry.blocked}</dd>
            </>
          )}
          {installed && (
            <dd style={{ marginTop: 12 }}>
              <button type="button" className="btn danger" onClick={onRemove}>
                Remove from this workspace
              </button>
            </dd>
          )}
        </dl>
      )}
    </div>
  );
}
