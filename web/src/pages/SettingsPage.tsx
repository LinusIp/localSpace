// Settings: the session, the network mode under the admin's ceiling, and
// what this build is.

import { logout } from "../api/client";
import type { NetworkMode } from "../api/generated";
import { useSession } from "../store";
import { Button, Card, KeyValue, SectionTitle } from "../components/ui";

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
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>Network</SectionTitle>
          <ul className="space-y-2">
            {MODES.map((m) => {
              const allowed = rank(m.mode) <= rank(ceiling);
              const current = environment?.network === m.mode;
              return (
                <li key={m.mode}>
                  <button
                    disabled={!allowed}
                    onClick={() => void setNetwork(m.mode)}
                    className={`flex w-full items-center gap-3 rounded-lg border px-3 py-2 text-left disabled:opacity-40 ${
                      current ? "border-accent bg-accent-soft" : "border-line hover:bg-page"
                    }`}
                  >
                    <span className="flex-1">
                      <div className="text-sm font-medium">{m.label}</div>
                      <div className="text-xs text-muted">{m.detail}</div>
                    </span>
                    {!allowed && <span className="text-xs text-faint">above the admin ceiling</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        </Card>
        <Card className="p-5">
          <SectionTitle>Session</SectionTitle>
          <KeyValue
            rows={[
              ["user", me?.user ?? "…"],
              ["workspace", environment?.workspace ?? "…"],
              ["mode", me?.topology ?? "…"],
            ]}
          />
          <Button
            className="mt-4"
            onClick={() => {
              void logout().then(signOut);
            }}
          >
            Sign out
          </Button>
        </Card>
      </div>
      <Card className="p-5">
        <SectionTitle>About</SectionTitle>
        <KeyValue
          rows={[
            ["localSpace", me?.version ?? "…"],
            ["harness API", me?.harness_api ?? "…"],
            ["machine", environment?.machine ?? "…"],
            ["model profile", environment?.profile ?? "…"],
          ]}
        />
        <p className="mt-4 text-xs text-muted">
          The API this client speaks is described at{" "}
          <a className="text-accent underline" href="/api/v1/openapi.json" target="_blank" rel="noreferrer">
            /api/v1/openapi.json
          </a>
          , generated from the same types this client is built from.
        </p>
      </Card>
    </div>
  );
}
