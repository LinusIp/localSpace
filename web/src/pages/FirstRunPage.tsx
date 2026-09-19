// The first run on a person's own computer (the "any hardware" plan of
// 2026-09-18): what this computer is, in plain words, the model localSpace
// would start with, how it will run, and one button. No file names, no
// scores, no list to choose from; choosing is one link away.

import { useEffect, useRef, useState } from "react";
import { sizeInWords } from "../lib/models";
import { useSession } from "../store";

export function FirstRunPage() {
  const { computer, catalogModels, environment, downloadModel, loadModel, leaveFirstRun, go, goSettings } = useSession();
  const started = useRef(false);
  // The one deliberate click: nothing starts, downloaded or already here,
  // before the person has read what the page says and accepted it.
  const [accepted, setAccepted] = useState(false);

  const model = catalogModels.find((m) => m.id === computer?.recommended) ?? null;
  const downloading = !!model?.download && model.download.stage.startsWith("downloading");
  const failed = !!model?.download && model.download.stage.startsWith("failed");
  // Stopped part-way, by a lost connection or by closing the app: what came is kept.
  const paused = !!model?.download && model.download.stage === "paused";
  // Its SHA-256 is being compared with the published one: after a download,
  // or for a file that was already on this computer (copied from a stick).
  const checking = !!model?.download && model.download.stage === "verifying";
  const percent = model?.download && model.download.total_bytes > 0 ? Math.min(100, (100 * model.download.done_bytes) / model.download.total_bytes) : 0;
  const engine = environment?.engine;
  const starting = !!model && !!engine && engine.loading && engine.model === model.id;
  const offline = environment?.network === "airgapped";

  // Once accepted and here it is started, and once it answers this page is done.
  useEffect(() => {
    if (!accepted || !model?.installed || started.current) return;
    started.current = true;
    void loadModel(model.id);
  }, [accepted, model?.installed, model?.id, loadModel]);
  useEffect(() => {
    if (model && engine?.running && engine.model === model.id) {
      leaveFirstRun();
      go("chat");
    }
  }, [model, engine?.running, engine?.model, leaveFirstRun, go]);

  if (!computer) return null;
  const place = computer.disk.charAt(0).toUpperCase() + computer.disk.slice(1);
  const room = computer.disk_free_gb === null ? null : `${place} has ${computer.disk_free_gb} GB free.`;

  return (
    <div className="auth">
      <div className="auth-column" style={{ width: 520 }}>
        <img src="/brand/localspace-lockup.png" alt="localSpace" style={{ width: 224, height: "auto", display: "block" }} />
        <div className="auth-card">
          <div className="auth-title">This computer</div>
          <div className="first-run-computer">{computer.sentence}</div>
          {computer.notes.map((note) => (
            <div key={note} className="auth-sub">
              {note}
            </div>
          ))}

          {model ? (
            <div className="first-run-model">
              <div className="first-run-label">Recommended for it</div>
              <div className="first-run-title">{model.title}</div>
              <div className="first-run-verdict">
                {model.verdict_label}
                {model.speed && <span className="ls-muted"> · {model.speed}</span>}
              </div>
              <div className="auth-sub">{model.placement}</div>
              {model.quality_words && <div className="auth-sub">{model.quality_words}</div>}
              {model.license_words && <div className="auth-sub">{model.license_words}</div>}
              {model.installed ? (
                <div className="auth-sub">It is already on this computer: nothing to download.</div>
              ) : (
                !checking && (
                  <div className="auth-sub">
                    About {sizeInWords(model.bytes)} to download.{room ? ` ${room}` : ""}
                  </div>
                )
              )}
              {(downloading || paused || checking || starting || (accepted && model.installed)) && (
                <div style={{ marginTop: 18 }}>
                  <div className="progress">
                    <div style={{ width: `${model.installed ? 100 : percent}%` }} />
                  </div>
                  <div className="auth-sub">{model.installed ? "Starting it up. A larger model takes a minute." : checking ? "Checking that the file on this computer is the published one…" : paused ? `${percent.toFixed(0)}% is already here.` : `${percent.toFixed(0)}% downloaded`}</div>
                </div>
              )}
              {failed && <div className="error">The download stopped. What came is kept: start it again and it continues from there.</div>}
              {offline && !model.installed && <div className="auth-sub">Downloads need the network, and this computer is set to stay offline.</div>}
              <button
                type="button"
                className="auth-button"
                onClick={() => {
                  setAccepted(true);
                  if (!model.installed) void downloadModel(model.id);
                }}
                disabled={downloading || checking || starting || (accepted && model.installed) || (offline && !model.installed)}
              >
                {downloading ? "Downloading…" : checking ? "Checking…" : starting || (accepted && model.installed) ? "Starting…" : model.installed ? "Start" : paused || failed ? "Continue the download" : "Download and start"}
              </button>
            </div>
          ) : (
            <div className="first-run-model">
              <div className="auth-sub">None of the models localSpace knows fits this computer's memory and disk.</div>
            </div>
          )}

          <div className="auth-help">
            <button
              type="button"
              className="link green"
              onClick={() => {
                leaveFirstRun();
                goSettings("assistant");
              }}
            >
              Choose a different model
            </button>
            <span> · </span>
            <button type="button" className="link" onClick={leaveFirstRun}>
              Not now
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
