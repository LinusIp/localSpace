# The laptop test: for the people running it

Test A is **Monday 21 September 2026**, unsupervised: ten people install
localSpace on their own computers, in their own time, with the installer and
`docs/TEST-A-TESTER-SHEET.md` and nobody beside them
(docs/DECISIONS.md, 2026-09-19, the answers after day 4 and their addendum).
This page is for us; the sheet is for them and holds nothing of this.

## Who is sent the installer

The screening answers (graphics card, memory, free disk, Windows, Smart App
Control) decide two things for each person.

- **Smart App Control "On": a signed build, or not at all.** An unsigned
  installer is refused outright there, with no button, and unsupervised that
  is a lost tester we would not even hear from. "Evaluation" and "Off" can
  take part with the unsigned build: they meet the SmartScreen warning the
  sheet prepares them for.
- **What they will be offered.** Send the graphics cards to the builder
  before Monday: each is run through the machine-shape test
  (`what_typical_computers_are_told_and_offered` in
  `crates/localspace-core/src/models.rs`), so that what each named tester
  is told and offered is known before they start, and a card the table
  lacks is added with its manufacturer's figure.

- **AMD machines are the least-validated path in the product.** Everything
  measured so far was measured on one NVIDIA laptop; an AMD card runs
  through the same Vulkan engine on figures AMD publishes as "up to". **On
  Monday their logs are read first.**
- Smart App Control's verdicts are not uniform: on the development laptop,
  with it on, it blocked neither the unsigned installer nor the unsigned app
  on 19 September, while refusing the Rust compiler for two hours the same
  evening. The rule above stands all the same: nobody whose answer says
  "On" is sent an unsigned installer.

Five to seven usable results of ten is a normal outcome of an unsupervised
test, and the conclusions are planned around that number. Recruiting a few
more people before Monday is the cheapest insurance there is.

## What is sent

1. **The installer**, `localSpace-<version>-<build>-windows-x64-setup.exe`
   (about 35 MB), from the `package` workflow's artefact of the head the dry
   run passed on (Monday morning: one clean build of that same head), **as a
   link to a cloud drive the founder controls, with the SHA-256 from
   `SHA256SUMS.txt` in the email**. Google Drive puts its own "can't scan
   this file for viruses" page before the download; the sheet pictures it. A
   GitHub Release on the public localLabs repository would spare that page
   and make an unsigned pre-release public: the founder's call.
2. **The sheet**, with the ‹pictures› from the dry run put in, and the
   Telegram contact or group filled in at its top. The ten questions are at
   its end.

Nothing else: no stick, no model file. Each tester downloads their model
over their own connection. `scripts/stick-list.mjs` and the recognition of a
copied file stay in the product for a company that is air-gapped.

## The dry run, Sunday morning

On a Windows machine that is not the builder's, **with the release build,
downloaded the way a tester will: through the real link, in a browser, on a
machine that has never seen the file** (testing the artefact without the
link tests half of it), **following the sheet to the letter and using
nothing a tester would not know.** The sheet is under test as much as the build: every place where you
had to think is a defect of the sheet, and is fixed that afternoon. Best of
all: someone who is neither the builder nor the sheet's author follows it on
their own machine while you watch and say nothing.

Write down, for that machine:

- **every dialog a tester meets, in order, as a picture** (they go into the
  sheet): the download page and its warning, the browser's own warning,
  SmartScreen as it first appears and after *More info*, the installer's
  pages (first, progress, completed), and the folder that holds `app.log`;
- the exact clicks at each of them;
- **the installer's clicks, by watching them**: the sheet says *Next*, *Next*
  on the folder page, *Next* once *Completed* shows, *Finish*. The folder
  page's button was seen to read *Next* (19 September); the rest is read
  from the installer's template and has not been watched. A wrong
  instruction to a person alone is worse than none: they believe the sheet
  over the screen. How long it took;
- what the first start said about the machine, what it recommended, and with
  what estimate;
- how long the download took, and whether the three stages read as the
  sheet's table says;
- the time from double-click to the first answer, end to end;
- the measured speed against the promised range (`app.log`, below);
- uninstalling with *Delete the application data* ticked: nothing may be
  left in `%LOCALAPPDATA%\localSpace` (the one step CI cannot tick).

**Then run the message script there and read what it says**, not that it
ended. It needs Node.js on that machine (nodejs.org, the LTS installer) and
the repository's `scripts/message-script.mjs`. Close the app first. In
PowerShell:

```powershell
$cfg = "$env:TEMP\dryrun.toml"
$root = ($env:LOCALAPPDATA -replace '\\', '/') + '/localSpace'
"[server]`nbind = `"127.0.0.1:8470`"`n[storage]`nroot = `"$root`"" | Set-Content $cfg
& "$env:LOCALAPPDATA\Programs\localSpace\localspace.exe" serve --config $cfg --personal --token dryrun
```

and in a second PowerShell window, with the model the first start
recommended (its id is in `app.log`, on the line "first run: … is
recommended"):

```powershell
node scripts\message-script.mjs http://127.0.0.1:8470 dryrun qwen2.5-7b-instruct-q4_k_m --out dryrun.md
```

**Read the `app.log` that the dry run wrote as if a stranger had sent it**:
from that file alone, which machine was it, what was it offered and why, and
how fast did it go? If that cannot be told, the log is not finished, and
Monday depends on it more than on anything else.

Send `dryrun.md` and `app.log` to the builder. **A plain go or no-go before
the end of Sunday**, and on Monday morning one clean package build from a
head that has not changed since.

## Reading an `app.log` that comes back

`%LOCALAPPDATA%\localSpace\logs\app.log` holds, a line an event, what we
would have seen over a shoulder. Nothing a person typed and nothing they
were answered is in it, and their folder's name is written `~`.

| The line begins | What it tells |
|---|---|
| `localSpace 0.1.0 (…)` | the build they ran |
| `the operating system:` | which Windows |
| `the computer as found:` | the card as the engine names it, its memory, what other programs held of it, whether the card table knows it; the memory, how fast it copied; the processor; the free disk |
| `first run: … is recommended, estimated at …` | what they were offered, and on what estimate |
| `models: the download of … begins` / `is done and checked` / `stopped` | how much was already there, how long it took, at what rate, or why it stopped |
| `models: … was found on this computer` | a file that was already there was taken as the published one |
| `fit: … layers on the card` | the plan the model was started with |
| `engine: started … with -c … -ngl … --device …` | every start of the engine, with its flags |
| `fit: … spilled … now N` / `engine: … did not load … trying again` | every step back from a plan that did not hold |
| `engine: … read the prompt's stable part (… tokens) in … s` | the warm-up, or that it did not finish |
| `engine: … ready … after … s` | how long the person waited for the model |
| `answer: step N, first piece after … s, … tokens in … s (… tokens a second while writing), prompt of … tokens, tools asked for: …` | every answer's measure: compare the rate (× 0.75 for words) with the range promised on the first run |
| `shown to the person:` | every warning and error they saw, word for word |
| `hardware: the memory measured at …, which no memory does` | the measurement failed and a careful figure stood in: tell the builder |

A recommendation that surprises is explained by the first four lines. "It
felt slow" is checked against the `answer:` lines.
