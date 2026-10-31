// Help: short, in the words of the screens, with one place to turn.

import { useSession } from "../store";
import { TopBar } from "../components/TopBar";
import { useStrings } from "../useStrings";

/** localSpace's discussion group, opened in the person's browser; nothing is sent from the app. */
const FEEDBACK = "https://t.me/localSpace_discussion";

export function HelpPage() {
  const { me, environment } = useSession();
  const strings = useStrings();
  const organisation = me?.topology === "organisation";
  const board = environment?.harnesses.some((h) => h.id === "io.localspace.whiteboard") ?? false;
  return (
    <>
      <TopBar />
      <div className="page">
        <h1 className="page-title">{strings.helpTitle}</h1>
        <div className="page-sub">{organisation ? strings.helpSubOrganisation : strings.helpSubPersonal}</div>
        <div className="row-list" style={{ maxWidth: 660 }}>
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">{strings.helpAskTitle}</div>
              <div className="row-body">{strings.helpAskBody}</div>
            </div>
          </div>
          {/* Only where Core offers the whiteboard, which no release does; its
              words leave with it rather than moving to the strings. */}
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
              <div className="row-title">{strings.helpChooseTitle}</div>
              <div className="row-body">{strings.helpChooseBody}</div>
            </div>
          </div>
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">{strings.helpWrongTitle}</div>
              <div className="row-body">{organisation ? strings.helpWrongOrganisation : strings.helpWrongPersonal}</div>
            </div>
          </div>
          <div className="row-item">
            <div className="row-main">
              <div className="row-title">{strings.helpFeedbackTitle}</div>
              <div className="row-body">
                {strings.helpFeedbackBody}{" "}
                <a className="link green" href={FEEDBACK} target="_blank" rel="noreferrer">
                  {strings.helpFeedbackLink}
                </a>
              </div>
            </div>
          </div>
        </div>
        <p className="ls-small ls-muted" style={{ marginTop: 22 }}>
          localSpace {me?.version ?? ""}
        </p>
      </div>
    </>
  );
}
