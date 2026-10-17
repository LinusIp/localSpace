// Maths in an answer, found before its Markdown is parsed (docs/DECISIONS.md,
// 2026-10-18). Markdown would read `x_1` inside a formula as emphasis and
// `\(` as an escaped bracket, so each formula is lifted out first and a
// placeholder left in its place; the renderer puts KaTeX's rendering where the
// placeholder is. Code, fenced or inline, is left exactly as it is.
//
// What counts as maths, as models write it:
//   $$ … $$  and  \[ … \]      a formula on a line of its own (display)
//   \( … \)                    a formula within a sentence
//   $ … $                      within a sentence, as Pandoc reads it: the
//                              opening $ is followed by something other than a
//                              space, the closing $ follows something other than
//                              a space and is not followed by a digit, so that
//                              "$5 and $10" stays two prices.

export interface MathsPart {
  tex: string;
  display: boolean;
}

export interface WithMaths {
  /** The answer with every formula replaced by its placeholder. */
  text: string;
  /** The formulas, by the number in their placeholder. */
  parts: MathsPart[];
}

/** A placeholder's marks: characters from Unicode's private-use area, which no answer is shown with. */
export const OPEN = "";
export const CLOSE = "";

const TOKEN = /(`+)[\s\S]*?\1|\$\$([\s\S]+?)\$\$|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)|(?<!\\)\$(?=\S)([^$\n]*?\S)\$(?!\d)/g;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

export function extractMaths(source: string): WithMaths {
  const parts: MathsPart[] = [];
  const lift = (prose: string): string =>
    prose.replace(TOKEN, (whole, code: string | undefined, dollars?: string, brackets?: string, parens?: string, inline?: string) => {
      if (code !== undefined) return whole;
      const display = dollars ?? brackets;
      const tex = (display ?? parens ?? inline ?? "").trim();
      if (!tex) return whole;
      parts.push({ tex, display: display !== undefined });
      return `${OPEN}${parts.length - 1}${CLOSE}`;
    });

  // Fenced code blocks pass through untouched; the prose between them is
  // searched as one piece, so that a display formula may span lines.
  const out: string[] = [];
  let prose: string[] = [];
  const flush = () => {
    if (prose.length) out.push(lift(prose.join("\n")));
    prose = [];
  };
  let fence: string | null = null;
  for (const line of source.replaceAll(OPEN, "").replaceAll(CLOSE, "").split("\n")) {
    const mark = FENCE.exec(line)?.[1];
    if (fence === null && mark) {
      flush();
      fence = mark;
      out.push(line);
    } else if (fence !== null) {
      out.push(line);
      if (mark && mark[0] === fence[0] && mark.length >= fence.length) fence = null;
    } else {
      prose.push(line);
    }
  }
  flush();
  return { text: out.join("\n"), parts };
}

/** A text with placeholders, split into its plain pieces and the formulas' numbers. */
export function splitAtMaths(text: string): Array<string | number> {
  const pieces: Array<string | number> = [];
  const mark = new RegExp(`${OPEN}(\\d+)${CLOSE}`, "g");
  let from = 0;
  for (const match of text.matchAll(mark)) {
    if (match.index > from) pieces.push(text.slice(from, match.index));
    pieces.push(Number(match[1]));
    from = match.index + match[0].length;
  }
  if (from < text.length) pieces.push(text.slice(from));
  return pieces;
}
