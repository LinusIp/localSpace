// Size limits for what the web build ships, in gzipped bytes: the base client
// bundle at most 2 MB compressed (CLAUDE.md, definition of done for the MVP),
// and a limit per package in size-limits.json. Run in web/ after the shell's
// build and the surfaces':
//
//   node scripts/check-sizes.mjs       exit code 1 when anything is over its limit
//
// gzip at zlib's default level, the same as `gzip -c`.

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join, resolve } from "node:path";

const root = process.cwd();
const { limits } = JSON.parse(readFileSync(resolve(root, "size-limits.json"), "utf8"));
const kb = (n) => `${(n / 1024).toFixed(1)} KB`;

let failed = 0;
const rows = [["what", "raw", "gzipped", "limit", ""]];
for (const limit of limits) {
  const files = (limit.files ?? []).map((f) => resolve(root, f));
  if (limit.dir) {
    const dir = resolve(root, limit.dir);
    const match = new RegExp(limit.match ?? ".");
    if (existsSync(dir)) for (const name of readdirSync(dir)) if (match.test(name)) files.push(join(dir, name));
  }
  const missing = files.filter((f) => !existsSync(f));
  if (files.length === 0 || missing.length) {
    failed += 1;
    rows.push([limit.name, "", "", kb(limit.gzip), `MISSING ${missing.join(", ") || limit.dir}`]);
    continue;
  }
  let raw = 0;
  let gzipped = 0;
  for (const file of files) {
    const bytes = readFileSync(file);
    raw += bytes.length;
    gzipped += gzipSync(bytes).length;
  }
  const over = gzipped > limit.gzip;
  if (over) failed += 1;
  rows.push([limit.name, kb(raw), kb(gzipped), kb(limit.gzip), over ? "OVER" : "ok"]);
}

const widths = rows[0].map((_, i) => Math.max(...rows.map((r) => r[i].length)));
for (const row of rows) console.log(row.map((cell, i) => (i === 0 ? cell.padEnd(widths[i]) : cell.padStart(widths[i]))).join("  "));
if (failed) {
  console.error(`${failed} over its limit or missing`);
  process.exitCode = 1;
}
