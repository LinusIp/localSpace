// What the release check found, kept as it goes and written out as the run's
// summary: a pass/fail table first, then the answers in Russian and Uzbek as
// text, then the figures read from the app's log. Written so a phone can
// read it; the screenshots go with the run as files.

import fs from "node:fs";
import path from "node:path";

export class Report {
  constructor(out) {
    this.out = out;
    this.rows = [];
    this.answers = [];
    this.figures = [];
    this.notes = [];
    fs.mkdirSync(path.join(out, "screens"), { recursive: true });
  }

  /** One line of the table. `ok` is true, false, or null for "not reached". */
  row(item, ok, detail = "") {
    this.rows.push({ item, ok, detail });
    console.log(`${ok === true ? "PASS" : ok === false ? "FAIL" : "----"}  ${item}${detail ? `: ${detail}` : ""}`);
    this.write();
  }

  /** Runs `work` as one item: its returned text is the detail; a throw is a failure. */
  async item(item, work) {
    try {
      const detail = await work();
      this.row(item, true, typeof detail === "string" ? detail : "");
      return true;
    } catch (error) {
      this.row(item, false, String(error?.message ?? error).split("\n")[0].slice(0, 300));
      return false;
    }
  }

  answer(language, question, text) {
    this.answers.push({ language, question, text });
    this.write();
  }

  figure(what, value) {
    this.figures.push({ what, value });
    this.write();
  }

  note(text) {
    this.notes.push(text);
    this.write();
  }

  get passed() {
    return this.rows.length > 0 && this.rows.every((r) => r.ok === true);
  }

  write() {
    const cell = (s) => String(s).replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
    const lines = [
      `# Release check: ${this.passed ? "every item passed" : "not every item passed"}`,
      "",
      "| Item | Result | Detail |",
      "|---|---|---|",
      ...this.rows.map((r) => `| ${cell(r.item)} | ${r.ok === true ? "Pass" : r.ok === false ? "**Fail**" : "Not reached"} | ${cell(r.detail)} |`),
      "",
    ];
    if (this.answers.length) {
      lines.push("## Answers in Russian and Uzbek", "");
      for (const a of this.answers) lines.push(`**${a.language}, asked:** ${a.question}`, "", ...a.text.split(/\r?\n/).map((l) => `> ${l}`), "");
    }
    if (this.figures.length) {
      lines.push("## Read from the app's log", "", "| What | Value |", "|---|---|", ...this.figures.map((f) => `| ${cell(f.what)} | ${cell(f.value)} |`), "");
    }
    if (this.notes.length) lines.push("## Notes", "", ...this.notes.map((n) => `- ${n}`), "");
    lines.push(
      "The screenshots of every screen, in English, Russian and Uzbek, and the app's log are this run's artifact, **release-check-screens-and-logs**.",
      "",
      "This machine has no graphics card and runs Windows Server: the graphics card, Smart App Control and speed are not checked here, and no figure from here is published.",
      "",
    );
    fs.writeFileSync(path.join(this.out, "summary.md"), lines.join("\n"));
    fs.writeFileSync(path.join(this.out, "report.json"), JSON.stringify({ rows: this.rows, answers: this.answers, figures: this.figures, notes: this.notes }, null, 2));
  }
}
