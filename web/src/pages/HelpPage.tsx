import { Card, SectionTitle } from "@localspace/ui";

export function HelpPage() {
  return (
    <div className="page">
      <Card className="ls-pad-6" style={{ maxWidth: "48rem", lineHeight: 1.6 }}>
        <SectionTitle>How this works</SectionTitle>
        <p>
          localSpace runs an agent over <em>harnesses</em>: installable tools with their own documents, each declaring exactly what it may touch.
          Nothing leaves this machine unless the network mode allows it, and every change a tool makes is a commit you can undo.
        </p>
        <h3 className="ls-mt-4 ls-medium">Getting a model</h3>
        <p>
          Open <strong>Models</strong> and choose one from the catalog, or connect an OpenAI-compatible endpoint, for example a running{" "}
          <span className="ls-mono">llama-server</span>. The top bar shows <strong>Ready</strong> once one is selected.
        </p>
        <h3 className="ls-mt-4 ls-medium">Chat</h3>
        <p>
          Enter sends, Shift+Enter adds a line. Tool calls appear inline as they run; anything that needs your say-so appears as an approval card.{" "}
          <strong>Stop</strong> cancels the turn.
        </p>
        <h3 className="ls-mt-4 ls-medium">Tools, Agents, History</h3>
        <p>
          <strong>Tools</strong> shows what the model can reach this turn and why, lets you run a tool yourself, and opens a harness's views as panels
          beside the chat. <strong>Agents</strong> shows the task ledger and the exact prompt the model will see. <strong>History</strong> is the version
          DAG: undo, redo, or drop an agent's whole run.
        </p>
        <h3 className="ls-mt-4 ls-medium">Library</h3>
        <p>
          Only chat ships in the box; harnesses come from the catalog the server was started with and are installed from there. Installing one that asks
          for wider capabilities than before shows the difference and asks first.
        </p>
      </Card>
    </div>
  );
}
