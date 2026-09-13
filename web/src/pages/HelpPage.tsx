// Help: short, in the words of the screens, with one place to turn.

import { useSession } from "../store";
import { TopBar } from "../components/TopBar";

export function HelpPage() {
  const { me, environment } = useSession();
  const organisation = me?.topology === "organisation";
  const board = environment?.harnesses.some((h) => h.id === "io.localspace.whiteboard") ?? false;
  return (
    <>
      <TopBar />
      <div className="page">
        <h1 className="page-title">Help</h1>
        <div className="page-sub">{organisation ? "localSpace runs on your organisation's own server. Nothing you type leaves your network." : "localSpace runs on this computer."}</div>
        <div className="row-list" style={{ maxWidth: 660 }}>
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">Ask in your own words</div>
              <div className="row-body">Type what you want in the box and press Enter. Shift+Enter adds a line. The assistant answers, and asks you before it does anything that needs your say-so.</div>
            </div>
          </div>
          {board && (
            <div className="row-item">
              <div className="row-main">
                <div className="row-title">The whiteboard</div>
                <div className="row-body">Open Whiteboard in the sidebar. Notes, shapes and arrows are yours to move; ask the assistant for a plan and it puts one on the board. Ctrl+Z undoes, Ctrl+Shift+Z redoes.</div>
              </div>
            </div>
          )}
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">Choosing the assistant</div>
              <div className="row-body">Settings → Assistant lists what is available and what each is good for. Settings → Network says whether the assistant may reach the internet.</div>
            </div>
          </div>
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">If something is wrong</div>
              <div className="row-body">{organisation ? "Ask your IT team. They can see the server, add people, and send you a new sign-in link." : "Settings → Advanced shows what the app knows about itself."}</div>
            </div>
          </div>
        </div>
        <p className="ls-small ls-faint" style={{ marginTop: 22 }}>
          localSpace {me?.version ?? ""}
        </p>
      </div>
    </>
  );
}
