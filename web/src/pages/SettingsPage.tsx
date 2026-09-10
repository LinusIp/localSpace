// Settings: the session, the network mode under the admin's ceiling, and
// what this build is.

import { Button, Card, KeyValue, SectionTitle } from "@localspace/ui";
import { logout } from "../api/client";
import type { NetworkMode } from "../api/generated";
import { useSession } from "../store";

const MODES: Array<{ mode: NetworkMode; label: string; detail: string }> = [
  { mode: "airgapped", label: "Air-gapped", detail: "nothing leaves this machine" },
  { mode: "ask", label: "Ask", detail: "each outbound request is approved first" },
  { mode: "online", label: "Online", detail: "web tools may call out through the gateway" },
];

function rank(mode: NetworkMode): number {
  return mode === "airgapped" ? 0 : mode === "ask" ? 1 : 2;
}

export function SettingsPage() {
  const { me, environment, setNetwork, signOut } = useSession();
  const ceiling = environment?.network_ceiling ?? "airgapped";

  return (
    <div className="page page-grid-2">
      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle>Network</SectionTitle>
          <ul className="ls-list ls-col ls-gap-2">
            {MODES.map((m) => {
              const allowed = rank(m.mode) <= rank(ceiling);
              const current = environment?.network === m.mode;
              return (
                <li key={m.mode}>
                  <button type="button" disabled={!allowed} onClick={() => void setNetwork(m.mode)} className={`choice${current ? " current" : ""}`}>
                    <span className="ls-grow">
                      <div className="ls-medium">{m.label}</div>
                      <div className="ls-small ls-muted">{m.detail}</div>
                    </span>
                    {!allowed && <span className="ls-small ls-faint">above the admin ceiling</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        </Card>
        <Card className="ls-pad-4">
          <SectionTitle>Session</SectionTitle>
          <KeyValue
            rows={[
              ["user", me?.user ?? "…"],
              ["workspace", environment?.workspace ?? "…"],
              ["mode", me?.topology ?? "…"],
            ]}
          />
          <Button
            className="ls-mt-4"
            onClick={() => {
              void logout().then(signOut);
            }}
          >
            Sign out
          </Button>
        </Card>
      </div>
      <Card className="ls-pad-4">
        <SectionTitle>About</SectionTitle>
        <KeyValue
          rows={[
            ["localSpace", me?.version ?? "…"],
            ["harness API", me?.harness_api ?? "…"],
            ["machine", environment?.machine ?? "…"],
            ["model profile", environment?.profile ?? "…"],
          ]}
        />
        <p className="ls-mt-4 ls-small ls-muted">
          The API this client speaks is described at{" "}
          <a className="ls-accent" href="/api/v1/openapi.json" target="_blank" rel="noreferrer">
            /api/v1/openapi.json
          </a>
          , generated from the same types this client is built from.
        </p>
      </Card>
    </div>
  );
}
