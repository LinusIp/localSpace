// Every file of every entry in models/catalog.json, asked of Hugging Face at
// its pinned address: does it answer, and are its size and SHA-256 the ones
// the catalog holds? Two of the first five entries could not have been
// downloaded when this was written (docs/DECISIONS.md, 2026-09-19): a file
// name that never existed, and one that upstream had replaced. Only headers
// are read; nothing is downloaded.
//
//   node scripts/check-catalog.mjs            every entry
//   node scripts/check-catalog.mjs <id> ...   some entries
//
// Exits non-zero when anything is wrong, so that it can be a CI job. It is
// run by a person or by CI, never by the product.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalog = JSON.parse(readFileSync(join(root, "models/catalog.json"), "utf8"));
const wanted = process.argv.slice(2);
const entries = catalog.models.filter((m) => m.repo && (!wanted.length || wanted.includes(m.id)));
if (!entries.length) {
  console.error("no such entry");
  process.exit(2);
}

let wrong = 0;
const say = (ok, text) => {
  if (!ok) wrong += 1;
  console.log(`${ok ? "ok   " : "WRONG"} ${text}`);
};

for (const m of entries) {
  if (!m.revision) say(false, `${m.id}: not pinned to a commit of ${m.repo}`);
  const total = m.files.reduce((sum, f) => sum + (m.verify?.[f]?.bytes ?? 0), 0);
  if (m.verify && total !== m.bytes) say(false, `${m.id}: its files come to ${total} bytes, the entry says ${m.bytes}`);
  for (const file of m.files) {
    const check = m.verify?.[file];
    const url = `https://huggingface.co/${m.repo}/resolve/${m.revision || "main"}/${file}`;
    let res;
    try {
      // The first answer carries the file's own size and digest; following
      // the redirect to the storage it points at would lose them.
      res = await fetch(url, { method: "HEAD", redirect: "manual" });
    } catch (e) {
      say(false, `${m.id}: ${file}: no answer (${e.cause?.code ?? e.message})`);
      continue;
    }
    if (![200, 301, 302, 307, 308].includes(res.status)) {
      say(false, `${m.id}: ${file}: answered ${res.status} at ${url}`);
      continue;
    }
    const size = Number(res.headers.get("x-linked-size") ?? res.headers.get("content-length"));
    const digest = (res.headers.get("x-linked-etag") ?? "").replaceAll('"', "");
    if (!check) {
      say(false, `${m.id}: ${file}: answers, but the entry says nothing of its size or digest (it is ${size} bytes, ${digest})`);
    } else if (size !== check.bytes) {
      say(false, `${m.id}: ${file}: ${size} bytes there, ${check.bytes} in the entry`);
    } else if (digest !== check.sha256) {
      say(false, `${m.id}: ${file}: SHA-256 ${digest} there, ${check.sha256} in the entry`);
    } else {
      say(true, `${m.id}: ${file} (${(size / 2 ** 30).toFixed(2)} GB)`);
    }
  }
}
console.log(wrong ? `${wrong} wrong` : `every file of ${entries.length} entries answers with the size and digest the catalog holds`);
process.exit(wrong ? 1 : 0);
