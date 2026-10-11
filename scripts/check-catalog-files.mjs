// Every file of every catalog entry is still at its download address, with
// the size and SHA-256 the entry records (document 34 §4, check 7,
// docs/DECISIONS.md, 2026-10-11). Hugging Face answers a file's address with
// a redirect whose headers carry the file's size and its SHA-256, so nothing
// is downloaded: one request per file, at the entry's pinned revision.
//
//   node scripts/check-catalog-files.mjs [models/catalog.json]
//
// CI runs it; it needs the network, which CI has and the app never uses for
// this.

import { readFileSync } from "node:fs";

const path = process.argv[2] ?? "models/catalog.json";
const { models } = JSON.parse(readFileSync(path, "utf8"));
let failures = 0;
let checked = 0;
for (const m of models) {
  if (!m.repo) continue;
  for (const file of m.files ?? []) {
    const published = m.published_as?.[file] ?? file;
    const want = m.verify?.[file];
    const url = `https://huggingface.co/${m.repo}/resolve/${m.revision ?? "main"}/${published}`;
    let res;
    for (let attempt = 1; ; attempt++) {
      try {
        res = await fetch(url, { method: "HEAD", redirect: "manual" });
        if (res.status < 500 || attempt === 3) break;
      } catch (e) {
        if (attempt === 3) throw e;
      }
      await new Promise((r) => setTimeout(r, 2000 * attempt));
    }
    checked++;
    const size = Number(res.headers.get("x-linked-size") ?? res.headers.get("content-length") ?? NaN);
    const sha = (res.headers.get("x-linked-etag") ?? "").replaceAll('"', "");
    const problems = [];
    if (![200, 302, 307].includes(res.status)) problems.push(`answered ${res.status}`);
    if (want?.bytes && size !== want.bytes) problems.push(`size ${size}, the entry says ${want.bytes}`);
    if (want?.sha256 && sha && sha !== want.sha256) problems.push(`SHA-256 ${sha}, the entry says ${want.sha256}`);
    if (problems.length) {
      failures++;
      console.log(`FAIL ${m.id}: ${published}: ${problems.join("; ")}`);
    } else {
      console.log(`ok   ${m.id}: ${published}${want ? "" : " (no size or digest recorded)"}`);
    }
  }
}
console.log(`${checked} files checked, ${failures} failed`);
process.exit(failures ? 1 : 0);
