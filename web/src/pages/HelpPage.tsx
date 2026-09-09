import { Card, SectionTitle } from "../components/ui";

export function HelpPage() {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
      <Card className="max-w-3xl p-6 text-sm leading-relaxed">
        <SectionTitle>How this works</SectionTitle>
        <p>
          localSpace runs an agent over <em>harnesses</em>: installable tools with their own documents,
          each declaring exactly what it may touch. Nothing leaves this machine unless the network
          mode allows it, and every change a tool makes is a commit you can undo.
        </p>
        <h3 className="mt-4 font-medium">Getting a model</h3>
        <p>
          Open <strong>Models</strong> and connect an OpenAI-compatible endpoint, for example a running{" "}
          <span className="font-mono">llama-server</span>. The top bar shows <strong>Ready</strong> once one is
          selected.
        </p>
        <h3 className="mt-4 font-medium">Chat</h3>
        <p>
          Enter sends, Shift+Enter adds a line. Tool calls appear inline as they run; anything that needs
          your say-so appears as an approval card. <strong>Stop</strong> cancels the turn.
        </p>
        <h3 className="mt-4 font-medium">Tools, Agents, History</h3>
        <p>
          <strong>Tools</strong> shows what the model can reach this turn and why, and lets you run a tool
          yourself. <strong>Agents</strong> shows the task ledger and the exact prompt the model will see.{" "}
          <strong>History</strong> is the version DAG: undo, redo, or drop an agent's whole run.
        </p>
        <h3 className="mt-4 font-medium">Library</h3>
        <p>
          Harnesses come from the catalog the server was started with. Installing one that asks for wider
          capabilities than before shows the difference and asks first.
        </p>
      </Card>
    </div>
  );
}
