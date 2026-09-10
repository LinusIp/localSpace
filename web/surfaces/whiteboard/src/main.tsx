// The whiteboard's web surface: the canvas engine and the UI library from
// the shell's import map, the document as an Automerge replica, every edit
// to Core through the bridge (architecture v2.1 §6.3, §13 step 5).

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { connect } from "@localspace/harness-sdk";
import { Board } from "./Board.tsx";
import style from "./style.css?inline";

const root = document.getElementById("root");
if (!root) throw new Error("the page has no root");

// The surface's own layout, in the page with it: the generated page has no
// link to give it, and the origin's policy allows an inline style.
const sheet = document.createElement("style");
sheet.textContent = style;
document.head.appendChild(sheet);

connect({ sync: true })
  .then((harness) => {
    createRoot(root).render(
      <StrictMode>
        <Board harness={harness} />
      </StrictMode>,
    );
  })
  .catch((err: unknown) => {
    root.textContent = String(err);
    root.className = "ls-empty ls-muted";
  });
