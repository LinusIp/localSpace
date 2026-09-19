// The models for a USB stick, from models/catalog.json: which files, how
// large, and the SHA-256 each must have. The app takes a file it finds in its
// models folder as the model's only when its name, size and SHA-256 are the
// catalog's (docs/DECISIONS.md, 2026-09-19), so a stick is worth checking
// before the day and not at the tenth laptop.
//
//   node scripts/stick-list.mjs                  the list, as Markdown
//   node scripts/stick-list.mjs --check <folder> every file in that folder
//                                                against the list
//
// Reads files and nothing else: no network. It is run by a person, never by
// the product. Exits non-zero when a file in the folder is not the catalog's.

import { createHash } from "node:crypto";
import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalog = JSON.parse(readFileSync(join(root, "models/catalog.json"), "utf8"));
const entries = catalog.models.filter((m) => m.repo && m.verify).sort((a, b) => a.bytes - b.bytes);
const gib = (bytes) => `${(bytes / 2 ** 30).toFixed(2)} GiB`;

const sha256 = (path) =>
  new Promise((done, fail) => {
    const hash = createHash("sha256");
    createReadStream(path)
      .on("data", (chunk) => hash.update(chunk))
      .on("end", () => done(hash.digest("hex")))
      .on("error", fail);
  });

const at = process.argv.indexOf("--check");
if (at < 0) {
  console.log("| Model | Licence | File | Size | SHA-256 |");
  console.log("|---|---|---|---|---|");
  for (const m of entries) {
    for (const file of m.files) {
      const check = m.verify[file];
      console.log(`| ${m.title} | ${m.license_words} | \`${file}\` | ${gib(check.bytes)} | \`${check.sha256}\` |`);
    }
  }
  console.log("");
  console.log(`Every model together: ${gib(entries.reduce((sum, m) => sum + m.bytes, 0))}.`);
  console.log("Each file is at `https://huggingface.co/<repo>/resolve/<revision>/<file>`, with the repo and the revision of its entry in `models/catalog.json`.");
  process.exit(0);
}

const folder = process.argv[at + 1];
if (!folder || !existsSync(folder)) {
  console.error("give the folder to check: node scripts/stick-list.mjs --check <folder>");
  process.exit(2);
}

let wrong = 0;
for (const m of entries) {
  const found = m.files.filter((file) => existsSync(join(folder, file)));
  if (!found.length) {
    console.log(`absent   ${m.title}`);
    continue;
  }
  let whole = found.length === m.files.length;
  for (const file of found) {
    const path = join(folder, file);
    const check = m.verify[file];
    const size = statSync(path).size;
    if (size !== check.bytes) {
      whole = false;
      wrong += 1;
      console.log(`WRONG    ${file}: ${size} bytes, the catalog says ${check.bytes}`);
      continue;
    }
    const digest = await sha256(path);
    if (digest !== check.sha256) {
      whole = false;
      wrong += 1;
      console.log(`WRONG    ${file}: SHA-256 ${digest}, the catalog says ${check.sha256}`);
    }
  }
  const missing = m.files.filter((file) => !found.includes(file));
  if (missing.length) console.log(`PARTLY   ${m.title}: missing ${missing.join(", ")}`);
  else if (whole) console.log(`ok       ${m.title} (${gib(m.bytes)})`);
}
if (wrong) {
  console.error(`${wrong} file(s) are not the catalog's: the app will not take them as the model.`);
  process.exit(1);
}
