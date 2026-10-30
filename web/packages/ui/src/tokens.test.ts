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
  const sheets = ["packages/ui/src/ui.css", "src/index.css", ...filesIn("surfaces/", [".css"]).filter((p) => p.includes("/src/"))];
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
