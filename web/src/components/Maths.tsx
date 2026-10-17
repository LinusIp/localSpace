// A formula in an answer, rendered by KaTeX 0.19.0 (MIT; docs/DECISIONS.md,
// 2026-10-10 and 2026-10-18). KaTeX and its stylesheet are fetched the first
// time an answer has maths, so the base bundle does not carry them; its fonts
// come by relative address, woff2 only. `katex.render()` writes into an
// element of its own that React never fills, and builds its nodes through the
// DOM, never from an HTML string. Until it is ready, or if it cannot be
// loaded, the formula's source is shown as code.

import { useEffect, useRef, useState } from "react";

type Katex = (typeof import("katex"))["default"];

let loading: Promise<Katex> | null = null;

function loadKatex(): Promise<Katex> {
  loading ??= Promise.all([import("katex"), import("katex/dist/katex.min.css")]).then(([module]) => module.default);
  return loading;
}

export function Maths({ tex, display }: { tex: string; display: boolean }) {
  const target = useRef<HTMLSpanElement>(null);
  const [rendered, setRendered] = useState(false);
  useEffect(() => {
    let live = true;
    loadKatex()
      .then((katex) => {
        if (!live || !target.current) return;
        // A formula KaTeX cannot read is shown in red as written, never thrown.
        katex.render(tex, target.current, { displayMode: display, throwOnError: false, trust: false, strict: "ignore" });
        setRendered(true);
      })
      .catch(() => {
        // KaTeX could not be loaded: the source stays, as code.
      });
    return () => {
      live = false;
    };
  }, [tex, display]);
  return (
    <span className={display ? "maths maths-display" : "maths"}>
      {!rendered && <code className="maths-source">{tex}</code>}
      <span ref={target} />
    </span>
  );
}
