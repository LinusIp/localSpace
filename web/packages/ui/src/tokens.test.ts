import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

// Every colour on screen comes from a --ls-* token, so a theme can change
// all of them in one place. A colour written straight into a stylesheet or a
// component would stay the same in every theme.

const WEB = new URL("../../../", import.meta.url);
const COLOUR = /#[0-9a-f]{3,8}\b|rgba?\(/i;

function read(path: string): string {
  return readFileSync(new URL(path, WEB), "utf8");
}

/** The files under a folder with one of the extensions, as paths from web/. */
function filesIn(folder: string, extensions: string[]): string[] {
  const entries = readdirSync(new URL(folder, WEB), { recursive: true, withFileTypes: true });
  return entries
    .filter((e) => e.isFile() && extensions.some((x) => e.name.endsWith(x)) && !e.name.endsWith(".test.ts"))
    .map((e) => relative(fileURLToPath(WEB), join(e.parentPath, e.name)).replaceAll("\\", "/"));
}

/** A stylesheet without its comments, its token definitions and the fallbacks
 *  inside var(--ls-…, fallback), which only apply where the tokens are absent. */
function stripAllowed(css: string): string {
  let out = css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*--ls-[\w-]+:.*$/gm, "");
  for (let at = out.indexOf("var(--ls-"); at !== -1; at = out.indexOf("var(--ls-", at + 1)) {
    let depth = 0;
    let end = at;
    for (; end < out.length; end++) {
      if (out[end] === "(") depth++;
      if (out[end] === ")" && --depth === 0) break;
    }
    out = `${out.slice(0, at)}var()${out.slice(end + 1)}`;
  }
  return out;
}

test("no stylesheet names a colour outside the token definitions", () => {
  const sheets = ["packages/ui/src/ui.css", "packages/ui/src/look.css", "src/index.css", ...filesIn("surfaces/", [".css"]).filter((p) => p.includes("/src/"))];
  for (const path of sheets) {
    const lines = stripAllowed(read(path)).split("\n");
    const found = lines.filter((l) => COLOUR.test(l));
    assert.deepEqual(found, [], `${path} names a colour; use a --ls-* token`);
  }
});

test("no component names a colour in a string", () => {
  const literal = /["'`]#[0-9a-f]{3,8}["'`]|["'`]rgba?\(/i;
  const sources = [...filesIn("src/", [".ts", ".tsx"]), ...filesIn("packages/ui/src/", [".ts", ".tsx"])];
  for (const path of sources) {
    const found = read(path).split("\n").filter((l) => literal.test(l));
    assert.deepEqual(found, [], `${path} names a colour; use var(--ls-…)`);
  }
});

test("the stripping keeps what it should catch", () => {
  assert.match(stripAllowed(".a { color: #fff; }"), COLOUR);
  assert.doesNotMatch(stripAllowed("  --ls-ink: #1c1e20;\n.a { color: var(--ls-ink, #1c1e20); }"), COLOUR);
  assert.doesNotMatch(stripAllowed(".a { box-shadow: var(--ls-x, 0 1px rgba(0, 0, 0, 0.1)); }"), COLOUR);
  assert.match(stripAllowed(".a { box-shadow: var(--ls-x, 0 1px rgba(0, 0, 0, 0.1)); color: #000; }"), COLOUR);
});

// The contrast check (docs/design/0.2.1-THEMES-AND-TRANSLATION.md §2): every
// palette passes WCAG AA in light and dark. Text needs 4.5:1; controls and
// the decorative `faint` need 3:1.

type Tokens = Record<string, string>;

/** The --ls-* colours a rule in ui.css sets, by the rule's exact selector. */
function block(css: string, selector: string): Tokens {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `ui.css has no rule for ${selector}`);
  const body = css.slice(start, css.indexOf("}", start));
  const tokens: Tokens = {};
  for (const m of body.matchAll(/--ls-([\w-]+):\s*(#[0-9a-f]{6});/gi)) tokens[m[1]] = m[2];
  return tokens;
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const v = parseInt(hex.slice(i, i + 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** [foreground, background, minimum]: the pairs the design brief checks. */
const PAIRS: [string, string, number][] = [
  ["ink", "page", 4.5], ["ink-soft", "page", 4.5], ["muted", "page", 4.5],
  ["ink", "surface", 4.5], ["muted", "surface", 4.5], ["muted", "rail", 4.5],
  ["accent", "page", 4.5], ["accent", "surface", 4.5], ["accent", "accent-soft", 4.5], ["ink", "accent-soft", 4.5],
  ["on-accent", "accent-fill", 4.5], ["on-accent", "accent-strong", 4.5],
  ["highlight", "page", 4.5], ["highlight", "highlight-soft", 4.5], ["on-highlight", "highlight-fill", 4.5],
  ["ok", "ok-soft", 4.5], ["warn", "warn-soft", 4.5], ["danger", "danger-soft", 4.5], ["danger", "page", 4.5],
  // Text on ink (avatars, the send button, toasts) is the page colour.
  ["page", "ink", 4.5], ["page", "warn", 4.5],
  ["faint", "page", 3], ["faint", "surface", 3],
];

const PALETTE_NAMES = ["indigo", "violet", "teal"];

test("every palette, light and dark, passes the contrast pairs", () => {
  const css = read("packages/ui/src/ui.css");
  const failures: string[] = [];
  for (const name of PALETTE_NAMES) {
    for (const [mode, selector] of [["light", `:root[data-palette="${name}"]`], ["dark", `:root[data-palette="${name}"][data-theme="dark"]`]]) {
      const t = block(css, selector);
      for (const [fg, bg, min] of PAIRS) {
        assert.ok(t[fg] && t[bg], `${name} ${mode} lacks --ls-${fg} or --ls-${bg}`);
        const ratio = contrast(t[fg], t[bg]);
        if (ratio < min) failures.push(`${name} ${mode}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1, needs ${min}:1`);
      }
    }
  }
  assert.deepEqual(failures, []);
});

test("the initials on every avatar colour pass in every palette", () => {
  const css = read("packages/ui/src/ui.css");
  const failures: string[] = [];
  const avatars = { light: block(css, ":root[data-palette]"), dark: block(css, `:root[data-palette][data-theme="dark"]`) };
  for (const name of PALETTE_NAMES) {
    for (const mode of ["light", "dark"] as const) {
      const selector = mode === "light" ? `:root[data-palette="${name}"]` : `:root[data-palette="${name}"][data-theme="dark"]`;
      const page = block(css, selector).page;
      for (const n of [1, 2, 3, 4, 5]) {
        const colour = avatars[mode][`avatar-${n}`];
        assert.ok(colour, `${mode} lacks --ls-avatar-${n}`);
        const ratio = contrast(page, colour);
        if (ratio < 4.5) failures.push(`${name} ${mode}: initials on avatar-${n} are ${ratio.toFixed(2)}:1`);
      }
    }
  }
  assert.deepEqual(failures, []);
});

test("the contrast maths matches WCAG's own examples", () => {
  assert.equal(contrast("#000000", "#ffffff").toFixed(1), "21.0");
  assert.equal(contrast("#777777", "#ffffff").toFixed(2), "4.48");
});
